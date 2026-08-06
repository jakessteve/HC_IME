use crate::vowel::strip_all_marks;
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictError {
    NotFound = 1,
    InvalidMagic = 2,
    InvalidVersion = 3,
    Corrupted = 4,
}

pub static EMBEDDED_DICT_DATA: &[u8] = include_bytes!("../data/han_nom_dict.bin");
pub static EMBEDDED_PHRASE_DICT_DATA: &[u8] = include_bytes!("../data/han_nom_phrase_dict.bin");
static GLOBAL_DICT: OnceLock<Result<Arc<EmbeddedNomDict>, DictError>> = OnceLock::new();
static GLOBAL_PHRASE_DICT: OnceLock<Result<Arc<EmbeddedPhraseDict>, DictError>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseEntry {
    pub reading: String,
    pub glyphs: String,
    pub system_rank: u32,
}

#[derive(Debug)]
pub struct EmbeddedPhraseDict {
    entries: Vec<PhraseEntry>,
}

pub const USER_PHRASE_MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const USER_PHRASE_MAX_ROWS: usize = 50_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserPhraseLoadSummary {
    pub loaded: usize,
    pub malformed: usize,
    pub invalid_utf8: bool,
    pub rejected_oversize: bool,
    pub unreadable: bool,
}

fn open_regular_user_phrase_file(path: &Path) -> Result<File, UserPhraseLoadSummary> {
    let mut summary = UserPhraseLoadSummary::default();
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NONBLOCK keeps a path swapped to a FIFO from stalling the IME.
        OpenOptions::new()
            .read(true)
            .custom_flags(0o4000)
            .open(path)
    };
    #[cfg(not(unix))]
    let file = File::open(path);
    let Ok(file) = file else {
        summary.unreadable = true;
        return Err(summary);
    };
    let Ok(metadata) = file.metadata() else {
        summary.unreadable = true;
        return Err(summary);
    };
    if !metadata.file_type().is_file() {
        summary.unreadable = true;
        return Err(summary);
    }
    if metadata.len() > USER_PHRASE_MAX_BYTES {
        summary.rejected_oversize = true;
        return Err(summary);
    }
    Ok(file)
}

fn is_cjk_scalar(ch: char) -> bool {
    matches!(ch as u32,
        0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff |
        0x20000..=0x2fa1f | 0x30000..=0x323af)
}

/// Load an optional user dictionary outside the typing path. Invalid files
/// fail closed so the bundled dictionary remains the only source.
pub fn load_user_phrase_dict(path: &Path) -> (Vec<PhraseEntry>, UserPhraseLoadSummary) {
    let file = match open_regular_user_phrase_file(path) {
        Ok(file) => file,
        Err(summary) => return (Vec::new(), summary),
    };
    let mut summary = UserPhraseLoadSummary::default();
    let mut bytes = Vec::new();
    let mut reader = file.take(USER_PHRASE_MAX_BYTES + 1);
    if reader.read_to_end(&mut bytes).is_err() {
        summary.unreadable = true;
        return (Vec::new(), summary);
    }
    if bytes.len() as u64 > USER_PHRASE_MAX_BYTES {
        summary.rejected_oversize = true;
        return (Vec::new(), summary);
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        summary.invalid_utf8 = true;
        return (Vec::new(), summary);
    };
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut rows = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        rows += 1;
        if rows > USER_PHRASE_MAX_ROWS {
            summary.rejected_oversize = true;
            return (Vec::new(), summary);
        }
        let Some((reading, glyphs)) = line.split_once('\t') else {
            summary.malformed += 1;
            continue;
        };
        if glyphs.contains('\t') {
            summary.malformed += 1;
            continue;
        }
        let reading = normalize_phrase_reading(reading);
        let glyphs = glyphs.trim();
        if reading.split(' ').count() != 2
            || reading.split(' ').any(|part| part.is_empty())
            || !reading
                .split(' ')
                .all(crate::language::is_valid_vietnamese_word)
            || glyphs.chars().count() != 2
            || !glyphs.chars().all(is_cjk_scalar)
        {
            summary.malformed += 1;
            continue;
        }
        let pair = (reading.clone(), glyphs.to_owned());
        if seen.insert(pair) {
            entries.push(PhraseEntry {
                reading,
                glyphs: glyphs.to_owned(),
                system_rank: entries.len() as u32,
            });
        }
    }
    summary.loaded = entries.len();
    (entries, summary)
}

