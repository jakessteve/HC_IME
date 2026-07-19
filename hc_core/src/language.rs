use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::types::{ContextSegment, InputMode, LanguageScores, SegmentKind, Tone};
use crate::vowel::{has_vietnamese_mark, strip_marks_ascii_lower, vowel_signature};

const SCORE_SPELLCHECK_VI: i32 = 3;
const SCORE_DICT_VI: i32 = 4;
const SCORE_MARK_VI: i32 = 2;
const SCORE_TERMINAL_TRIGGER: i32 = 4;
const SCORE_NON_ASCII: i32 = 3;
const SCORE_DICT_EN: i32 = 8;
const SCORE_ENGLISH_SUFFIX: i32 = 2;
const SCORE_CODE_SHAPE: i32 = 5;
const SCORE_INVALID_KEY: i32 = 4;
const SCORE_ASCII_ALPHA: i32 = 1;

pub fn language_scores(
    raw: &str,
    rendered: &str,
    mode: InputMode,
    spell_check: bool,
) -> LanguageScores {
    let raw_lower = raw.to_ascii_lowercase();
    let rendered_key = strip_marks_ascii_lower(rendered);
    let raw_shape = raw_base_for_vietnamese_shape(raw, mode);
    let raw_shape_key = strip_marks_ascii_lower(&raw_shape);

    let mut vietnamese = 0;
    let mut english = 0;

    if !spell_check {
        vietnamese += SCORE_MARK_VI;
    } else if is_valid_vietnamese_word(rendered) {
        vietnamese += SCORE_SPELLCHECK_VI;
    }
    if spell_check && is_dictionary_vietnamese_word(&rendered_key) {
        vietnamese += SCORE_DICT_VI;
    }
    if has_vietnamese_mark(rendered) {
        vietnamese += SCORE_MARK_VI;
    }
    if is_terminal_vietnamese_trigger(raw, mode) {
        vietnamese += SCORE_TERMINAL_TRIGGER;
    }
    if !raw.is_ascii() {
        vietnamese += SCORE_NON_ASCII;
    }

    if is_known_english_word(&raw_lower) {
        english += SCORE_DICT_EN;
    }
    if has_english_suffix(&raw_lower) {
        english += SCORE_ENGLISH_SUFFIX;
    }
    if has_code_shape(raw, mode) {
        english += SCORE_CODE_SHAPE;
    }
    if spell_check && !raw_shape_key.is_empty() && !is_valid_vietnamese_key(&raw_shape_key) {
        english += SCORE_INVALID_KEY;
    }
    if rendered != raw && raw.chars().all(|ch| ch.is_ascii_alphabetic()) {
        english += SCORE_ASCII_ALPHA;
    }

    LanguageScores {
        vietnamese,
        english,
    }
}

pub fn segment_context(input: &str) -> Vec<ContextSegment> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_kind: Option<SegmentKind> = None;

    for ch in input.chars() {
        let kind = if ch.is_alphabetic() {
            SegmentKind::Word
        } else if ch.is_ascii_digit() {
            SegmentKind::Number
        } else {
            SegmentKind::Boundary
        };

        if current_kind.is_some_and(|value| value != kind) {
            segments.push(ContextSegment {
                kind: current_kind.unwrap(),
                text: current,
            });
            current = String::new();
        }
        current_kind = Some(kind);
        current.push(ch);
    }

    if let Some(kind) = current_kind {
        segments.push(ContextSegment {
            kind,
            text: current,
        });
    }
    segments
}

fn raw_base_for_vietnamese_shape(raw: &str, mode: InputMode) -> String {
    let mut chars: Vec<char> = raw.chars().collect();
    match mode {
        InputMode::Telex => {
            while raw_has_terminal_telex_trigger(&chars) {
                chars.pop();
            }
        }
        InputMode::Vni => {
            while chars.last().is_some_and(|last| last.is_ascii_digit()) {
                chars.pop();
            }
        }
        InputMode::Viqr => {
            if chars.last().is_some_and(|last| is_viqr_trigger(*last)) {
                chars.pop();
            }
        }
    }
    chars.into_iter().collect()
}

