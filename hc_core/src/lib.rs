use std::cell::RefCell;
use std::ffi::{c_char, CStr};
use std::ptr;

pub mod compose;
pub mod han_nom;
mod language;
mod quick_consonants;
mod session;
mod transform;
mod types;
mod vowel;

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

pub use types::*;

use language::is_known_english_word;
use session::{render_raw_input, vni_digit_transforms_buffer, NomTextCandidate, Session};
use transform::{apply_circumflex, apply_telex_w, apply_tone_to_word};

use crate::han_nom::{
    get_global_dict, get_global_phrase_dict, load_user_phrase_dict, normalize_phrase_reading,
    PhraseHistory,
};
use vowel::strip_all_marks;

thread_local! {
    static UTF8_RESULT_BUFFER: RefCell<String> = const { RefCell::new(String::new()) };
}

impl Session {
    fn set_hannom_options(&mut self, options: &HC_HanNomOptions) {
        self.phrase_prediction_enabled = options.phrase_prediction != 0;
        self.phrase_learning_enabled = options.learning_enabled != 0;
        if !options.history_path.is_null() {
            if let Some(path) = key_text(options.history_path) {
                if !path.is_empty() {
                    self.phrase_history_path = std::path::PathBuf::from(path);
                }
            }
        }
        self.phrase_history = PhraseHistory::load(&self.phrase_history_path);
    }

    fn set_hannom_options_v2(&mut self, options: &HC_HanNomOptionsV2) {
        self.set_hannom_options(&HC_HanNomOptions {
            phrase_prediction: options.phrase_prediction,
            learning_enabled: options.learning_enabled,
            history_path: options.history_path,
        });
        self.user_phrase_path = None;
        if !options.user_phrase_path.is_null() {
            if let Some(path) = key_text(options.user_phrase_path).filter(|path| !path.is_empty()) {
                self.user_phrase_path = Some(std::path::PathBuf::from(path));
            }
        }
        self.reload_user_phrase_entries();
    }

    fn reload_user_phrase_entries(&mut self) {
        self.user_phrase_entries.clear();
        if let Some(path) = self.user_phrase_path.as_deref() {
            self.user_phrase_entries = load_user_phrase_dict(path).0;
        }
    }

    fn phrase_reading(&self) -> String {
        match &self.phrase_first {
            Some(first) if self.buffer.is_empty() => format!("{first} "),
            Some(first) => format!("{first} {}", self.buffer),
            None => self.buffer.clone(),
        }
    }

    fn rebuild_phrase_candidates(&mut self) {
        self.phrase_candidates.clear();
        let Ok(chars) = get_global_dict() else {
            return;
        };
        let full = self.phrase_reading();
        if self.phrase_first.is_none() {
            for (rank, ch) in chars.lookup(&self.buffer).into_iter().enumerate() {
                self.phrase_candidates.push(NomTextCandidate {
                    text: ch.to_string(),
                    reading: self.buffer.clone(),
                    kind: 3,
                    system_rank: rank as u32,
                    source_tier: 1,
                });
            }
            self.sort_phrase_candidates();
            return;
        }
        let Ok(phrases) = get_global_phrase_dict() else {
            return;
        };
        let normalized = normalize_phrase_reading(&full);
        let exact = !self.buffer.is_empty();
        let entries = if exact {
            phrases.exact(&normalized)
        } else if self.phrase_prediction_enabled {
            phrases.prefix(&normalized)
        } else {
            Vec::new()
        };
        for entry in entries {
            self.phrase_candidates.push(NomTextCandidate {
                text: entry.glyphs,
                reading: entry.reading,
                kind: if exact { 0 } else { 1 },
                system_rank: entry.system_rank,
                source_tier: 1,
            });
        }
        for entry in &self.user_phrase_entries {
            if (exact && entry.reading == normalized)
                || (!exact
                    && self.phrase_prediction_enabled
                    && entry.reading.starts_with(&normalized))
            {
                self.phrase_candidates.push(NomTextCandidate {
                    text: entry.glyphs.clone(),
                    reading: entry.reading.clone(),
                    kind: if exact { 0 } else { 1 },
                    system_rank: entry.system_rank,
                    source_tier: 0,
                });
            }
        }
        if exact && self.phrase_candidates.is_empty() {
            let first = self.phrase_first.as_deref().unwrap_or_default();
            let left = chars.lookup(first);
            let right = chars.lookup(&self.buffer);
            for (li, l) in left.into_iter().take(3).enumerate() {
                for (ri, r) in right.iter().take(3).enumerate() {
                    self.phrase_candidates.push(NomTextCandidate {
                        text: format!("{l}{r}"),
                        reading: normalized.clone(),
                        kind: 2,
                        system_rank: (li * 3 + ri) as u32,
                        source_tier: 2,
                    });
                }
            }
        }
        self.sort_phrase_candidates();
    }

    fn sort_phrase_candidates(&mut self) {
        self.phrase_candidates.sort_by(|left, right| {
            let class = left.kind.cmp(&right.kind);
            if class != std::cmp::Ordering::Equal {
                return class;
            }
            let (lc, lt) = if self.phrase_learning_enabled {
                self.phrase_history.score(&left.reading, &left.text)
            } else {
                (0, 0)
            };
            let (rc, rt) = if self.phrase_learning_enabled {
                self.phrase_history.score(&right.reading, &right.text)
            } else {
                (0, 0)
            };
            rc.cmp(&lc)
                .then_with(|| rt.cmp(&lt))
                .then_with(|| left.source_tier.cmp(&right.source_tier))
                .then_with(|| left.system_rank.cmp(&right.system_rank))
                .then_with(|| left.text.cmp(&right.text))
        });
        self.phrase_candidates
            .dedup_by(|left, right| left.text == right.text);
        if self.phrase_candidate_page * 9 >= self.phrase_candidates.len() {
            self.phrase_candidate_page = 0;
        }
    }

