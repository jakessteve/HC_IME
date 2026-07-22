use std::collections::HashMap;
use std::ffi::c_char;
use std::time::Instant;

use crate::han_nom::{default_history_path, PhraseHistory};
use crate::language::{is_viqr_trigger, language_scores};
use crate::quick_consonants;
use crate::types::{
    CommitDecision, EnglishProtectionLevel, HCSpellCheckStatus, HCStatusFlag, HC_CandidateChar,
    HC_HanNomCandidateText, HC_State, InputMode, NomPhase,
};

#[derive(Debug, Clone)]
pub struct NomTextCandidate {
    pub text: String,
    pub reading: String,
    pub kind: u8,
    pub system_rank: u32,
}
use crate::vowel::strip_all_marks;

pub const EDIT_TIMEOUT_MS: u128 = 1500;

#[derive(Debug, Clone)]
pub struct Session {
    pub mode: InputMode,
    pub legacy_tone: bool,
    pub buffer: String,
    pub raw_buffer: String,
    pub last_commit: String,
    pub last_raw: String,
    pub reconversion_active: bool,
    pub spell_check: bool,
    pub auto_restore: bool,
    pub last_commit_time: Option<Instant>,
    pub previous_buffer: String,
    pub previous_raw_buffer: String,
    pub last_spell_check_status: HCSpellCheckStatus,
    pub rendered_raw_len: usize,
    previous_rendered_raw_len: usize,
    pub macros: HashMap<String, String>,
    pub quick_consonants_enabled: bool,
    pub english_protection: EnglishProtectionLevel,
    pub macro_in_english: bool,
    pub esc_restore_raw: bool,
    pub committed_raw_history: Vec<String>,
    pub quick_consonant_lock: usize,
    pub nom_phase: NomPhase,
    pub nom_candidates: Vec<char>,
    pub candidate_page: usize,
    pub reading_buffer: String,
    pub ffi_candidates_buf: Vec<HC_CandidateChar>,
    pub phrase_first: Option<String>,
    pub phrase_candidates: Vec<NomTextCandidate>,
    pub phrase_candidate_page: usize,
    pub ffi_phrase_candidates_buf: Vec<HC_HanNomCandidateText>,
    pub ffi_v2_output: String,
    pub phrase_prediction_enabled: bool,
    pub phrase_learning_enabled: bool,
    pub phrase_history_path: std::path::PathBuf,
    pub phrase_history: PhraseHistory,
    pub phrase_history_dirty: bool,
}