fn is_terminal_vietnamese_trigger(raw: &str, mode: InputMode) -> bool {
    let chars: Vec<char> = raw.chars().collect();
    let Some(&last) = chars.last() else {
        return false;
    };
    match mode {
        InputMode::Telex => raw_has_terminal_telex_trigger(&chars),
        InputMode::Vni => matches!(last, '1'..='9'),
        InputMode::Viqr => matches!(last, '\'' | '`' | '?' | '~' | '.'),
    }
}

fn raw_has_terminal_telex_trigger(chars: &[char]) -> bool {
    let Some(&last) = chars.last() else {
        return false;
    };
    if matches!(
        last,
        's' | 'S' | 'f' | 'F' | 'r' | 'R' | 'x' | 'X' | 'j' | 'J' | 'w' | 'W' | 'z' | 'Z'
    ) {
        return true;
    }

    if chars.len() < 2 {
        return false;
    }
    let previous = chars[chars.len() - 2];
    let trigger = last.to_ascii_lowercase();
    matches!(trigger, 'a' | 'e' | 'o' | 'd') && previous.to_ascii_lowercase() == trigger
}

fn has_english_suffix(word: &str) -> bool {
    word.len() > 4
        && [
            "ing", "ed", "er", "ly", "tion", "ment", "ness", "able", "ible", "ous",
        ]
        .iter()
        .any(|suffix| word.ends_with(suffix))
}

fn has_code_shape(raw: &str, mode: InputMode) -> bool {
    if raw.contains('_') || raw.contains("::") || raw.contains("->") || raw.contains('/') {
        return true;
    }
    match mode {
        InputMode::Vni => false,
        _ => raw.chars().any(|ch| ch.is_ascii_digit()),
    }
}

pub fn is_known_english_word(word: &str) -> bool {
    ENGLISH_WORDS.contains(&word)
        || external_english_dictionary().is_some_and(|dictionary| dictionary.contains(word))
}

#[derive(Default)]
struct DictionaryCache {
    paths: Vec<PathBuf>,
    dictionary: Option<Arc<HashSet<String>>>,
}

fn load_cached_dictionary(
    cache: &Mutex<DictionaryCache>,
    paths: Vec<PathBuf>,
    loader: fn(&[PathBuf]) -> Option<HashSet<String>>,
) -> Option<Arc<HashSet<String>>> {
    let mut cache = cache.lock().unwrap();
    if cache.paths != paths {
        cache.paths = paths.clone();
        cache.dictionary = loader(&paths).map(Arc::new);
    }
    cache.dictionary.clone()
}

static EXTERNAL_ENGLISH_DICTIONARY: OnceLock<Mutex<DictionaryCache>> = OnceLock::new();

fn external_english_dictionary() -> Option<Arc<HashSet<String>>> {
    let cache = EXTERNAL_ENGLISH_DICTIONARY.get_or_init(|| Mutex::new(DictionaryCache::default()));
    load_cached_dictionary(
        cache,
        english_dictionary_paths(),
        load_external_english_dictionary,
    )
}

fn load_external_english_dictionary(paths: &[PathBuf]) -> Option<HashSet<String>> {
    for path in paths {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        let words: HashSet<String> = contents
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .filter(|word| !word.is_empty() && !word.starts_with('#'))
            .map(|word| word.trim().to_ascii_lowercase())
            .filter(|word| {
                word.chars()
                    .all(|ch| ch.is_ascii_alphabetic() || ch == '\'')
            })
            .collect();
        if !words.is_empty() {
            return Some(words);
        }
    }
    None
}

fn english_dictionary_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("HC_IME_EN_DICT") {
        paths.push(PathBuf::from(path));
    }
    paths.push(PathBuf::from("/usr/share/dict/words"));
    paths.push(PathBuf::from("/usr/local/share/dict/words"));
    paths
}

