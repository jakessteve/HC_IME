use crate::vowel::strip_all_marks;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictError {
    NotFound = 1,
    InvalidMagic = 2,
    InvalidVersion = 3,
    Corrupted = 4,
}

pub static EMBEDDED_DICT_DATA: &[u8] = include_bytes!("../data/han_nom_dict.bin");
static GLOBAL_DICT: OnceLock<Result<Arc<EmbeddedNomDict>, DictError>> = OnceLock::new();

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