impl Session {
    pub fn new(mode: InputMode, legacy_tone: bool) -> Self {
        Self {
            mode,
            legacy_tone,
            buffer: String::new(),
            raw_buffer: String::new(),
            last_commit: String::new(),
            last_raw: String::new(),
            reconversion_active: false,
            spell_check: true,
            auto_restore: true,
            last_commit_time: None,
            previous_buffer: String::new(),
            previous_raw_buffer: String::new(),
            last_spell_check_status: HCSpellCheckStatus::Valid,
            rendered_raw_len: 0,
            previous_rendered_raw_len: 0,
            macros: HashMap::new(),
            quick_consonants_enabled: false,
            english_protection: EnglishProtectionLevel::Off,
            macro_in_english: false,
            esc_restore_raw: false,
            committed_raw_history: Vec::new(),
            quick_consonant_lock: 0,
            nom_phase: NomPhase::Reading,
            nom_candidates: Vec::new(),
            candidate_page: 0,
            reading_buffer: String::new(),
            ffi_candidates_buf: Vec::new(),
            phrase_first: None,
            phrase_candidates: Vec::new(),
            phrase_candidate_page: 0,
            ffi_phrase_candidates_buf: Vec::new(),
            ffi_v2_output: String::new(),
            phrase_prediction_enabled: true,
            phrase_learning_enabled: true,
            phrase_history_path: default_history_path(),
            phrase_history: PhraseHistory::default(),
            phrase_history_dirty: false,
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.raw_buffer.clear();
        self.last_commit.clear();
        self.last_raw.clear();
        self.reconversion_active = false;
        self.last_commit_time = None;
        self.previous_buffer.clear();
        self.previous_raw_buffer.clear();
        self.rendered_raw_len = self.previous_rendered_raw_len;
        self.rendered_raw_len = 0;
        self.previous_rendered_raw_len = 0;
        self.quick_consonant_lock = 0;
        self.nom_phase = NomPhase::Reading;
        self.nom_candidates.clear();
        self.candidate_page = 0;
        self.reading_buffer.clear();
        self.ffi_candidates_buf.clear();
        self.phrase_first = None;
        self.phrase_candidates.clear();
        self.phrase_candidate_page = 0;
        self.ffi_phrase_candidates_buf.clear();
    }

    pub fn render_from_raw(&mut self) {
        self.save_state_for_undo();
        let raw_len = self.raw_buffer.len();
        if raw_len == self.rendered_raw_len + 1 {
            let last_char = self.raw_buffer.chars().last().unwrap();
            if !crate::compose::TypingEngine::apply_trigger(
                &mut self.buffer,
                self.mode,
                last_char,
                self.legacy_tone,
            ) {
                self.buffer.push(last_char);
            }
        } else {
            self.buffer = crate::compose::TypingEngine::render_raw(
                &self.raw_buffer,
                self.mode,
                self.legacy_tone,
            );
        }
        if self.quick_consonants_enabled {
            self.apply_quick_consonants();
        }
        crate::compose::TypingEngine::mirror_raw_casing(&self.raw_buffer, &mut self.buffer);
        self.rendered_raw_len = self.raw_buffer.len();
        self.update_spell_check_status();
    }

    fn apply_quick_consonants(&mut self) {
        quick_consonants::apply_mid_word_quick_consonants(
            &mut self.raw_buffer,
            &mut self.quick_consonant_lock,
        );
        quick_consonants::apply_start_quick_consonants(
            &mut self.raw_buffer,
            &mut self.quick_consonant_lock,
        );
        self.buffer =
            crate::compose::TypingEngine::render_raw(&self.raw_buffer, self.mode, self.legacy_tone);
    }

    pub fn apply_end_quick_consonants_if_enabled(&mut self) {
        if self.quick_consonants_enabled {
            quick_consonants::apply_end_quick_consonants(
                &mut self.raw_buffer,
                &mut self.quick_consonant_lock,
            );
            self.buffer = crate::compose::TypingEngine::render_raw(
                &self.raw_buffer,
                self.mode,
                self.legacy_tone,
            );
        }
    }

    pub fn update_spell_check_status(&mut self) {
        if !self.spell_check || self.buffer.is_empty() {
            self.last_spell_check_status = HCSpellCheckStatus::Valid;
            return;
        }

        let raw = if self.raw_buffer.is_empty() {
            strip_all_marks(&self.buffer)
        } else {
            self.raw_buffer.clone()
        };

        let scores = language_scores(&raw, &self.buffer, self.mode, self.spell_check);

        if self.english_protection == EnglishProtectionLevel::Hard
            && crate::language::is_hard_english_raw_start(&raw)
        {
            self.last_spell_check_status = HCSpellCheckStatus::EnglishFallback;
            return;
        }
        if matches!(
            self.english_protection,
            EnglishProtectionLevel::Hard | EnglishProtectionLevel::Soft
        ) && crate::language::is_soft_english_pattern(&raw)
        {
            self.last_spell_check_status = HCSpellCheckStatus::EnglishFallback;
            return;
        }

        if scores.english > scores.vietnamese {
            self.last_spell_check_status = HCSpellCheckStatus::EnglishFallback;
        } else if !crate::language::is_valid_vietnamese_word(&self.buffer) {
            self.last_spell_check_status = HCSpellCheckStatus::Invalid;
        } else {
            self.last_spell_check_status = HCSpellCheckStatus::Valid;
        }
    }

    pub fn emit_preedit(&self, handled: bool) -> crate::types::HC_KeyResult {
        crate::types::HC_KeyResult {
            state: crate::hc_state_from_string_with_spell_check(
                &self.buffer,
                if self.reconversion_active {
                    HCStatusFlag::ReconversionActive
                } else {
                    HCStatusFlag::InProgress
                },
                crate::types::HCErrorCode::None,
                self.last_spell_check_status,
            ),
            handled: handled as u8,
        }
    }

    pub fn commit_current(&mut self) -> HC_State {
        let raw = if self.raw_buffer.is_empty() {
            strip_all_marks(&self.buffer)
        } else {
            self.raw_buffer.clone()
        };

        let raw_lower = raw.trim().to_lowercase();
        let macro_match = self.macros.get(&raw_lower).cloned();
        if let Some(expansion) = macro_match {
            let should_expand =
                if self.last_spell_check_status == HCSpellCheckStatus::EnglishFallback {
                    self.macro_in_english
                } else {
                    true
                };
            if should_expand {
                self.committed_raw_history.push(raw.trim().to_string());
                self.last_commit = expansion.clone();
                self.last_raw = raw.trim().to_string();
                self.reconversion_active = false;
                self.buffer.clear();
                self.raw_buffer.clear();
                self.last_commit_time = Some(Instant::now());
                self.quick_consonant_lock = 0;
                return crate::hc_state_from_string(
                    &expansion,
                    HCStatusFlag::Commit,
                    crate::types::HCErrorCode::None,
                );
            }
        }

        let rendered = self.buffer.clone();
        let decision = resolve_commit_text(
            &raw,
            &rendered,
            self.mode,
            self.spell_check,
            self.auto_restore,
        );

        self.committed_raw_history.push(raw.trim().to_string());
        self.last_commit = decision.text.clone();
        self.last_raw = raw.trim().to_string();
        self.reconversion_active = false;
        self.buffer.clear();
        self.raw_buffer.clear();
        self.last_commit_time = Some(Instant::now());
        self.quick_consonant_lock = 0;

        crate::hc_state_from_string(
            &decision.text,
            decision.status,
            crate::types::HCErrorCode::None,
        )
    }

    pub fn add_macro(&mut self, key: &str, value: &str) {
        self.macros.insert(key.to_lowercase(), value.to_string());
    }

    pub fn clear_macros(&mut self) {
        self.macros.clear();
    }

    pub fn can_edit_last_commit(&self) -> bool {
        if self.last_commit.is_empty() {
            return false;
        }
        match self.last_commit_time {
            Some(t) => t.elapsed().as_millis() < EDIT_TIMEOUT_MS,
            None => false,
        }
    }

    pub fn save_state_for_undo(&mut self) {
        self.previous_buffer = self.buffer.clone();
        self.previous_raw_buffer = self.raw_buffer.clone();
        self.previous_rendered_raw_len = self.rendered_raw_len;
    }

    pub fn undo(&mut self) -> bool {
        if self.previous_buffer.is_empty() && self.previous_raw_buffer.is_empty() {
            return false;
        }
        self.buffer = self.previous_buffer.clone();
        self.raw_buffer = self.previous_raw_buffer.clone();
        self.previous_buffer.clear();
        self.previous_raw_buffer.clear();
        self.rendered_raw_len = self.previous_rendered_raw_len;
        true
    }

    pub fn try_boundary_trigger(&mut self, text: *const c_char) -> bool {
        let Some(text) = crate::key_text(text) else {
            return false;
        };
        let mut chars = text.chars();
        let Some(ch) = chars.next() else {
            return false;
        };
        if chars.next().is_some() || !is_viqr_trigger(ch) {
            return false;
        }

        self.raw_buffer.push(ch);
        self.render_from_raw();
        true
    }

    pub fn try_esc_restore_raw(&mut self) -> Option<String> {
        if !self.esc_restore_raw {
            return None;
        }
        if !self.buffer.is_empty() && !self.raw_buffer.is_empty() {
            let raw = self.raw_buffer.clone();
            self.reset();
            return Some(raw);
        }
        None
    }
}

pub fn render_raw_input(raw: &str, mode: InputMode, legacy_tone: bool) -> String {
    crate::compose::TypingEngine::render_raw(raw, mode, legacy_tone)
}

pub fn vni_digit_transforms_buffer(buffer: &str, ch: char, legacy_tone: bool) -> bool {
    crate::compose::TypingEngine::vni_digit_transforms_buffer(buffer, ch, legacy_tone)
}

pub fn resolve_commit_text(
    raw: &str,
    rendered: &str,
    mode: InputMode,
    spell_check: bool,
    auto_restore: bool,
) -> CommitDecision {
    let raw = raw.trim();
    let rendered = rendered.trim();
    if raw.is_empty() && rendered.is_empty() {
        return CommitDecision {
            text: String::new(),
            status: HCStatusFlag::Commit,
        };
    }

    if !auto_restore {
        return CommitDecision {
            text: rendered.to_string(),
            status: HCStatusFlag::Commit,
        };
    }

    let scores = language_scores(raw, rendered, mode, spell_check);
    if scores.english > scores.vietnamese {
        CommitDecision {
            text: raw.to_string(),
            status: HCStatusFlag::EnglishFallback,
        }
    } else {
        CommitDecision {
            text: rendered.to_string(),
            status: HCStatusFlag::Commit,
        }
    }
}