const ENGLISH_WORDS: &[&str] = &[
    "about",
    "access",
    "account",
    "action",
    "active",
    "address",
    "after",
    "again",
    "agent",
    "agents",
    "already",
    "always",
    "android",
    "answer",
    "api",
    "app",
    "apply",
    "archive",
    "around",
    "async",
    "audit",
    "available",
    "back",
    "backspace",
    "base",
    "before",
    "between",
    "block",
    "board",
    "branch",
    "browser",
    "buffer",
    "build",
    "button",
    "call",
    "camera",
    "cancel",
    "cargo",
    "center",
    "change",
    "channel",
    "chat",
    "check",
    "class",
    "clear",
    "click",
    "cli",
    "close",
    "code",
    "color",
    "command",
    "comment",
    "commit",
    "computer",
    "config",
    "confirm",
    "connect",
    "contact",
    "content",
    "context",
    "control",
    "copy",
    "core",
    "cover",
    "create",
    "current",
    "custom",
    "dashboard",
    "data",
    "database",
    "debug",
    "default",
    "define",
    "delete",
    "deploy",
    "design",
    "desktop",
    "detail",
    "device",
    "different",
    "digital",
    "direct",
    "disable",
    "display",
    "docs",
    "document",
    "done",
    "down",
    "download",
    "draft",
    "driver",
    "drop",
    "edit",
    "effect",
    "email",
    "emoji",
    "enable",
    "end",
    "engine",
    "english",
    "enter",
    "error",
    "event",
    "every",
    "example",
    "export",
    "extend",
    "extra",
    "facebook",
    "false",
    "fast",
    "feature",
    "fcitx",
    "fcitx5",
    "field",
    "file",
    "filter",
    "final",
    "find",
    "finish",
    "first",
    "fixed",
    "float",
    "folder",
    "follow",
    "font",
    "force",
    "forgot",
    "form",
    "format",
    "forward",
    "found",
    "frame",
    "free",
    "from",
    "front",
    "full",
    "function",
    "general",
    "generate",
    "get",
    "git",
    "github",
    "global",
    "good",
    "google",
    "group",
    "guide",
    "handle",
    "hash",
    "header",
    "hello",
    "help",
    "here",
    "hidden",
    "history",
    "home",
    "host",
    "how",
    "https",
    "icon",
    "image",
    "ime",
    "import",
    "include",
    "index",
    "info",
    "inject",
    "input",
    "insert",
    "inside",
    "install",
    "instance",
    "interface",
    "internal",
    "issue",
    "item",
    "javascript",
    "job",
    "join",
    "just",
    "keep",
    "key",
    "keyboard",
    "known",
    "label",
    "language",
    "large",
    "last",
    "latest",
    "layout",
    "left",
    "level",
    "library",
    "light",
    "like",
    "line",
    "link",
    "linux",
    "list",
    "load",
    "local",
    "location",
    "lock",
    "log",
    "logic",
    "login",
    "long",
    "look",
    "loop",
    "lower",
    "macro",
    "main",
    "make",
    "manage",
    "manager",
    "manual",
    "many",
    "mark",
    "master",
    "match",
    "media",
    "memory",
    "menu",
    "merge",
    "message",
    "messages",
    "method",
    "micro",
    "middle",
    "might",
    "minus",
    "miss",
    "mixed",
    "mobile",
    "mode",
    "model",
    "module",
    "monitor",
    "moo",
    "more",
    "mouse",
    "move",
    "much",
    "multi",
    "must",
    "name",
    "native",
    "need",
    "network",
    "never",
    "new",
    "next",
    "node",
    "none",
    "normal",
    "note",
    "nothing",
    "notice",
    "number",
    "object",
    "off",
    "offer",
    "office",
    "offset",
    "often",
    "okay",
    "old",
    "once",
    "online",
    "only",
    "open",
    "opencode",
    "option",
    "order",
    "origin",
    "other",
    "output",
    "outside",
    "over",
    "overflow",
    "own",
    "package",
    "page",
    "panel",
    "parent",
    "parse",
    "part",
    "pass",
    "password",
    "passwords",
    "paste",
    "path",
    "pause",
    "people",
    "perform",
    "phone",
    "photo",
    "picture",
    "place",
    "platform",
    "play",
    "please",
    "plugin",
    "plus",
    "point",
    "pop",
    "port",
    "position",
    "post",
    "power",
    "preedit",
    "press",
    "preview",
    "previous",
    "primary",
    "print",
    "private",
    "process",
    "profile",
    "program",
    "progress",
    "project",
    "promise",
    "property",
    "protect",
    "provide",
    "public",
    "pull",
    "push",
    "put",
    "query",
    "question",
    "queue",
    "quick",
    "quite",
    "random",
    "range",
    "rate",
    "raw",
    "react",
    "read",
    "ready",
    "real",
    "receive",
    "record",
    "reduce",
    "refresh",
    "release",
    "reload",
    "remote",
    "remove",
    "render",
    "repeat",
    "replace",
    "reply",
    "repo",
    "report",
    "request",
    "require",
    "reset",
    "resize",
    "resolve",
    "resource",
    "response",
    "restart",
    "restore",
    "result",
    "return",
    "review",
    "right",
    "role",
    "root",
    "round",
    "route",
    "rule",
    "run",
    "runtime",
    "rust",
    "safe",
    "same",
    "save",
    "scale",
    "scan",
    "screen",
    "script",
    "scroll",
    "search",
    "section",
    "secure",
    "select",
    "self",
    "send",
    "server",
    "service",
    "session",
    "set",
    "setting",
    "setup",
    "share",
    "shell",
    "shift",
    "short",
    "should",
    "show",
    "side",
    "sign",
    "simple",
    "single",
    "site",
    "size",
    "skip",
    "sleep",
    "slide",
    "slow",
    "small",
    "smart",
    "soft",
    "some",
    "sort",
    "sound",
    "source",
    "space",
    "special",
    "split",
    "stack",
    "stage",
    "standard",
    "start",
    "state",
    "static",
    "status",
    "stay",
    "step",
    "still",
    "stop",
    "store",
    "story",
    "stream",
    "string",
    "strong",
    "struct",
    "style",
    "submit",
    "success",
    "support",
    "sure",
    "suspend",
    "swap",
    "switch",
    "sync",
    "system",
    "table",
    "tag",
    "take",
    "target",
    "task",
    "team",
    "template",
    "terminal",
    "test",
    "text",
    "that",
    "theme",
    "then",
    "there",
    "these",
    "thing",
    "think",
    "this",
    "thread",
    "through",
    "time",
    "title",
    "today",
    "toggle",
    "token",
    "tool",
    "tools",
    "top",
    "total",
    "touch",
    "trace",
    "track",
    "transfer",
    "trigger",
    "true",
    "trust",
    "turn",
    "type",
    "under",
    "undo",
    "unicode",
    "unit",
    "unix",
    "unknown",
    "unlock",
    "until",
    "update",
    "upgrade",
    "upload",
    "upper",
    "url",
    "use",
    "user",
    "using",
    "valid",
    "value",
    "version",
    "video",
    "view",
    "virtual",
    "visible",
    "visual",
    "wait",
    "warning",
    "watch",
    "web",
    "welcome",
    "what",
    "when",
    "where",
    "which",
    "while",
    "widget",
    "will",
    "window",
    "with",
    "word",
    "work",
    "workflow",
    "workspace",
    "world",
    "wrap",
    "write",
    "wrong",
    "zero",
];