    fn populate_nom_result_v2(&mut self, result: &mut HC_HanNomResultV2, handled: u8) {
        self.rebuild_phrase_candidates();
        self.reading_buffer = self.phrase_reading();
        result.status_flag = HCStatusFlag::InProgress as i32;
        result.error_code = HCErrorCode::None as i32;
        result.reading = self.reading_buffer.as_ptr();
        result.reading_len = self.reading_buffer.len().min(u16::MAX as usize) as u16;
        result.handled = handled;
        let start = self.phrase_candidate_page * 9;
        let end = (start + 9).min(self.phrase_candidates.len());
        self.ffi_phrase_candidates_buf.clear();
        for candidate in &self.phrase_candidates[start..end] {
            self.ffi_phrase_candidates_buf.push(HC_HanNomCandidateText {
                text: candidate.text.as_ptr(),
                text_len: candidate.text.len().min(u16::MAX as usize) as u16,
                reading: candidate.reading.as_ptr(),
                reading_len: candidate.reading.len().min(u16::MAX as usize) as u16,
                kind: candidate.kind,
            });
        }
        result.candidates = if self.ffi_phrase_candidates_buf.is_empty() {
            ptr::null()
        } else {
            self.ffi_phrase_candidates_buf.as_ptr()
        };
        result.candidate_count = self.ffi_phrase_candidates_buf.len() as u16;
    }

    fn populate_nom_result_v3(&mut self, result: &mut HC_HanNomResultV3, handled: u8) {
        const MAX_CANDIDATES: usize = 256;
        self.rebuild_phrase_candidates();
        self.reading_buffer = self.phrase_reading();
        self.ffi_phrase_candidates_buf.clear();
        let total = self.phrase_candidates.len();
        for candidate in self.phrase_candidates.iter().take(MAX_CANDIDATES) {
            self.ffi_phrase_candidates_buf.push(HC_HanNomCandidateText {
                text: candidate.text.as_ptr(),
                text_len: candidate.text.len().min(u16::MAX as usize) as u16,
                reading: candidate.reading.as_ptr(),
                reading_len: candidate.reading.len().min(u16::MAX as usize) as u16,
                kind: candidate.kind,
            });
        }
        result.status_flag = HCStatusFlag::InProgress as i32;
        result.error_code = HCErrorCode::None as i32;
        result.reading = self.reading_buffer.as_ptr();
        result.reading_len = self.reading_buffer.len().min(u16::MAX as usize) as u16;
        result.candidates = if self.ffi_phrase_candidates_buf.is_empty() {
            ptr::null()
        } else {
            self.ffi_phrase_candidates_buf.as_ptr()
        };
        result.candidate_count = self.ffi_phrase_candidates_buf.len() as u16;
        result.total_candidate_count = total.min(u16::MAX as usize) as u16;
        result.page_size = 9;
        result.truncated = (total > MAX_CANDIDATES) as u8;
        result.handled = handled;
    }

    fn commit_phrase_text(
        &mut self,
        text: String,
        _reading: String,
        result: &mut HC_HanNomResultV2,
    ) {
        self.ffi_v2_output = text;
        result.status_flag = HCStatusFlag::Commit as i32;
        result.error_code = HCErrorCode::None as i32;
        result.reading = self.ffi_v2_output.as_ptr();
        result.reading_len = self.ffi_v2_output.len().min(u16::MAX as usize) as u16;
        result.candidates = ptr::null();
        result.candidate_count = 0;
        result.handled = 1;
        self.reset();
    }

    fn record_phrase_selection(&mut self, reading: &str, glyphs: &str) {
        if self.phrase_learning_enabled && !reading.is_empty() {
            self.phrase_history.record(reading, glyphs);
            self.phrase_history_dirty = true;
        }
    }

    fn flush_hannom_learning(&mut self) {
        if self.phrase_history_dirty
            && self.phrase_learning_enabled
            && self
                .phrase_history
                .persist(&self.phrase_history_path)
                .is_ok()
        {
            self.phrase_history_dirty = false;
        }
    }

