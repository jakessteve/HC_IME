use crate::vowel::strip_all_marks;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
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

#[derive(Debug)]
pub struct EmbeddedNomDict {
    entries: HashMap<String, Vec<char>>,
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
            entries.insert(reading, candidates);
        }

        Ok(EmbeddedNomDict { entries })
    }

    pub fn lookup(&self, reading: &str) -> Vec<char> {
        let lower = reading.trim().to_lowercase();
        if lower.is_empty() {
            return Vec::new();
        }
        if let Some(found) = self.entries.get(&lower) {
            return found.clone();
        }
        let input_toneless = strip_all_marks(&lower);
        let mut fallback = Vec::new();
        for (k, v) in &self.entries {
            if strip_all_marks(k) == input_toneless {
                for &ch in v {
                    if !fallback.contains(&ch) {
                        fallback.push(ch);
                    }
                }
            }
        }
        fallback
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
                if let Ok(data) = std::fs::read(&path) {
                    if let Ok(dict) = EmbeddedNomDict::from_binary(&data) {
                        return Ok(Arc::new(dict));
                    }
                }
            }
            EmbeddedNomDict::from_binary(EMBEDDED_DICT_DATA).map(Arc::new)
        })
        .clone()
}

pub fn get_global_phrase_dict() -> Result<Arc<EmbeddedPhraseDict>, DictError> {
    GLOBAL_PHRASE_DICT
        .get_or_init(|| EmbeddedPhraseDict::from_binary(EMBEDDED_PHRASE_DICT_DATA).map(Arc::new))
        .clone()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhraseHistoryEntry {
    pub reading: String,
    pub glyphs: String,
    pub count: u32,
    pub last_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PhraseHistory {
    pub entries: Vec<PhraseHistoryEntry>,
}

pub fn default_history_path() -> PathBuf {
    if let Ok(value) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(value).join("hcime/han_nom_history.json");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/state/hcime/han_nom_history.json");
    }
    PathBuf::from("han_nom_history.json")
}

impl PhraseHistory {
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
    pub fn score(&self, reading: &str, glyphs: &str) -> (u32, u64) {
        self.entries
            .iter()
            .find(|item| item.reading == reading && item.glyphs == glyphs)
            .map(|item| (item.count, item.last_used))
            .unwrap_or((0, 0))
    }
    pub fn record(&mut self, reading: &str, glyphs: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|item| item.reading == reading && item.glyphs == glyphs)
        {
            entry.count = entry.count.saturating_add(1);
            entry.last_used = now;
        } else {
            self.entries.push(PhraseHistoryEntry {
                reading: reading.to_owned(),
                glyphs: glyphs.to_owned(),
                count: 1,
                last_used: now,
            });
        }
        if self.entries.len() > 2048 {
            self.entries
                .sort_by_key(|entry| std::cmp::Reverse(entry.last_used));
            self.entries.truncate(2048);
        }
    }
    pub fn persist(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
            }
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(self).unwrap_or_default())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        }
        fs::rename(tmp, path)
    }
    pub fn reset(&mut self, path: &Path) {
        self.entries.clear();
        let _ = fs::remove_file(path);
    }
}