pub fn is_dictionary_vietnamese_word(word: &str) -> bool {
    is_known_vietnamese_word(word)
        || external_vietnamese_dictionary().is_some_and(|dictionary| dictionary.contains(word))
}

fn is_known_vietnamese_word(word: &str) -> bool {
    VIETNAMESE_WORDS.contains(&word)
}

static EXTERNAL_VIETNAMESE_DICTIONARY: OnceLock<Mutex<DictionaryCache>> = OnceLock::new();

pub fn external_vietnamese_dictionary() -> Option<Arc<HashSet<String>>> {
    let cache =
        EXTERNAL_VIETNAMESE_DICTIONARY.get_or_init(|| Mutex::new(DictionaryCache::default()));
    load_cached_dictionary(
        cache,
        vietnamese_dictionary_paths(),
        load_external_vietnamese_dictionary,
    )
}

fn load_external_vietnamese_dictionary(paths: &[PathBuf]) -> Option<HashSet<String>> {
    for path in paths {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        let words: HashSet<String> = contents
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .filter(|word| !word.is_empty() && !word.starts_with('#'))
            .map(strip_marks_ascii_lower)
            .filter(|word| !word.is_empty())
            .collect();
        if !words.is_empty() {
            return Some(words);
        }
    }
    None
}

fn vietnamese_dictionary_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("HC_IME_VI_DICT") {
        paths.push(PathBuf::from(path));
    }
    paths.push(PathBuf::from("/usr/share/fcitx5/bamboo/vietnamese.cm.dict"));
    paths.push(PathBuf::from(
        "/usr/local/share/fcitx5/bamboo/vietnamese.cm.dict",
    ));
    paths
}