    fn handle_han_nom_key_v2(
        &mut self,
        request: &HC_KeyRequest,
        result: &mut HC_HanNomResultV2,
    ) -> i32 {
        self.ffi_v2_output.clear();
        *result = HC_HanNomResultV2 {
            status_flag: HCStatusFlag::InProgress as i32,
            error_code: HCErrorCode::None as i32,
            reading: ptr::null(),
            reading_len: 0,
            candidates: ptr::null(),
            candidate_count: 0,
            handled: 0,
        };
        self.mode = match InputMode::try_from(request.input_mode) {
            Ok(value) => value,
            Err(_) => {
                result.error_code = HCErrorCode::InvalidInputMode as i32;
                return 0;
            }
        };
        let Some(kind) = key_kind(request.kind) else {
            return 0;
        };
        let text = key_text(request.text).unwrap_or("");
        match kind {
            HCKeyKind::Escape => {
                if self.phrase_first.is_some() && self.buffer.is_empty() {
                    self.phrase_first = None;
                    self.phrase_candidate_page = 0;
                } else {
                    self.reset();
                }
                self.populate_nom_result_v2(result, 1);
                1
            }
            HCKeyKind::Backspace => {
                self.phrase_candidate_page = 0;
                if self.raw_buffer.is_empty() && self.phrase_first.is_some() {
                    self.buffer = self.phrase_first.take().unwrap_or_default();
                    self.raw_buffer = self.buffer.clone();
                    self.rendered_raw_len = self.raw_buffer.len();
                } else if !self.raw_buffer.is_empty() {
                    self.raw_buffer.pop();
                    self.render_from_raw();
                }
                self.populate_nom_result_v2(result, 1);
                1
            }
            HCKeyKind::Space => {
                if self.buffer.is_empty() && self.phrase_first.is_none() {
                    return 0;
                }
                if self.phrase_first.is_none() {
                    self.phrase_first = Some(normalize_phrase_reading(&self.buffer));
                    self.phrase_candidate_page = 0;
                    self.buffer.clear();
                    self.raw_buffer.clear();
                    self.rendered_raw_len = 0;
                    self.populate_nom_result_v2(result, 1);
                    return 1;
                }
                self.rebuild_phrase_candidates();
                let reading = normalize_phrase_reading(&self.phrase_reading());
                let output = self
                    .phrase_candidates
                    .first()
                    .map(|candidate| candidate.text.clone())
                    .unwrap_or_else(|| reading.clone());
                if !self.phrase_candidates.is_empty() {
                    self.record_phrase_selection(&reading, &output);
                }
                self.commit_phrase_text(format!("{output} "), reading, result);
                1
            }
            HCKeyKind::Enter => {
                if self.buffer.is_empty() && self.phrase_first.is_none() {
                    return 0;
                }
                let raw = normalize_phrase_reading(&self.phrase_reading());
                self.commit_phrase_text(raw.clone(), String::new(), result);
                1
            }
            HCKeyKind::Printable => {
                let ch = text.chars().next().unwrap_or('\0');
                if matches!(ch, '=' | ']' | '+') {
                    self.rebuild_phrase_candidates();
                    if (self.phrase_candidate_page + 1) * 9 < self.phrase_candidates.len() {
                        self.phrase_candidate_page += 1;
                    }
                    self.populate_nom_result_v2(result, 1);
                    return 1;
                }
                if matches!(ch, '-' | '[') {
                    self.phrase_candidate_page = self.phrase_candidate_page.saturating_sub(1);
                    self.populate_nom_result_v2(result, 1);
                    return 1;
                }
                self.phrase_candidate_page = 0;
                if is_nom_punctuation(ch) && self.phrase_first.is_some() {
                    self.rebuild_phrase_candidates();
                    let reading = normalize_phrase_reading(&self.phrase_reading());
                    let output = self
                        .phrase_candidates
                        .first()
                        .map(|candidate| candidate.text.clone())
                        .unwrap_or(reading.clone());
                    if !self.phrase_candidates.is_empty() {
                        self.record_phrase_selection(&reading, &output);
                    }
                    self.commit_phrase_text(format!("{output}{ch}"), reading, result);
                    return 1;
                }
                if matches!(self.mode, InputMode::HanNomVni | InputMode::Vni) && ch.is_ascii_digit()
                {
                    if self.raw_buffer.is_empty() {
                        result.handled = 0;
                        return 0;
                    }
                    compose::TypingEngine::apply_vni_trigger(
                        &mut self.buffer,
                        ch,
                        self.legacy_tone,
                    );
                    self.raw_buffer.push(ch);
                    self.populate_nom_result_v2(result, 1);
                    return 1;
                }
                if ch.is_ascii_digit() {
                    result.handled = 0;
                    return 0;
                }
                if self.raw_buffer.len() < 64 {
                    self.raw_buffer.push_str(text);
                    self.render_from_raw();
                }
                self.populate_nom_result_v2(result, 1);
                1
            }
            _ => 0,
        }
    }

    fn select_han_nom_candidate_v2(&mut self, index: usize, result: &mut HC_HanNomResultV2) -> i32 {
        self.rebuild_phrase_candidates();
        let index = self.phrase_candidate_page * 9 + index;
        let Some(candidate) = self.phrase_candidates.get(index).cloned() else {
            return 0;
        };
        self.record_phrase_selection(&candidate.reading, &candidate.text);
        self.commit_phrase_text(candidate.text, candidate.reading, result);
        1
    }

    fn handle_han_nom_key_v3(
        &mut self,
        request: &HC_KeyRequest,
        result: &mut HC_HanNomResultV3,
    ) -> i32 {
        *result = HC_HanNomResultV3 {
            status_flag: HCStatusFlag::InProgress as i32,
            error_code: HCErrorCode::None as i32,
            reading: ptr::null(),
            reading_len: 0,
            candidates: ptr::null(),
            candidate_count: 0,
            total_candidate_count: 0,
            page_size: 9,
            truncated: 0,
            handled: 0,
        };
        if matches!(key_kind(request.kind), Some(HCKeyKind::Printable))
            && matches!(
                key_text(request.text).unwrap_or(""),
                "=" | "+" | "]" | "-" | "["
            )
        {
            // V3 deliberately keeps paging in Fcitx; never mutate the frozen V2 page cursor.
            self.populate_nom_result_v3(result, 1);
            return 1;
        }
        let mut v2 = HC_HanNomResultV2 {
            status_flag: 0,
            error_code: 0,
            reading: ptr::null(),
            reading_len: 0,
            candidates: ptr::null(),
            candidate_count: 0,
            handled: 0,
        };
        let handled = self.handle_han_nom_key_v2(request, &mut v2);
        if handled == 0 {
            return 0;
        }
        if v2.status_flag == HCStatusFlag::Commit as i32 {
            result.status_flag = v2.status_flag;
            result.error_code = v2.error_code;
            result.reading = v2.reading;
            result.reading_len = v2.reading_len;
            result.handled = v2.handled;
        } else {
            self.populate_nom_result_v3(result, v2.handled);
        }
        handled
    }

