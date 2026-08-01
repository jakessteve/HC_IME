use std::cell::RefCell;
use std::ffi::{c_char, CStr};
use std::ptr;
#[cfg(test)]
use std::time::{Duration, Instant};

pub mod compose;
pub mod composition;
pub mod han_nom;
mod language;
pub mod platform;
mod quick_consonants;
mod session;
mod transform;
pub mod translation;
mod types;
mod vowel;

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

pub use types::*;

use session::Session;

use composition::{get_global_macros, vni_digit_transforms_buffer};
use language::is_known_english_word;
use transform::{apply_circumflex, apply_telex_w, apply_tone_to_word};

use vowel::strip_all_marks;

#[cfg(test)]
pub(crate) fn hc_session_test_set_last_commit_age(session: *mut std::ffi::c_void, age_ms: u64) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        session.composition.last_commit_time = Some(
            Instant::now()
                .checked_sub(Duration::from_millis(age_ms))
                .unwrap_or_else(Instant::now),
        );
    }
}

thread_local! {
    static UTF8_RESULT_BUFFER: RefCell<String> = const { RefCell::new(String::new()) };
}

impl Session {
    fn handle_key(&mut self, request: &HC_KeyRequest) -> HC_KeyResult {
        self.composition.mode = match InputMode::try_from(request.input_mode) {
            Ok(mode) => mode,
            Err(_) => {
                return HC_KeyResult {
                    state: hc_error_state(HCErrorCode::InvalidInputMode),
                    handled: 0,
                }
            }
        };
        self.composition.legacy_tone = request.legacy_tone != 0;
        self.composition.spell_check = request.spell_check != 0;
        self.composition.auto_restore = request.auto_restore != 0;
        self.composition.quick_consonants_enabled = request.quick_consonants != 0;
        self.composition.english_protection =
            EnglishProtectionLevel::from(request.english_protection);
        self.composition.macro_in_english = request.macro_in_english != 0;
        self.composition.esc_restore_raw = request.esc_restore_raw != 0;

        if let Some(kind) = key_kind(request.kind) {
            match kind {
                HCKeyKind::Other => {
                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
                HCKeyKind::Escape => {
                    if let Some(raw) = self.composition.try_esc_restore_raw() {
                        return HC_KeyResult {
                            state: hc_state_from_string(
                                &raw,
                                HCStatusFlag::EscRestoredRaw,
                                HCErrorCode::None,
                            ),
                            handled: 1,
                        };
                    }
                    if self.composition.buffer.is_empty() && self.composition.last_commit.is_empty()
                    {
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
                    if !self.composition.raw_buffer.is_empty() {
                        match self.composition.mode {
                            InputMode::Vni => {
                                self.composition.raw_buffer = vni_raw_after_visible_backspace(
                                    &self.composition.raw_buffer,
                                    &self.composition.buffer,
                                    self.composition.legacy_tone,
                                );
                                self.composition.render_from_raw();
                            }
                            _ => {
                                self.composition.raw_buffer.pop();
                                if self.composition.quick_consonants_enabled {
                                    self.composition.quick_consonant_lock = self
                                        .composition
                                        .quick_consonant_lock
                                        .min(self.composition.raw_buffer.len());
                                }
                                self.composition.render_from_raw();
                            }
                        }
                        if self.composition.raw_buffer.is_empty() {
                            self.composition.reconversion_active = false;
                        }
                        return self.composition.emit_preedit(true);
                    }

                    if self.composition.can_edit_last_commit() {
                        self.composition.buffer = self.composition.last_commit.clone();
                        self.composition.raw_buffer = if self.composition.last_raw.is_empty() {
                            strip_all_marks(&self.composition.buffer)
                        } else {
                            self.composition.last_raw.clone()
                        };
                        self.composition.reconversion_active = true;
                        self.composition.last_commit.clear();
                        self.composition.last_raw.clear();
                        self.composition.last_commit_time = None;
                        return HC_KeyResult {
                            state: hc_state_from_string(
                                &self.composition.buffer,
                                HCStatusFlag::ReconversionActive,
                                HCErrorCode::None,
                            ),
                            handled: 1,
                        };
                    }

                    // Expired reconversion state must not linger after a rejected
                    // backspace; the client receives its normal key handling.
                    self.composition.clear_last_commit();

                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
                HCKeyKind::Enter | HCKeyKind::Space | HCKeyKind::Boundary => {
                    if self.composition.buffer.is_empty() {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::None),
                            handled: 0,
                        };
                    }

                    if kind == HCKeyKind::Boundary
                        && self.composition.mode == InputMode::Viqr
                        && self.composition.try_boundary_trigger(request.text)
                    {
                        return self.composition.emit_preedit(true);
                    }

                    self.composition.apply_end_quick_consonants_if_enabled();

                    let commit = self.composition.commit_current();
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

                    self.composition.reconversion_active = false;
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
                    let auto_reopen_allowed = self.composition.can_edit_last_commit() && {
                        if ('1'..='5').contains(&first_char) {
                            !self.composition.last_commit.chars().any(|ch| {
                                crate::vowel::vowel_signature(ch)
                                    .is_some_and(|(_, _, t)| t != crate::types::Tone::Flat)
                            })
                        } else {
                            vni_digit_transforms_buffer(
                                &self.composition.last_commit,
                                first_char,
                                self.composition.legacy_tone,
                            )
                        }
                    };
                    let mut auto_reopened = false;
                    if self.composition.mode == InputMode::Vni
                        && single_char
                        && first_char.is_ascii_digit()
                        && self.composition.buffer.is_empty()
                        && self.composition.raw_buffer.is_empty()
                        && auto_reopen_allowed
                    {
                        self.composition.buffer = self.composition.last_commit.clone();
                        self.composition.raw_buffer = if self.composition.last_raw.is_empty() {
                            strip_all_marks(&self.composition.buffer)
                        } else {
                            self.composition.last_raw.clone()
                        };
                        self.composition.reconversion_active = true;
                        auto_reopened = true;
                    }

                    self.composition.last_commit.clear();
                    self.composition.last_raw.clear();
                    self.composition.last_commit_time = None;

                    if self.composition.mode == InputMode::Vni
                        && single_char
                        && first_char.is_ascii_digit()
                    {
                        if self.composition.buffer.is_empty()
                            && self.composition.raw_buffer.is_empty()
                        {
                            return HC_KeyResult {
                                state: hc_error_state(HCErrorCode::None),
                                handled: 0,
                            };
                        }

                        let mut probe = self.composition.buffer.clone();
                        let transforms = crate::compose::TypingEngine::apply_vni_trigger(
                            &mut probe,
                            first_char,
                            self.composition.legacy_tone,
                        );
                        if !transforms {
                            if probe != self.composition.buffer {
                                self.composition.buffer = probe;
                                self.composition.raw_buffer.push(first_char);
                                self.composition.buffer.push(first_char);
                                self.composition.update_spell_check_status();
                                return self.composition.emit_preedit(true);
                            }
                            self.composition.raw_buffer.push(first_char);
                            self.composition.buffer.push(first_char);
                            let commit = self.composition.commit_current();
                            return HC_KeyResult {
                                state: commit,
                                handled: 1,
                            };
                        }
                    }

                    self.composition.raw_buffer.push_str(text);
                    self.composition.render_from_raw();
                    if auto_reopened {
                        self.composition.reconversion_active = true;
                    }
                    return self.composition.emit_preedit(true);
                }
                HCKeyKind::Undo => {
                    if self.composition.undo() {
                        return self.composition.emit_preedit(true);
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

    (composition::render_raw_input(&candidate, InputMode::Vni, legacy_tone) == target)
        .then_some(candidate)
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
        if let Some(t) = session.translator_mut() {
            t.flush_hannom_learning();
        }
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
        session.composition.reset();
        if let Some(t) = session.translator_mut() {
            t.reset();
            t.reload_user_phrase_entries();
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_add_macro(
    _session: *mut std::ffi::c_void,
    key: *const c_char,
    value: *const c_char,
) {
    if key.is_null() || value.is_null() {
        return;
    }
    unsafe {
        let key_str = match CStr::from_ptr(key).to_str() {
            Ok(s) => s,
            Err(_) => return,
        };
        let value_str = match CStr::from_ptr(value).to_str() {
            Ok(s) => s,
            Err(_) => return,
        };
        get_global_macros()
            .write()
            .unwrap()
            .insert(key_str.to_lowercase(), value_str.to_string());
    }
}

#[no_mangle]
pub extern "C" fn hc_session_clear_macros(_session: *mut std::ffi::c_void) {
    get_global_macros().write().unwrap().clear();
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
pub extern "C" fn hc_session_handle_key_v4(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequestV2,
    result: *mut HC_KeyResultV2,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        if !result.is_null() {
            unsafe {
                *result = HC_KeyResultV2 {
                    composition_string: ptr::null(),
                    composition_len: 0,
                    status_flag: HCStatusFlag::InProgress as i32,
                    error_code: HCErrorCode::NullPointer as i32,
                    spell_check_status: HCSpellCheckStatus::Valid as i32,
                    handled: 0,
                    candidates: ptr::null(),
                    candidate_count: 0,
                    total_candidate_count: 0,
                };
            }
        }
        return 0;
    }

    unsafe {
        let session = &mut *(session as *mut Session);
        let req = &*request;

        let composition_method = req.composition_method;
        let is_han_nom = req.translation_target == TRANSLATION_TARGET_HAN_NOM;

        let input_mode = if is_han_nom {
            composition_method + 3
        } else {
            composition_method
        };

        let key_request = HC_KeyRequest {
            kind: req.kind,
            text: req.text,
            input_mode,
            legacy_tone: req.legacy_tone,
            spell_check: req.spell_check,
            auto_restore: req.auto_restore,
            quick_consonants: req.quick_consonants,
            english_protection: req.english_protection,
            macro_in_english: req.macro_in_english,
            esc_restore_raw: req.esc_restore_raw,
        };

        if is_han_nom {
            let translator = session.translator.as_mut().and_then(|t| {
                t.as_any_mut()
                    .downcast_mut::<crate::translation::HanNomTranslator>()
            });
            let composition = &mut session.composition;

            match translator {
                Some(t) => {
                    let mut nom_result = HC_HanNomResultV3 {
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
                    let handled =
                        t.handle_han_nom_key_v3(composition, &key_request, &mut nom_result);

                    *result = HC_KeyResultV2 {
                        composition_string: nom_result.reading,
                        composition_len: nom_result.reading_len as usize,
                        status_flag: nom_result.status_flag,
                        error_code: nom_result.error_code,
                        spell_check_status: HCSpellCheckStatus::Valid as i32,
                        handled: nom_result.handled,
                        candidates: nom_result.candidates,
                        candidate_count: nom_result.candidate_count,
                        total_candidate_count: nom_result.total_candidate_count,
                    };
                    handled
                }
                None => {
                    *result = HC_KeyResultV2 {
                        composition_string: ptr::null(),
                        composition_len: 0,
                        status_flag: HCStatusFlag::InProgress as i32,
                        error_code: HCErrorCode::InvalidInputMode as i32,
                        spell_check_status: HCSpellCheckStatus::Valid as i32,
                        handled: 0,
                        candidates: ptr::null(),
                        candidate_count: 0,
                        total_candidate_count: 0,
                    };
                    0
                }
            }
        } else {
            let key_result = session.handle_key(&key_request);

            UTF8_RESULT_BUFFER.with(|buffer| {
                let mut buffer = buffer.borrow_mut();
                state_to_utf8_into(&key_result.state, &mut buffer);

                let comp_ptr = if buffer.is_empty() {
                    ptr::null()
                } else {
                    buffer.as_ptr()
                };

                *result = HC_KeyResultV2 {
                    composition_string: comp_ptr,
                    composition_len: buffer.len(),
                    status_flag: key_result.state.status_flag,
                    error_code: key_result.state.error_code,
                    spell_check_status: key_result.state.spell_check_status,
                    handled: key_result.handled,
                    candidates: ptr::null(),
                    candidate_count: 0,
                    total_candidate_count: 0,
                };
            });

            let mut state = key_result.state;
            hc_state_free(&mut state);

            if (*result).handled != 0 {
                1
            } else {
                0
            }
        }
    }
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

// ── Hán Nôm FFI (delegates to HanNomTranslator) ──

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[deprecated(note = "Use hc_session_handle_key_v4")]
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
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.handle_han_nom_key(composition, &*request, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[deprecated(note = "Use hc_session_handle_key_v4")]
pub extern "C" fn hc_session_handle_key_hannom_v2(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
    result: *mut HC_HanNomResultV2,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.handle_han_nom_key_v2(composition, &*request, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
    }
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
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.select_han_nom_candidate_v2(composition, index as usize, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
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
        let session = &mut *(session as *mut Session);
        if let Some(t) = session.translator_mut() {
            t.set_hannom_options(&*options);
        }
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
    unsafe {
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.handle_han_nom_key_v3(composition, &*request, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
    }
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
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.select_han_nom_candidate_v3(composition, index as usize, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
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
        let session = &mut *(session as *mut Session);
        if let Some(t) = session.translator_mut() {
            t.set_hannom_options_v2(&*options);
        }
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
        if let Some(t) = session.translator_mut() {
            t.reset_learning_data();
        }
    }
}

#[no_mangle]
pub extern "C" fn hc_session_flush_hannom_learning(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        if let Some(t) = session.translator_mut() {
            t.flush_hannom_learning();
        }
    }
}

#[no_mangle]
pub extern "C" fn hc_nom_dict_status(_session: *mut std::ffi::c_void) -> i32 {
    match han_nom::get_global_dict() {
        Ok(_) => 0,
        Err(err) => err as i32,
    }
}