const VIETNAMESE_WORDS: &[&str] = &[
    "ai", "anh", "ban", "bao", "biet", "bo", "cach", "cac", "cam", "can", "chao", "cho", "chu",
    "chung", "co", "con", "cong", "cua", "cuoc", "da", "dang", "day", "de", "den", "di", "dieu",
    "do", "duoc", "em", "go", "hai", "hanh", "hay", "hen", "hien", "hoa", "hoc", "hoi", "khac",
    "khi", "khong", "la", "lai", "lam", "lap", "len", "lich", "luat", "ma", "mai", "minh", "mot",
    "muon", "nam", "nay", "ngay", "nghe", "nghi", "nghia", "ngon", "nguoi", "nguyen", "nha",
    "nhan", "nhat", "nhieu", "nhung", "noi", "nuoc", "phai", "phan", "ra", "rang", "rat", "roi",
    "rut", "sau", "se", "song", "ta", "tai", "tam", "tat", "ten", "tet", "the", "thi", "thich",
    "tho", "thoi", "thu", "thuong", "tieng", "toi", "trong", "truong", "tu", "tuan", "tung",
    "tuyen", "va", "van", "ve", "viet", "viec", "voi", "vui", "xin", "yeu",
];

pub fn is_valid_vietnamese_word(word: &str) -> bool {
    let segments = segment_context(word);
    let word_segments: Vec<&ContextSegment> = segments
        .iter()
        .filter(|segment| segment.kind == SegmentKind::Word)
        .collect();
    if word_segments.len() != 1
        || segments
            .iter()
            .any(|segment| segment.kind == SegmentKind::Number)
    {
        return false;
    }

    let key = strip_marks_ascii_lower(&word_segments[0].text);
    let tone = word_tone(&word_segments[0].text);
    if let Some((_, coda)) = parse_vietnamese_key(&key) {
        if matches!(coda, "c" | "ch" | "p" | "t")
            && matches!(tone, Tone::Huyen | Tone::Hoi | Tone::Nga)
        {
            return false;
        }
        return true;
    }

    is_dictionary_vietnamese_word(&key)
}

pub fn is_valid_vietnamese_key(key: &str) -> bool {
    parse_vietnamese_key(key).is_some()
}

fn parse_vietnamese_key(key: &str) -> Option<(&str, &str)> {
    if key.is_empty() || !key.chars().all(|ch| ch.is_ascii_lowercase()) {
        return None;
    }

    let rest = VIETNAMESE_ONSETS
        .iter()
        .find_map(|onset| key.strip_prefix(onset))
        .unwrap_or(key);

    if rest.is_empty() {
        return None;
    }

    for coda in VIETNAMESE_CODAS {
        if let Some(cluster) = rest.strip_suffix(coda) {
            if !cluster.is_empty() && VIETNAMESE_VOWEL_CLUSTERS.contains(&cluster) {
                return Some((cluster, coda));
            }
        }
    }

    if VIETNAMESE_VOWEL_CLUSTERS.contains(&rest) {
        return Some((rest, ""));
    }

    None
}

fn word_tone(word: &str) -> Tone {
    word.chars()
        .filter_map(vowel_signature)
        .find_map(|(_, _, tone)| (tone != Tone::Flat).then_some(tone))
        .unwrap_or(Tone::Flat)
}