    fn select_han_nom_candidate_v3(&mut self, index: usize, result: &mut HC_HanNomResultV3) -> i32 {
        self.rebuild_phrase_candidates();
        let Some(candidate) = self.phrase_candidates.get(index).cloned() else {
            return 0;
        };
        self.record_phrase_selection(&candidate.reading, &candidate.text);
        self.ffi_v2_output = candidate.text;
        result.status_flag = HCStatusFlag::Commit as i32;
        result.error_code = HCErrorCode::None as i32;
        result.reading = self.ffi_v2_output.as_ptr();
        result.reading_len = self.ffi_v2_output.len().min(u16::MAX as usize) as u16;
        result.candidates = ptr::null();
        result.candidate_count = 0;
        result.total_candidate_count = 0;
        result.page_size = 9;
        result.truncated = 0;
        result.handled = 1;
        self.reset();
        1
    }
    fn handle_key(&mut self, request: &HC_KeyRequest) -> HC_KeyResult {
        self.mode = match InputMode::try_from(request.input_mode) {
            Ok(mode) => mode,
            Err(_) => {
                return HC_KeyResult {
                    state: hc_error_state(HCErrorCode::InvalidInputMode),
                    handled: 0,
                }
            }
        };
        self.legacy_tone = request.legacy_tone != 0;
        self.spell_check = request.spell_check != 0;
        self.auto_restore = request.auto_restore != 0;
        self.quick_consonants_enabled = request.quick_consonants != 0;
        self.english_protection = EnglishProtectionLevel::from(request.english_protection);
        self.macro_in_english = request.macro_in_english != 0;
        self.esc_restore_raw = request.esc_restore_raw != 0;

        if let Some(kind) = key_kind(request.kind) {
            match kind {
                HCKeyKind::Other => {
                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
                HCKeyKind::Escape => {
                    if let Some(raw) = self.try_esc_restore_raw() {
                        return HC_KeyResult {
                            state: hc_state_from_string(
                                &raw,
                                HCStatusFlag::EscRestoredRaw,
                                HCErrorCode::None,
                            ),
                            handled: 1,
                        };
                    }
                    if self.buffer.is_empty() && self.last_commit.is_empty() {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::None),
                            handled: 0,
                        };
                    }
                    self.reset();
                    return HC_KeyResult {
                        state: hc_state_from_string(
                            "",
                            HCStatusFlag::InProgress,
                            HCErrorCode::None,
                        ),
                        handled: 1,
                    };
                }
                HCKeyKind::Backspace => {
                    if !self.raw_buffer.is_empty() {
                        match self.mode {
                            InputMode::Vni => {
                                self.raw_buffer = vni_raw_after_visible_backspace(
                                    &self.raw_buffer,
                                    &self.buffer,
                                    self.legacy_tone,
                                );
                                self.render_from_raw();
                            }
                            _ => {
                                self.raw_buffer.pop();
                                if self.quick_consonants_enabled {
                                    self.quick_consonant_lock =
                                        self.quick_consonant_lock.min(self.raw_buffer.len());
                                }
                                self.render_from_raw();
                            }
                        }
                        if self.raw_buffer.is_empty() {
                            self.reconversion_active = false;
                        }
                        return self.emit_preedit(true);
                    }

                    if self.can_edit_last_commit() {
                        self.buffer = self.last_commit.clone();
                        self.raw_buffer = if self.last_raw.is_empty() {
                            strip_all_marks(&self.buffer)
                        } else {
                            self.last_raw.clone()
                        };
                        self.reconversion_active = true;
                        self.last_commit.clear();
                        self.last_raw.clear();
                        self.last_commit_time = None;
                        return HC_KeyResult {
                            state: hc_state_from_string(
                                &self.buffer,
                                HCStatusFlag::ReconversionActive,
                                HCErrorCode::None,
                            ),
                            handled: 1,
                        };
                    }

                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
                HCKeyKind::Enter | HCKeyKind::Space | HCKeyKind::Boundary => {
                    if self.buffer.is_empty() {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::None),
                            handled: 0,
                        };
                    }

                    if kind == HCKeyKind::Boundary
                        && self.mode == InputMode::Viqr
                        && self.try_boundary_trigger(request.text)
                    {
                        return self.emit_preedit(true);
                    }

                    self.apply_end_quick_consonants_if_enabled();

                    let commit = self.commit_current();
                    return HC_KeyResult {
                        state: commit,
                        handled: 1,
                    };
                }
                HCKeyKind::Printable => {
                    let Some(text) = key_text(request.text) else {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::InvalidUtf8),
                            handled: 0,
                        };
                    };

