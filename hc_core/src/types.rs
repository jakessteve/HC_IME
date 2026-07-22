use std::os::raw::c_char;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HC_State {
    pub composition_string: *const u16,
    pub length: usize,
    pub status_flag: i32,
    pub error_code: i32,
    pub spell_check_status: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HC_ComposeRequest {
    pub onset: *const c_char,
    pub medial: *const c_char,
    pub nucleus: *const c_char,
    pub coda: *const c_char,
    pub tone: i32,
    pub trigger_case: *const c_char,
    pub raw_input: *const c_char,
    pub legacy_tone: u8,
    pub boundary: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HC_RehydrateRequest {
    pub committed_word: *const c_char,
    pub input_mode: i32,
    pub trigger_kind: i32,
    pub trigger_value: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HC_KeyRequest {
    pub kind: i32,
    pub text: *const c_char,
    pub input_mode: i32,
    pub legacy_tone: u8,
    pub spell_check: u8,
    pub auto_restore: u8,
    pub quick_consonants: u8,
    pub english_protection: u8,
    pub macro_in_english: u8,
    pub esc_restore_raw: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HC_KeyResult {
    pub state: HC_State,
    pub handled: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HC_Utf8KeyResult {
    pub composition_string: *const c_char,
    pub length: usize,
    pub status_flag: i32,
    pub error_code: i32,
    pub spell_check_status: i32,
    pub handled: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HC_CandidateChar {
    pub utf8: [u8; 5],
    pub byte_len: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HC_HanNomResult {
    pub status_flag: i32,
    pub error_code: i32,
    pub reading: [u8; 256],
    pub reading_len: u16,
    pub candidates: *const HC_CandidateChar,
    pub candidate_count: u16,
    pub page: u16,
    pub total_candidates: u16,
    pub has_more: u8,
    pub handled: u8,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NomPhase {
    #[default]
    Reading = 0,
    Candidate = 1,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HCSpellCheckStatus {
    Valid = 0,
    Invalid = 1,
    EnglishFallback = 2,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HCStatusFlag {
    InProgress = 0,
    Commit = 1,
    EnglishFallback = 2,
    ReconversionActive = 3,
    EscRestoredRaw = 4,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HCErrorCode {
    None = 0,
    NullPointer = -1,
    InvalidUtf8 = -2,
    InvalidTone = -3,
    InvalidBoundary = -4,
    InvalidInputMode = -5,
    InvalidEditTrigger = -6,
    MissingRequiredField = -7,
    EngineFailure = -8,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HCKeyKind {
    Printable = 0,
    Backspace = 1,
    Enter = 2,
    Space = 3,
    Boundary = 4,
    Escape = 5,
    Other = 6,
    Undo = 7,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Telex = 0,
    Vni = 1,
    Viqr = 2,
    HanNomTelex = 3,
    HanNomVni = 4,
    HanNomViqr = 5,
}

impl TryFrom<i32> for InputMode {
    type Error = HCErrorCode;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => InputMode::Telex,
            1 => InputMode::Vni,
            2 => InputMode::Viqr,
            3 => InputMode::HanNomTelex,
            4 => InputMode::HanNomVni,
            5 => InputMode::HanNomViqr,
            _ => InputMode::Telex,
        })
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTrigger {
    Cancel = 0,
    TelexW = 1,
    Tone = 2,
    VniDiacritic = 3,
    LiteralNumber = 4,
    Escape = 5,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiacriticKind {
    TelexW = 0,
    Circumflex = 1,
    Horn = 2,
    Breve = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Flat,
    Sac,
    Huyen,
    Hoi,
    Nga,
    Nang,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnglishProtectionLevel {
    Off = 0,
    Soft = 1,
    Hard = 2,
}

impl From<u8> for EnglishProtectionLevel {
    fn from(value: u8) -> Self {
        match value {
            1 => EnglishProtectionLevel::Soft,
            2 => EnglishProtectionLevel::Hard,
            _ => EnglishProtectionLevel::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    PhonotacticFail,
    BloomFilter,
    TrailingConsonant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineDecision {
    Compose(String),
    RawFallback {
        text: String,
        reason: FallbackReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDecision {
    pub text: String,
    pub status: HCStatusFlag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageScores {
    pub vietnamese: i32,
    pub english: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Word,
    Number,
    Boundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSegment {
    pub kind: SegmentKind,
    pub text: String,
}

pub fn parse_tone(value: i32) -> Result<Tone, HCErrorCode> {
    Ok(match value {
        0 => Tone::Flat,
        1 => Tone::Sac,
        2 => Tone::Huyen,
        3 => Tone::Hoi,
        4 => Tone::Nga,
        5 => Tone::Nang,
        _ => return Err(HCErrorCode::InvalidTone),
    })
}

pub fn parse_edit_trigger(kind: i32, _value: i32) -> Result<EditTrigger, HCErrorCode> {
    Ok(match kind {
        0 => EditTrigger::Cancel,
        1 => EditTrigger::TelexW,
        2 => EditTrigger::Tone,
        3 => EditTrigger::VniDiacritic,
        4 => EditTrigger::LiteralNumber,
        5 => EditTrigger::Escape,
        _ => return Err(HCErrorCode::InvalidEditTrigger),
    })
}

pub fn key_kind(value: i32) -> Option<HCKeyKind> {
    Some(match value {
        0 => HCKeyKind::Printable,
        1 => HCKeyKind::Backspace,
        2 => HCKeyKind::Enter,
        3 => HCKeyKind::Space,
        4 => HCKeyKind::Boundary,
        5 => HCKeyKind::Escape,
        6 => HCKeyKind::Other,
        7 => HCKeyKind::Undo,
        _ => return None,
    })
}