pub fn normalize_phrase_reading(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl EmbeddedPhraseDict {
    pub fn from_binary(data: &[u8]) -> Result<Self, DictError> {
        if data.len() < 12 {
            return Err(DictError::Corrupted);
        }
        if &data[..4] != b"HNPH" {
            return Err(DictError::InvalidMagic);
        }
        if data[4] != 1 {
            return Err(DictError::InvalidVersion);
        }
        let count = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let mut idx = 12;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let read_u16 = |data: &[u8], idx: &mut usize| -> Result<usize, DictError> {
                if *idx + 2 > data.len() {
                    return Err(DictError::Corrupted);
                }
                let value = u16::from_le_bytes(data[*idx..*idx + 2].try_into().unwrap()) as usize;
                *idx += 2;
                Ok(value)
            };
            let reading_len = read_u16(data, &mut idx)?;
            if idx + reading_len > data.len() {
                return Err(DictError::Corrupted);
            }
            let reading = std::str::from_utf8(&data[idx..idx + reading_len])
                .map_err(|_| DictError::Corrupted)?
                .to_owned();
            idx += reading_len;
            let glyph_len = read_u16(data, &mut idx)?;
            if idx + glyph_len + 4 > data.len() {
                return Err(DictError::Corrupted);
            }
            let glyphs = std::str::from_utf8(&data[idx..idx + glyph_len])
                .map_err(|_| DictError::Corrupted)?
                .to_owned();
            idx += glyph_len;
            let system_rank = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap());
            idx += 4;
            entries.push(PhraseEntry {
                reading,
                glyphs,
                system_rank,
            });
        }
        if idx != data.len() {
            return Err(DictError::Corrupted);
        }
        Ok(Self { entries })
    }
    pub fn exact(&self, reading: &str) -> Vec<PhraseEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.reading == reading)
            .cloned()
            .collect()
    }
    pub fn prefix(&self, reading: &str) -> Vec<PhraseEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.reading.starts_with(reading))
            .cloned()
            .collect()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Test-only instrumentation.
///
/// The NOM-12 / PERF-03 / PERF-04 fixes are properties of *how much work
/// happens*, not of wall-clock time, so the regression tests assert on these
/// counters instead of on a stopwatch. Thread-local, because the test harness
/// runs tests in parallel on one process and a shared atomic would let one
/// test see another's keystrokes.
#[cfg(test)]
pub(crate) mod probe {
    use std::cell::Cell;