                    self.reconversion_active = false;
                    let mut chars = text.chars();
                    let Some(first_char) = chars.next() else {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::InvalidUtf8),
                            handled: 0,
                        };
                    };
                    let single_char = chars.next().is_none();

                    // Auto-reopen last commit for VNI digit transformation within edit window.
                    // For tone digits (1-5), only reopen when the committed word has no tone,
                    // so that standalone numbers after a toned word pass through as literals.
                    // For diacritic digits (6-9) and 0, use the normal transform check.
                    let auto_reopen_allowed = self.can_edit_last_commit() && {
                        if ('1'..='5').contains(&first_char) {
                            strip_all_marks(&self.last_commit) == self.last_commit
                        } else {
                            vni_digit_transforms_buffer(
                                &self.last_commit,
                                first_char,
                                self.legacy_tone,
                            )
                        }
                    };
                    if self.mode == InputMode::Vni
                        && single_char
                        && first_char.is_ascii_digit()
                        && self.buffer.is_empty()
                        && self.raw_buffer.is_empty()
                        && auto_reopen_allowed
                    {
                        self.buffer = self.last_commit.clone();
                        self.raw_buffer = if self.last_raw.is_empty() {
                            strip_all_marks(&self.buffer)
                        } else {
                            self.last_raw.clone()
                        };
                    }

                    self.last_commit.clear();
                    self.last_raw.clear();
                    self.last_commit_time = None;

                    if self.mode == InputMode::Vni && single_char && first_char.is_ascii_digit() {
                        if self.buffer.is_empty() && self.raw_buffer.is_empty() {
                            return HC_KeyResult {
                                state: hc_error_state(HCErrorCode::None),
                                handled: 0,
                            };
                        }

                        if !vni_digit_transforms_buffer(&self.buffer, first_char, self.legacy_tone)
                        {
                            self.raw_buffer.push(first_char);
                            self.buffer.push(first_char);
                            let commit = self.commit_current();
                            return HC_KeyResult {
                                state: commit,
                                handled: 1,
                            };
                        }
                    }

                    self.raw_buffer.push_str(text);
                    self.render_from_raw();
                    return self.emit_preedit(true);
                }
                HCKeyKind::Undo => {
                    if self.undo() {
                        return self.emit_preedit(true);
                    }
                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
            }
        }

        HC_KeyResult {
            state: hc_error_state(HCErrorCode::InvalidEditTrigger),
            handled: 0,
        }
    }

    pub fn handle_han_nom_key(
        &mut self,
        request: &HC_KeyRequest,
        result: &mut HC_HanNomResult,
    ) -> i32 {
        result.status_flag = HCStatusFlag::InProgress as i32;
        result.error_code = HCErrorCode::None as i32;
        result.handled = 0;
        result.reading_len = 0;
        result.candidate_count = 0;
        result.page = 0;
        result.total_candidates = 0;
        result.has_more = 0;
        result.candidates = ptr::null();

        let mode = match InputMode::try_from(request.input_mode) {
            Ok(m) => m,
            Err(_) => {
                result.error_code = HCErrorCode::InvalidInputMode as i32;
                return 0;
            }
        };
        self.mode = mode;

        let dict = match han_nom::get_global_dict() {
            Ok(d) => d,
            Err(err) => {
                result.error_code = err as i32;
                return 0;
            }
        };

        let kind = match key_kind(request.kind) {
            Some(k) => k,
            None => return 0,
        };

        let text = key_text(request.text).unwrap_or("");

        // Phase B (Candidate Selection)
        if self.nom_phase == NomPhase::Candidate {
            match kind {
                HCKeyKind::Space => {
                    if !self.nom_candidates.is_empty() {
                        let idx = self.candidate_page * 9;
                        if idx < self.nom_candidates.len() {
                            let selected_char = self.nom_candidates[idx];
                            self.commit_nom_char(selected_char, result);
                            return 1;
                        }
                    }
                    self.reset();
                    result.handled = 1;
                    return 1;
                }
                HCKeyKind::Enter => {
                    // CJK standard: Enter in candidate phase commits raw pre-edit reading (Quốc Ngữ)
                    let commit_str = self.buffer.clone();
                    self.reset();
                    result.status_flag = HCStatusFlag::Commit as i32;
                    let bytes = commit_str.as_bytes();
                    let len = bytes.len().min(255);
                    result.reading[..len].copy_from_slice(&bytes[..len]);
                    result.reading_len = len as u16;
                    result.handled = 1;
                    return 1;
                }
                HCKeyKind::Escape => {
                    self.nom_phase = NomPhase::Reading;
                    self.populate_nom_result(result, 1);
                    return 1;
                }
                HCKeyKind::Backspace => {
                    self.nom_phase = NomPhase::Reading;
                    if !self.raw_buffer.is_empty() {
                        self.raw_buffer.pop();
                        self.render_from_raw();
                    }
                    self.populate_nom_result(result, 1);
                    return 1;
                }
                HCKeyKind::Printable => {
                    let first_ch = text.chars().next().unwrap_or('\0');
                    if first_ch.is_ascii_digit() && first_ch != '0' {
                        let digit_val = first_ch.to_digit(10).unwrap() as usize;
                        let idx = self.candidate_page * 9 + (digit_val - 1);
                        if idx < self.nom_candidates.len() {
                            let selected_char = self.nom_candidates[idx];
                            self.commit_nom_char(selected_char, result);
                            return 1;
                        }
                        self.populate_nom_result(result, 1);
                        return 1;
                    }
                    if first_ch == '=' || first_ch == ']' || first_ch == '+' {
                        if (self.candidate_page + 1) * 9 < self.nom_candidates.len() {
                            self.candidate_page += 1;
                        }
                        self.populate_nom_result(result, 1);
                        return 1;
                    }
                    if first_ch == '-' || first_ch == '[' {
                        if self.candidate_page > 0 {
                            self.candidate_page -= 1;
                        }
                        self.populate_nom_result(result, 1);
                        return 1;
                    }
                    if is_nom_punctuation(first_ch) {
                        let mut output = String::new();
                        let idx = self.candidate_page * 9;
                        if idx < self.nom_candidates.len() {
                            output.push(self.nom_candidates[idx]);
                        } else {
                            output.push_str(&self.buffer);
                        }
                        output.push(first_ch);

                        result.status_flag = HCStatusFlag::Commit as i32;
                        let bytes = output.as_bytes();
                        let len = bytes.len().min(255);
                        result.reading[..len].copy_from_slice(&bytes[..len]);
                        result.reading_len = len as u16;
                        result.handled = 1;
                        self.reset();
                        return 1;
                    }
                    self.nom_phase = NomPhase::Reading;
                }
                _ => {
                    self.nom_phase = NomPhase::Reading;
                }
            }
        }

        // Phase A (Reading)
        match kind {
            HCKeyKind::Escape => {
                self.reset();
                self.populate_nom_result(result, 1);
                1
            }
            HCKeyKind::Backspace => {
                if !self.raw_buffer.is_empty() {
                    match self.mode {
                        InputMode::Vni | InputMode::HanNomVni => {
                            self.raw_buffer = vni_raw_after_visible_backspace(
                                &self.raw_buffer,
                                &self.buffer,
                                self.legacy_tone,
                            );
                            self.render_from_raw();
                        }
                        _ => {
                            self.raw_buffer.pop();
                            self.render_from_raw();
                        }
                    }
                }
                self.populate_nom_result(result, 1);
                1
            }
            HCKeyKind::Space | HCKeyKind::Enter => {
                if self.buffer.is_empty() {
                    result.handled = 0;
                    0
                } else {
                    let candidates = dict.lookup(&self.buffer);
                    if !candidates.is_empty() {
                        self.nom_candidates = candidates;
                        self.nom_phase = NomPhase::Candidate;
                        self.candidate_page = 0;
                        self.reading_buffer = self.buffer.clone();
                        self.populate_nom_result(result, 1);
                        1
                    } else {
                        let commit_str = self.buffer.clone();
                        self.reset();
                        result.status_flag = HCStatusFlag::Commit as i32;
                        let bytes = commit_str.as_bytes();
                        let len = bytes.len().min(255);
                        result.reading[..len].copy_from_slice(&bytes[..len]);
                        result.reading_len = len as u16;
                        result.handled = 1;
                        1
                    }
                }
            }
            HCKeyKind::Printable => {
                let first_ch = text.chars().next().unwrap_or('\0');
                match self.mode {
                    InputMode::HanNomVni | InputMode::Vni => {
                        if first_ch.is_ascii_digit() {
                            if self.raw_buffer.is_empty() {
                                result.handled = 0;
                                return 0;
                            }
                            compose::TypingEngine::apply_vni_trigger(
                                &mut self.buffer,
                                first_ch,
                                self.legacy_tone,
                            );
                            self.raw_buffer.push(first_ch);
                            self.populate_nom_result(result, 1);
                            return 1;
                        }
                    }
                    _ => {
                        if first_ch.is_ascii_digit() {
                            result.handled = 0;
                            return 0;
                        }
                    }
                }

                if self.raw_buffer.len() < 64 {
                    self.raw_buffer.push_str(text);
                    self.render_from_raw();
                }
                self.populate_nom_result(result, 1);
                1
            }
            _ => {
                result.handled = 0;
                0
            }
        }
    }

    fn commit_nom_char(&mut self, ch: char, result: &mut HC_HanNomResult) {
        let mut utf8_bytes = [0u8; 5];
        let s = ch.to_string();
        let bytes = s.as_bytes();
        let len = bytes.len().min(4);
        utf8_bytes[..len].copy_from_slice(&bytes[..len]);

        result.status_flag = HCStatusFlag::Commit as i32;
        result.reading[..len].copy_from_slice(&bytes[..len]);
        result.reading_len = len as u16;
        result.handled = 1;

        self.reset();
    }

    fn populate_nom_result(&mut self, result: &mut HC_HanNomResult, handled: u8) {
        result.handled = handled;
        let r_bytes = self.buffer.as_bytes();
        let r_len = r_bytes.len().min(255);
        result.reading[..r_len].copy_from_slice(&r_bytes[..r_len]);
        result.reading_len = r_len as u16;

        if self.nom_phase == NomPhase::Reading && !self.buffer.is_empty() {
            if let Ok(dict) = get_global_dict() {
                self.nom_candidates = dict.lookup(&self.buffer);
            }
        }

        if !self.nom_candidates.is_empty() && !self.buffer.is_empty() {
            let start = self.candidate_page * 9;
            let end = (start + 9).min(self.nom_candidates.len());
            let page_candidates = &self.nom_candidates[start..end];

            self.ffi_candidates_buf.clear();
            for &ch in page_candidates {
                let mut candidate_char = HC_CandidateChar {
                    utf8: [0u8; 5],
                    byte_len: 0,
                };
                let s = ch.to_string();
                let b = s.as_bytes();
                let b_len = b.len().min(4);
                candidate_char.utf8[..b_len].copy_from_slice(&b[..b_len]);
                candidate_char.byte_len = b_len as u8;
                self.ffi_candidates_buf.push(candidate_char);
            }

            result.candidates = self.ffi_candidates_buf.as_ptr();
            result.candidate_count = self.ffi_candidates_buf.len() as u16;
            result.page = self.candidate_page as u16;
            result.total_candidates = self.nom_candidates.len() as u16;
            result.has_more = if end < self.nom_candidates.len() {
                1
            } else {
                0
            };
        } else {
            result.candidates = ptr::null();
            result.candidate_count = 0;
            result.page = 0;
            result.total_candidates = 0;
            result.has_more = 0;
        }
    }
}

