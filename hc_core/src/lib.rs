use std::ffi::{c_char, CStr};
use std::ptr;

mod language;
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
use session::Session;
use transform::{apply_circumflex, apply_telex_w, apply_tone_to_word};
use vowel::strip_all_marks;

impl Session {
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

        if let Some(kind) = key_kind(request.kind) {
            match kind {
                HCKeyKind::Other => {
                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
                HCKeyKind::Escape => {
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
                        let last = self.raw_buffer.pop().unwrap();
                        if session::is_raw_trigger(last, self.mode) && self.raw_buffer.len() == 1 {
                            self.raw_buffer.pop();
                        }
                        self.render_from_raw();
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
                    if text.chars().next().is_none() {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::InvalidUtf8),
                            handled: 0,
                        };
                    };

                    self.last_commit.clear();
                    self.last_raw.clear();
                    self.last_commit_time = None;
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
        drop(Box::from_raw(session as *mut Session));
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
                InputMode::Telex => apply_telex_w(&mut clone),
                InputMode::Vni => apply_circumflex(&mut clone),
                InputMode::Viqr => apply_circumflex(&mut clone),
            };
            clone
        }
        EditTrigger::LiteralNumber => word.to_string(),
        EditTrigger::Escape => word.to_string(),
    }
}