    thread_local! {
        /// `strip_all_marks` calls made inside `EmbeddedNomDict::lookup`. The
        /// old fallback stripped every one of the 7,079 keys on every miss.
        static TONELESS_STRIPS: Cell<usize> = const { Cell::new(0) };
        static HISTORY_SCORE_CALLS: Cell<usize> = const { Cell::new(0) };
        static PHRASE_REBUILDS: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn reset() {
        TONELESS_STRIPS.with(|cell| cell.set(0));
        HISTORY_SCORE_CALLS.with(|cell| cell.set(0));
        PHRASE_REBUILDS.with(|cell| cell.set(0));
    }

    pub(crate) fn note_toneless_strip() {
        TONELESS_STRIPS.with(|cell| cell.set(cell.get() + 1));
    }

    pub(crate) fn note_history_score_call() {
        HISTORY_SCORE_CALLS.with(|cell| cell.set(cell.get() + 1));
    }

    pub(crate) fn note_phrase_rebuild() {
        PHRASE_REBUILDS.with(|cell| cell.set(cell.get() + 1));
    }

    pub(crate) fn toneless_strips() -> usize {
        TONELESS_STRIPS.with(|cell| cell.get())
    }

    pub(crate) fn history_score_calls() -> usize {
        HISTORY_SCORE_CALLS.with(|cell| cell.get())
    }

    pub(crate) fn phrase_rebuilds() -> usize {
        PHRASE_REBUILDS.with(|cell| cell.get())
    }
}

#[derive(Debug)]
pub struct EmbeddedNomDict {
    entries: HashMap<String, Vec<char>>,
    /// Toneless reading -> the merged, de-duplicated candidates of every entry
    /// that strips down to it. Precomputed at load so the fallback in `lookup`
    /// is a hash probe.
    ///
    /// It also makes the fallback *deterministic*: it used to iterate
    /// `entries`, a `HashMap` whose `RandomState` is reseeded per process, so
    /// the candidate order — and, once the caller truncated the list, the
    /// candidate set — changed on every launch (NOM-05). Buckets are built in
    /// dictionary file order instead.
    toneless: HashMap<String, Vec<char>>,
}

impl EmbeddedNomDict {
    pub fn from_binary(data: &[u8]) -> Result<Self, DictError> {
        if data.len() < 12 {
            return Err(DictError::Corrupted);
        }
        if &data[0..4] != b"HNOM" {
            return Err(DictError::InvalidMagic);
        }
        if data[4] != 0x01 {
            return Err(DictError::InvalidVersion);
        }
        let count = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let mut idx = 12;
        let mut entries = HashMap::with_capacity(count);
        // Readings in file order. `entries` is a HashMap and iterating it to
        // build the toneless index would reintroduce the per-process ordering
        // this index exists to remove.
        let mut order: Vec<String> = Vec::with_capacity(count);

        for _ in 0..count {
            if idx >= data.len() {
                return Err(DictError::Corrupted);
            }
            let r_len = data[idx] as usize;
            idx += 1;
            if idx + r_len > data.len() {
                return Err(DictError::Corrupted);
            }
            let reading = match std::str::from_utf8(&data[idx..idx + r_len]) {
                Ok(s) => s.to_string(),
                Err(_) => return Err(DictError::Corrupted),
            };
            idx += r_len;

            if idx + 2 > data.len() {
                return Err(DictError::Corrupted);
            }
            let c_count = u16::from_le_bytes(data[idx..idx + 2].try_into().unwrap()) as usize;
            idx += 2;

            if idx + c_count * 4 > data.len() {
                return Err(DictError::Corrupted);
            }
            let mut candidates = Vec::with_capacity(c_count);
            for _ in 0..c_count {
                let cp = u32::from_le_bytes(data[idx..idx + 4].try_into().unwrap());
                idx += 4;
                if let Some(ch) = char::from_u32(cp) {
                    candidates.push(ch);
                }
            }
            order.push(reading.clone());
            entries.insert(reading, candidates);
        }

        // Merge in file order. Reading the value back out of `entries` (rather
        // than using the decoded vector directly) keeps a duplicated reading
        // resolving to the same single value the exact lookup would return.
        let mut toneless: HashMap<String, Vec<char>> = HashMap::with_capacity(count);
        for reading in &order {
            let Some(candidates) = entries.get(reading) else {
                continue;
            };
            let bucket = toneless.entry(strip_all_marks(reading)).or_default();
            for &ch in candidates {
                if !bucket.contains(&ch) {
                    bucket.push(ch);
                }
            }
        }

        Ok(EmbeddedNomDict { entries, toneless })
    }