fn vni_raw_after_visible_backspace(raw: &str, rendered: &str, legacy_tone: bool) -> String {
    let mut target = rendered.to_string();
    if target.pop().is_none() {
        let mut fallback = raw.to_string();
        fallback.pop();
        return fallback;
    }

    let raw_chars: Vec<char> = raw.chars().collect();
    for primary_idx in (0..raw_chars.len()).rev() {
        let extra_digit_indices: Vec<usize> = ((primary_idx + 1)..raw_chars.len())
            .filter(|&idx| raw_chars[idx].is_ascii_digit())
            .collect();

        if let Some(candidate) = matching_vni_backspace_candidate(
            &raw_chars,
            primary_idx,
            &extra_digit_indices,
            &target,
            legacy_tone,
        ) {
            return candidate;
        }
    }

    let mut fallback = raw.to_string();
    fallback.pop();
    fallback
}

fn matching_vni_backspace_candidate(
    raw_chars: &[char],
    primary_idx: usize,
    extra_digit_indices: &[usize],
    target: &str,
    legacy_tone: bool,
) -> Option<String> {
    const MAX_EXTRA_DIGITS_FOR_EXACT_SEARCH: usize = 12;

    if extra_digit_indices.len() > MAX_EXTRA_DIGITS_FOR_EXACT_SEARCH {
        return candidate_if_matches(raw_chars, primary_idx, &[], target, legacy_tone);
    }

    let subset_count = 1usize << extra_digit_indices.len();
    for removed_extra_count in 0..=extra_digit_indices.len() {
        for mask in 0..subset_count {
            if mask.count_ones() as usize != removed_extra_count {
                continue;
            }
            let removed_extras: Vec<usize> = extra_digit_indices
                .iter()
                .enumerate()
                .filter_map(|(bit, idx)| ((mask & (1usize << bit)) != 0).then_some(*idx))
                .collect();
            if let Some(candidate) =
                candidate_if_matches(raw_chars, primary_idx, &removed_extras, target, legacy_tone)
            {
                return Some(candidate);
            }
        }
    }

    None
}

fn candidate_if_matches(
    raw_chars: &[char],
    primary_idx: usize,
    removed_extras: &[usize],
    target: &str,
    legacy_tone: bool,
) -> Option<String> {
    let candidate: String = raw_chars
        .iter()
        .enumerate()
        .filter_map(|(idx, ch)| {
            (idx != primary_idx && !removed_extras.contains(&idx)).then_some(*ch)
        })
        .collect();

    (render_raw_input(&candidate, InputMode::Vni, legacy_tone) == target).then_some(candidate)
}

fn is_nom_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '.' | ','
            | '!'
            | '?'
            | ';'
            | ':'
            | '('
            | ')'
            | '"'
            | '\''
            | '/'
            | '\\'
            | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '~'
    )
}

#[no_mangle]
pub extern "C" fn hc_session_new(input_mode: i32, legacy_tone: u8) -> *mut std::ffi::c_void {
    let mode = match InputMode::try_from(input_mode) {
        Ok(mode) => mode,
        Err(_) => return ptr::null_mut(),
    };
    Box::into_raw(Box::new(Session::new(mode, legacy_tone != 0))) as *mut std::ffi::c_void
}

#[no_mangle]
pub extern "C" fn hc_session_free(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let mut session = Box::from_raw(session as *mut Session);
        session.flush_hannom_learning();
        drop(session);
    }
}