const VIETNAMESE_ONSETS: &[&str] = &[
    "ngh", "ch", "gh", "gi", "kh", "ng", "nh", "ph", "qu", "th", "tr", "b", "c", "d", "g", "h",
    "k", "l", "m", "n", "p", "r", "s", "t", "v", "x",
];

const VIETNAMESE_CODAS: &[&str] = &["ch", "ng", "nh", "c", "m", "n", "p", "t"];

const VIETNAMESE_VOWEL_CLUSTERS: &[&str] = &[
    "a", "ai", "ao", "au", "ay", "e", "eo", "eu", "i", "ia", "ie", "ieu", "iu", "o", "oa", "oai",
    "oao", "oay", "oe", "oeo", "oi", "u", "ua", "uai", "uay", "ue", "ueo", "ui", "uo", "uoi",
    "uou", "uy", "uya", "uye", "uyu", "uu", "y", "ye", "yeu",
];

pub fn is_hard_english_raw_start(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    if chars.len() < 2 {
        return false;
    }
    let impossible_starts = [
        "cl", "cr", "br", "bl", "dr", "fr", "fl", "gr", "gl", "pr", "pl", "sm", "sn", "sp", "sw",
        "st", "sc", "sk", "sl", "wh", "wr", "kn", "pn", "ps",
    ];
    let two_char: String = chars[..2].iter().collect();
    if impossible_starts.iter().any(|&s| two_char == s) {
        return true;
    }
    false
}

pub fn is_soft_english_pattern(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    if chars.len() >= 2 && chars[0] == 'y' && matches!(chars[1], 'a' | 'e' | 'i' | 'o' | 'u') {
        return true;
    }
    false
}

pub fn is_viqr_trigger(ch: char) -> bool {
    matches!(ch, '\'' | '`' | '?' | '~' | '.' | '^' | '+' | '(')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("hcime-{name}-{}-{stamp}.dict", std::process::id()))
    }

    #[test]
    fn cached_dictionary_reloads_when_paths_change() {
        let english_path_one = unique_temp_path("english-one");
        let english_path_two = unique_temp_path("english-two");
        let vietnamese_path_one = unique_temp_path("vietnamese-one");
        let vietnamese_path_two = unique_temp_path("vietnamese-two");

        fs::write(&english_path_one, "alpha\n").expect("write english path one");
        fs::write(&english_path_two, "beta\n").expect("write english path two");
        fs::write(&vietnamese_path_one, "sắc\n").expect("write vietnamese path one");
        fs::write(&vietnamese_path_two, "mưa\n").expect("write vietnamese path two");

        let english_cache = Mutex::new(DictionaryCache::default());
        let first_english = load_cached_dictionary(
            &english_cache,
            vec![english_path_one.clone()],
            load_external_english_dictionary,
        )
        .expect("load first english dictionary");
        assert!(first_english.contains("alpha"));
        assert!(!first_english.contains("beta"));

        let second_english = load_cached_dictionary(
            &english_cache,
            vec![english_path_two.clone()],
            load_external_english_dictionary,
        )
        .expect("load second english dictionary");
        assert!(second_english.contains("beta"));
        assert!(!second_english.contains("alpha"));

        let vietnamese_cache = Mutex::new(DictionaryCache::default());
        let first_vietnamese = load_cached_dictionary(
            &vietnamese_cache,
            vec![vietnamese_path_one.clone()],
            load_external_vietnamese_dictionary,
        )
        .expect("load first vietnamese dictionary");
        assert!(first_vietnamese.contains("sac"));
        assert!(!first_vietnamese.contains("mua"));

        let second_vietnamese = load_cached_dictionary(
            &vietnamese_cache,
            vec![vietnamese_path_two.clone()],
            load_external_vietnamese_dictionary,
        )
        .expect("load second vietnamese dictionary");
        assert!(second_vietnamese.contains("mua"));
        assert!(!second_vietnamese.contains("sac"));

        let _ = fs::remove_file(english_path_one);
        let _ = fs::remove_file(english_path_two);
        let _ = fs::remove_file(vietnamese_path_one);
        let _ = fs::remove_file(vietnamese_path_two);
    }
}