    pub fn lookup(&self, reading: &str) -> Vec<char> {
        let lower = reading.trim().to_lowercase();
        if lower.is_empty() {
            return Vec::new();
        }
        if let Some(found) = self.entries.get(&lower) {
            return found.clone();
        }
        // Every incomplete reading lands here, so this runs 2–4 times per Hán
        // Nôm keystroke. It used to scan all 7,079 entries and allocate a
        // `String` per entry (PERF-03, 440 µs); the index makes it one probe.
        //
        // The merge is unchanged, including the fact that it collapses readings
        // that differ only by đ/d or a vowel shape (NOM-13): `strip_all_marks`
        // is the same key function as before.
        #[cfg(test)]
        probe::note_toneless_strip();
        self.toneless
            .get(&strip_all_marks(&lower))
            .cloned()
            .unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

pub fn get_global_dict() -> Result<Arc<EmbeddedNomDict>, DictError> {
    GLOBAL_DICT
        .get_or_init(|| {
            if let Ok(path) = std::env::var("HC_IME_NOM_DICT") {
                if let Ok(file) = File::open(&path) {
                    if let Ok(mmap) = unsafe { Mmap::map(&file) } {
                        if let Ok(dict) = EmbeddedNomDict::from_binary(&mmap) {
                            eprintln!("[HC_IME] Using nom dict from file: {}", path);
                            return Ok(Arc::new(dict));
                        }
                    }
                }
            }
            for dir in crate::platform::dictionary_paths() {
                let path = dir.join("han_nom_dict.bin");
                if let Ok(file) = File::open(&path) {
                    if let Ok(mmap) = unsafe { Mmap::map(&file) } {
                        if let Ok(dict) = EmbeddedNomDict::from_binary(&mmap) {
                            eprintln!("[HC_IME] Using nom dict from file: {}", path.display());
                            return Ok(Arc::new(dict));
                        }
                    }
                }
            }
            eprintln!("[HC_IME] Using embedded nom dict");
            EmbeddedNomDict::from_binary(EMBEDDED_DICT_DATA).map(Arc::new)
        })
        .clone()
}

pub fn get_global_phrase_dict() -> Result<Arc<EmbeddedPhraseDict>, DictError> {
    GLOBAL_PHRASE_DICT
        .get_or_init(|| {
            if let Ok(path) = std::env::var("HC_IME_NOM_PHRASE_DICT") {
                if let Ok(file) = File::open(&path) {
                    if let Ok(mmap) = unsafe { Mmap::map(&file) } {
                        if let Ok(dict) = EmbeddedPhraseDict::from_binary(&mmap) {
                            eprintln!("[HC_IME] Using phrase dict from file: {}", path);
                            return Ok(Arc::new(dict));
                        }
                    }
                }
            }
            for dir in crate::platform::dictionary_paths() {
                let path = dir.join("han_nom_phrase_dict.bin");
                if let Ok(file) = File::open(&path) {
                    if let Ok(mmap) = unsafe { Mmap::map(&file) } {
                        if let Ok(dict) = EmbeddedPhraseDict::from_binary(&mmap) {
                            eprintln!("[HC_IME] Using phrase dict from file: {}", path.display());
                            return Ok(Arc::new(dict));
                        }
                    }
                }
            }
            eprintln!("[HC_IME] Using embedded phrase dict");
            EmbeddedPhraseDict::from_binary(EMBEDDED_PHRASE_DICT_DATA).map(Arc::new)
        })
        .clone()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhraseHistoryEntry {
    pub reading: String,
    pub glyphs: String,
    pub count: u32,
    pub last_used: u64,
}

/// Learned `(reading, glyphs)` selections.
///
/// `entries` is kept ordered by `(reading, glyphs)`. That is not cosmetic:
/// `score` is called once per candidate by the ranker — up to ~1,000 times on
/// a single keystroke — and a linear scan of the history made keystroke
/// latency scale with how much the user had learned (NOM-12). Anything that
/// touches `entries` must restore the order before returning.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhraseHistory {
    pub entries: Vec<PhraseHistoryEntry>,
}

pub fn default_history_path() -> PathBuf {
    crate::platform::state_dir()
        .map(|p| p.join("han_nom_history.json"))
        .unwrap_or_else(|| PathBuf::from("han_nom_history.json"))
}

#[cfg(not(test))]
static GLOBAL_PHRASE_HISTORY_PATH: OnceLock<PathBuf> = OnceLock::new();

/// The history file every session uses unless the frontend configures its own.
///
/// Caching the *path* is safe — it cannot change while the process runs. What
/// must never be cached is the file's *contents*: a process-wide snapshot of
/// those meant no session ever observed another session's writes, and IMK
/// creates one session per client application (NOM-08).
#[cfg(not(test))]
pub fn get_global_phrase_history_path() -> PathBuf {
    GLOBAL_PHRASE_HISTORY_PATH
        .get_or_init(default_history_path)
        .clone()
}

/// Sessions flush Hán Nôm learning on free, so under `cargo test` the default
/// path is redirected into a temp directory. Without this the suite writes to
/// the developer's real state directory. `default_history_path()` itself stays
/// production-accurate and is asserted on directly.
///
/// It is also per-thread: the harness runs tests in parallel on one process,
/// and a single shared default file would let one test's flush overwrite the
/// history another test is asserting on.
#[cfg(test)]
pub fn get_global_phrase_history_path() -> PathBuf {
    let thread: String = format!("{:?}", std::thread::current().id())
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();
    std::env::temp_dir()
        .join(format!("hcime-test-state-{}", std::process::id()))
        .join(format!("han_nom_history-{thread}.json"))
}

/// Learning is capped so an unbounded file cannot make ranking — and therefore
/// typing — arbitrarily slow.
pub const PHRASE_HISTORY_MAX_ENTRIES: usize = 2048;

fn key_of(entry: &PhraseHistoryEntry) -> (&str, &str) {
    (entry.reading.as_str(), entry.glyphs.as_str())
}

impl PhraseHistory {
    pub fn load(path: &Path) -> Self {
        let mut history: Self = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        // The file is arbitrary input: order it before anything can search it.
        history.restore_key_order();
        // `record` enforces the cap, `load` did not, so a file that grew by any
        // other route (an older build, a hand-edit, a sync conflict) was taken
        // whole and every ranking comparison then walked it (NOM-12).
        history.enforce_cap();
        history
    }
    fn restore_key_order(&mut self) {
        self.entries
            .sort_by(|left, right| key_of(left).cmp(&key_of(right)));
    }
    /// Keeps the most recently used entries, then restores the search order.
    fn enforce_cap(&mut self) {
        if self.entries.len() > PHRASE_HISTORY_MAX_ENTRIES {
            self.entries
                .sort_by_key(|entry| std::cmp::Reverse(entry.last_used));
            self.entries.truncate(PHRASE_HISTORY_MAX_ENTRIES);
            self.restore_key_order();
        }
    }
    fn position(&self, reading: &str, glyphs: &str) -> Result<usize, usize> {
        self.entries
            .binary_search_by(|entry| key_of(entry).cmp(&(reading, glyphs)))
    }
    pub fn score(&self, reading: &str, glyphs: &str) -> (u32, u64) {
        #[cfg(test)]
        probe::note_history_score_call();
        self.position(reading, glyphs)
            .ok()
            .map(|index| {
                let entry = &self.entries[index];
                (entry.count, entry.last_used)
            })
            .unwrap_or((0, 0))
    }
    pub fn record(&mut self, reading: &str, glyphs: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        match self.position(reading, glyphs) {
            Ok(index) => {
                let entry = &mut self.entries[index];
                entry.count = entry.count.saturating_add(1);
                entry.last_used = now;
            }
            Err(index) => self.entries.insert(
                index,
                PhraseHistoryEntry {
                    reading: reading.to_owned(),
                    glyphs: glyphs.to_owned(),
                    count: 1,
                    last_used: now,
                },
            ),
        }
        self.enforce_cap();
    }
    pub fn persist(&self, path: &Path) -> std::io::Result<()> {
        // A bare filename ("history.json") has an *empty* parent, not none:
        // creating or chmod-ing "" fails ENOENT and would abort the write before
        // the temp file is ever produced. Skip the directory step there.
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Best-effort: the history file itself is locked to 0600 below,
                // so a directory we are not allowed to chmod (shared or owned by
                // someone else) must not cost the user their learning data.
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
            #[cfg(not(unix))]
            {
                // Permissions are best-effort; no-op on non-Unix platforms.
            }
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(self).unwrap_or_default())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        }
        #[cfg(not(unix))]
        {
            // Permissions are best-effort; no-op on non-Unix platforms.
        }
        fs::rename(tmp, path)
    }
    pub fn reset(&mut self, path: &Path) {
        self.entries.clear();
        let _ = fs::remove_file(path);
    }
}