#[no_mangle]
pub extern "C" fn hc_session_reset(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        session.reset();
        session.reload_user_phrase_entries();
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_add_macro(
    session: *mut std::ffi::c_void,
    key: *const c_char,
    value: *const c_char,
) {
    if session.is_null() || key.is_null() || value.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        let key_str = match CStr::from_ptr(key).to_str() {
            Ok(s) => s,
            Err(_) => return,
        };
        let value_str = match CStr::from_ptr(value).to_str() {
            Ok(s) => s,
            Err(_) => return,
        };
        session.add_macro(key_str, value_str);
    }
}

#[no_mangle]
pub extern "C" fn hc_session_clear_macros(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        session.clear_macros();
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
) -> HC_KeyResult {
    if session.is_null() || request.is_null() {
        return HC_KeyResult {
            state: hc_error_state(HCErrorCode::NullPointer),
            handled: 0,
        };
    }

    unsafe {
        let session = &mut *(session as *mut Session);
        session.handle_key(&*request)
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key_utf8(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
) -> HC_Utf8KeyResult {
    let result = hc_session_handle_key(session, request);
    let mut utf8_result = HC_Utf8KeyResult {
        composition_string: ptr::null(),
        length: 0,
        status_flag: result.state.status_flag,
        error_code: result.state.error_code,
        spell_check_status: result.state.spell_check_status,
        handled: result.handled,
    };

    UTF8_RESULT_BUFFER.with(|buffer| {
        let mut buffer = buffer.borrow_mut();
        state_to_utf8_into(&result.state, &mut buffer);
        utf8_result.composition_string = buffer.as_ptr() as *const c_char;
        utf8_result.length = buffer.len();
    });

    let mut state = result.state;
    hc_state_free(&mut state);
    utf8_result
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_compose_with_request(request: *const HC_ComposeRequest) -> HC_State {
    if request.is_null() {
        return hc_error_state(HCErrorCode::NullPointer);
    }

    let request = unsafe { &*request };
    let tone = match parse_tone(request.tone) {
        Ok(tone) => tone,
        Err(err) => return hc_error_state(err),
    };
    let onset = match required_cstr(request.onset) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let medial = match optional_cstr(request.medial) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let nucleus = match required_cstr(request.nucleus) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let coda = match optional_cstr(request.coda) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let trigger_case = match required_cstr(request.trigger_case) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let raw_input = match required_cstr(request.raw_input) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };

    let mut text = format!(
        "{}{}{}{}",
        onset,
        medial.unwrap_or(""),
        nucleus,
        coda.unwrap_or("")
    );
    if tone != Tone::Flat {
        let _ = apply_tone_to_word(&mut text, tone, request.legacy_tone != 0);
    }

    let rendered = mirror_capitalization(trigger_case, &text);
    let lower = vowel::strip_marks_ascii_lower(raw_input);
    if is_known_english_word(&lower) {
        hc_state_from_string(raw_input, HCStatusFlag::EnglishFallback, HCErrorCode::None)
    } else {
        hc_state_from_string(&rendered, HCStatusFlag::Commit, HCErrorCode::None)
    }
}

#[no_mangle]
pub extern "C" fn hc_compose_from_parts(
    onset: *const c_char,
    medial: *const c_char,
    nucleus: *const c_char,
    coda: *const c_char,
    tone: i32,
    trigger_case: *const c_char,
    raw_input: *const c_char,
    legacy_tone: u8,
    boundary: i32,
) -> HC_State {
    let request = HC_ComposeRequest {
        onset,
        medial,
        nucleus,
        coda,
        tone,
        trigger_case,
        raw_input,
        legacy_tone,
        boundary,
    };
    hc_compose_with_request(&request)
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_rehydrate_with_request(request: *const HC_RehydrateRequest) -> HC_State {
    if request.is_null() {
        return hc_error_state(HCErrorCode::NullPointer);
    }

    let request = unsafe { &*request };
    let word = match required_cstr(request.committed_word) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };

    let mode = match InputMode::try_from(request.input_mode) {
        Ok(mode) => mode,
        Err(_) => return hc_error_state(HCErrorCode::InvalidInputMode),
    };
    let trigger = match parse_edit_trigger(request.trigger_kind, request.trigger_value) {
        Ok(trigger) => trigger,
        Err(err) => return hc_error_state(err),
    };

    let edited = apply_edit_trigger_to_word(word, mode, trigger);
    hc_state_from_string(&edited, HCStatusFlag::ReconversionActive, HCErrorCode::None)
}

#[no_mangle]
pub extern "C" fn hc_rehydrate_apply(
    committed_word: *const c_char,
    input_mode: i32,
    trigger_kind: i32,
    trigger_value: i32,
) -> HC_State {
    let request = HC_RehydrateRequest {
        committed_word,
        input_mode,
        trigger_kind,
        trigger_value,
    };
    hc_rehydrate_with_request(&request)
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_state_free(state: *mut HC_State) {
    if state.is_null() {
        return;
    }
    unsafe {
        let state = &mut *state;
        if !state.composition_string.is_null() && state.length > 0 {
            let slice_ptr = std::ptr::slice_from_raw_parts_mut(
                state.composition_string as *mut u16,
                state.length,
            );
            drop(Box::from_raw(slice_ptr));
        }
        state.composition_string = ptr::null();
        state.length = 0;
        state.status_flag = HCStatusFlag::InProgress as i32;
        state.error_code = HCErrorCode::None as i32;
    }
}

fn key_text(ptr: *const c_char) -> Option<&'static str> {
    if ptr.is_null() {
        return None;
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    cstr.to_str().ok()
}

fn required_cstr(ptr: *const c_char) -> Result<&'static str, HCErrorCode> {
    if ptr.is_null() {
        return Err(HCErrorCode::MissingRequiredField);
    }
    optional_cstr(ptr)?.ok_or(HCErrorCode::MissingRequiredField)
}

fn optional_cstr(ptr: *const c_char) -> Result<Option<&'static str>, HCErrorCode> {
    if ptr.is_null() {
        return Ok(None);
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(Some)
        .map_err(|_| HCErrorCode::InvalidUtf8)
}

pub fn hc_state_from_string(text: &str, status: HCStatusFlag, error: HCErrorCode) -> HC_State {
    let utf16: Box<[u16]> = text.encode_utf16().collect::<Vec<_>>().into_boxed_slice();
    let length = utf16.len();
    let ptr = Box::into_raw(utf16) as *mut u16;
    HC_State {
        composition_string: ptr,
        length,
        status_flag: status as i32,
        error_code: error as i32,
        spell_check_status: HCSpellCheckStatus::Valid as i32,
    }
}

fn state_to_utf8_into(state: &HC_State, out: &mut String) {
    out.clear();
    if state.composition_string.is_null() || state.length == 0 {
        return;
    }
    out.reserve(state.length * 3);
    let data = unsafe { std::slice::from_raw_parts(state.composition_string, state.length) };
    let mut i = 0;
    while i < data.len() {
        let mut cp = data[i] as u32;
        if (0xD800..=0xDBFF).contains(&cp) {
            if i + 1 < data.len() {
                let low = data[i + 1] as u32;
                if (0xDC00..=0xDFFF).contains(&low) {
                    cp = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                    i += 1;
                } else {
                    cp = 0xFFFD;
                }
            } else {
                cp = 0xFFFD;
            }
        } else if (0xDC00..=0xDFFF).contains(&cp) {
            cp = 0xFFFD;
        }
        out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
        i += 1;
    }
}

pub fn hc_state_from_string_with_spell_check(
    text: &str,
    status: HCStatusFlag,
    error: HCErrorCode,
    spell_check: HCSpellCheckStatus,
) -> HC_State {
    let utf16: Box<[u16]> = text.encode_utf16().collect::<Vec<_>>().into_boxed_slice();
    let length = utf16.len();
    let ptr = Box::into_raw(utf16) as *mut u16;
    HC_State {
        composition_string: ptr,
        length,
        status_flag: status as i32,
        error_code: error as i32,
        spell_check_status: spell_check as i32,
    }
}

fn hc_error_state(error: HCErrorCode) -> HC_State {
    HC_State {
        composition_string: ptr::null(),
        length: 0,
        status_flag: HCStatusFlag::InProgress as i32,
        error_code: error as i32,
        spell_check_status: HCSpellCheckStatus::Valid as i32,
    }
}

fn mirror_capitalization(trigger_case: &str, output: &str) -> String {
    let mut chars = trigger_case.chars();
    let first = chars.next();
    let second = chars.next();
    if first.is_some_and(char::is_uppercase) && second.is_some_and(char::is_uppercase) {
        output.to_uppercase()
    } else if first.is_some_and(char::is_uppercase) {
        let mut rendered = output.chars();
        match rendered.next() {
            Some(head) => {
                let mut result = head.to_uppercase().collect::<String>();
                result.push_str(rendered.as_str());
                result
            }
            None => String::new(),
        }
    } else {
        output.to_string()
    }
}

fn apply_edit_trigger_to_word(word: &str, mode: InputMode, trigger: EditTrigger) -> String {
    match trigger {
        EditTrigger::Cancel => strip_all_marks(word),
        EditTrigger::TelexW => {
            let mut clone = word.to_string();
            if apply_telex_w(&mut clone) {
                clone
            } else {
                word.to_string()
            }
        }
        EditTrigger::Tone => {
            let mut clone = word.to_string();
            if transform::apply_tone(&mut clone, Tone::Sac, false) {
                clone
            } else {
                word.to_string()
            }
        }
        EditTrigger::VniDiacritic => {
            let mut clone = word.to_string();
            let _ = match mode {
                InputMode::Telex | InputMode::HanNomTelex => apply_telex_w(&mut clone),
                InputMode::Vni | InputMode::HanNomVni => apply_circumflex(&mut clone),
                InputMode::Viqr | InputMode::HanNomViqr => apply_circumflex(&mut clone),
            };
            clone
        }
        EditTrigger::LiteralNumber => word.to_string(),
        EditTrigger::Escape => word.to_string(),
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key_hannom(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
    result: *mut HC_HanNomResult,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        session.handle_han_nom_key(&*request, &mut *result)
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key_hannom_v2(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
    result: *mut HC_HanNomResultV2,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        return 0;
    }
    unsafe { (&mut *(session as *mut Session)).handle_han_nom_key_v2(&*request, &mut *result) }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_select_hannom_candidate_v2(
    session: *mut std::ffi::c_void,
    index: u16,
    result: *mut HC_HanNomResultV2,
) -> i32 {
    if session.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        (&mut *(session as *mut Session)).select_han_nom_candidate_v2(index as usize, &mut *result)
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_set_hannom_options(
    session: *mut std::ffi::c_void,
    options: *const HC_HanNomOptions,
) {
    if session.is_null() || options.is_null() {
        return;
    }
    unsafe {
        (&mut *(session as *mut Session)).set_hannom_options(&*options);
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key_hannom_v3(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
    result: *mut HC_HanNomResultV3,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        return 0;
    }
    unsafe { (&mut *(session as *mut Session)).handle_han_nom_key_v3(&*request, &mut *result) }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_select_hannom_candidate_v3(
    session: *mut std::ffi::c_void,
    index: u16,
    result: *mut HC_HanNomResultV3,
) -> i32 {
    if session.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        (&mut *(session as *mut Session)).select_han_nom_candidate_v3(index as usize, &mut *result)
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_set_hannom_options_v2(
    session: *mut std::ffi::c_void,
    options: *const HC_HanNomOptionsV2,
) {
    if session.is_null() || options.is_null() {
        return;
    }
    unsafe {
        (&mut *(session as *mut Session)).set_hannom_options_v2(&*options);
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_reset_hannom_learning(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        session.phrase_history.reset(&session.phrase_history_path);
    }
}

#[no_mangle]
pub extern "C" fn hc_session_flush_hannom_learning(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        (&mut *(session as *mut Session)).flush_hannom_learning();
    }
}

#[no_mangle]
pub extern "C" fn hc_nom_dict_status(_session: *mut std::ffi::c_void) -> i32 {
    match han_nom::get_global_dict() {
        Ok(_) => 0,
        Err(err) => err as i32,
    }
}
