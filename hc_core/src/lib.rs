use std::ffi::{c_char, CStr};
use std::ptr;

mod language;
mod session;
mod transform;
mod types;
mod vowel;

pub use types::*;

use language::is_known_english_word;
use session::Session;
use transform::{apply_circumflex, apply_telex_w, apply_tone_to_word};
use vowel::strip_all_marks;

impl Session {
    fn handle_key(&mut self, request: &HC_KeyRequest) -> HC_KeyResult {
        self.mode = match request.input_mode {
            0 => InputMode::Telex,
            1 => InputMode::Vni,
            2 => InputMode::Viqr,
            _ => {
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
                        self.raw_buffer.pop();
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
    let mode = match input_mode {
        0 => InputMode::Telex,
        1 => InputMode::Vni,
        2 => InputMode::Viqr,
        _ => return ptr::null_mut(),
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

    let mode = match request.input_mode {
        0 => InputMode::Telex,
        1 => InputMode::Vni,
        2 => InputMode::Viqr,
        _ => return hc_error_state(HCErrorCode::InvalidInputMode),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    fn read_state(state: HC_State) -> String {
        if state.composition_string.is_null() || state.length == 0 {
            return String::new();
        }
        let slice = unsafe { std::slice::from_raw_parts(state.composition_string, state.length) };
        String::from_utf16(slice).unwrap()
    }

    fn free_state(mut state: HC_State) {
        hc_state_free(&mut state);
    }

    fn read_and_free(mut state: HC_State) -> String {
        let text = read_state(state);
        hc_state_free(&mut state);
        text
    }

    fn key_request(mode: InputMode) -> HC_KeyRequest {
        HC_KeyRequest {
            kind: HCKeyKind::Printable as i32,
            text: ptr::null(),
            input_mode: mode as i32,
            legacy_tone: 0,
            spell_check: 1,
            auto_restore: 1,
        }
    }

    fn type_raw(session: *mut std::ffi::c_void, req: &mut HC_KeyRequest, raw: &str) -> String {
        let mut latest = String::new();
        req.kind = HCKeyKind::Printable as i32;
        for ch in raw.chars() {
            let key = c(&ch.to_string());
            req.text = key.as_ptr();
            latest = read_and_free(hc_session_handle_key(session, req).state);
        }
        latest
    }

    fn send_key(
        session: *mut std::ffi::c_void,
        req: &mut HC_KeyRequest,
        kind: HCKeyKind,
        text: &str,
    ) -> (String, i32) {
        req.kind = kind as i32;
        let key = c(text);
        req.text = key.as_ptr();
        let result = hc_session_handle_key(session, req);
        let status = result.state.status_flag;
        (read_and_free(result.state), status)
    }

    fn commit_with_space(session: *mut std::ffi::c_void, req: &mut HC_KeyRequest) -> (String, i32) {
        req.kind = HCKeyKind::Space as i32;
        let space = c(" ");
        req.text = space.as_ptr();
        let result = hc_session_handle_key(session, req);
        let status = result.state.status_flag;
        (read_and_free(result.state), status)
    }

    #[test]
    fn telex_simple_tone_and_cancel() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let h = c("h");
        let mut req = HC_KeyRequest {
            kind: HCKeyKind::Printable as i32,
            text: h.as_ptr(),
            input_mode: InputMode::Telex as i32,
            legacy_tone: 0,
            spell_check: 1,
            auto_restore: 1,
        };
        let res = hc_session_handle_key(session, &req);
        assert_eq!(read_and_free(res.state), "h");
        let o = c("o");
        req.text = o.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);
        let a = c("a");
        req.text = a.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);
        let f = c("f");
        req.text = f.as_ptr();
        let res = hc_session_handle_key(session, &req);
        assert_eq!(read_and_free(res.state), "hoà");
        let z = c("z");
        req.text = z.as_ptr();
        let res = hc_session_handle_key(session, &req);
        assert_eq!(read_and_free(res.state), "hoa");
        hc_session_free(session);
    }

    #[test]
    fn vni_triggers_and_literal_numbers() {
        let session = hc_session_new(InputMode::Vni as i32, 0);
        let t = c("t");
        let mut req = HC_KeyRequest {
            kind: HCKeyKind::Printable as i32,
            text: t.as_ptr(),
            input_mode: InputMode::Vni as i32,
            legacy_tone: 0,
            spell_check: 1,
            auto_restore: 1,
        };
        free_state(hc_session_handle_key(session, &req).state);
        let r = c("r");
        req.text = r.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);
        let u = c("u");
        req.text = u.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);
        let o = c("o");
        req.text = o.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);
        let n = c("n");
        req.text = n.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);
        let g = c("g");
        req.text = g.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);
        let seven = c("7");
        req.text = seven.as_ptr();
        let res = hc_session_handle_key(session, &req);
        assert!(read_and_free(res.state).contains('ư'));
        hc_session_free(session);
    }

    #[test]
    fn live_session_uses_requested_input_mode() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);
        assert_eq!(type_raw(session, &mut req, "hoa"), "hoa");

        hc_session_reset(session);
        req.input_mode = InputMode::Vni as i32;
        assert_eq!(type_raw(session, &mut req, "hoa2"), "hoà");
        hc_session_free(session);
    }

    #[test]
    fn compose_and_rehydrate_helpers_work() {
        let onset = c("h");
        let nucleus = c("oa");
        let coda = c("n");
        let trigger_case = c("hoa");
        let raw_input = c("hoan");
        let request = HC_ComposeRequest {
            onset: onset.as_ptr(),
            medial: ptr::null(),
            nucleus: nucleus.as_ptr(),
            coda: coda.as_ptr(),
            tone: 2,
            trigger_case: trigger_case.as_ptr(),
            raw_input: raw_input.as_ptr(),
            legacy_tone: 0,
            boundary: 0,
        };
        let mut state = hc_compose_with_request(&request);
        assert_eq!(read_state(state), "hoàn");
        hc_state_free(&mut state);

        let mut from_parts = hc_compose_from_parts(
            onset.as_ptr(),
            ptr::null(),
            nucleus.as_ptr(),
            coda.as_ptr(),
            2,
            trigger_case.as_ptr(),
            raw_input.as_ptr(),
            0,
            0,
        );
        assert_eq!(read_state(from_parts), "hoàn");
        hc_state_free(&mut from_parts);

        let word = c("hoàn");
        let mut rehydrated = hc_rehydrate_apply(word.as_ptr(), 0, EditTrigger::Cancel as i32, 0);
        assert_eq!(read_state(rehydrated), "hoan");
        hc_state_free(&mut rehydrated);
    }

    #[test]
    fn session_backspace_rehydrates_after_commit() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let h = c("h");
        let mut req = HC_KeyRequest {
            kind: HCKeyKind::Printable as i32,
            text: h.as_ptr(),
            input_mode: InputMode::Telex as i32,
            legacy_tone: 0,
            spell_check: 1,
            auto_restore: 1,
        };
        for ch in ["h", "o", "a", "f"] {
            let key = c(ch);
            req.text = key.as_ptr();
            free_state(hc_session_handle_key(session, &req).state);
        }
        req.kind = HCKeyKind::Space as i32;
        let space = c(" ");
        req.text = space.as_ptr();
        let commit = hc_session_handle_key(session, &req);
        assert_eq!(commit.state.status_flag, HCStatusFlag::Commit as i32);
        free_state(commit.state);
        req.kind = HCKeyKind::Backspace as i32;
        req.text = ptr::null();
        let back = hc_session_handle_key(session, &req);
        assert_eq!(
            back.state.status_flag,
            HCStatusFlag::ReconversionActive as i32
        );
        assert_eq!(read_and_free(back.state), "hoà");
        hc_session_free(session);
    }

    #[test]
    fn backspace_does_not_rehydrate_after_typing_new_word() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        for ch in ["h", "o", "a", "f"] {
            let key = c(ch);
            req.text = key.as_ptr();
            free_state(hc_session_handle_key(session, &req).state);
        }
        req.kind = HCKeyKind::Space as i32;
        let space = c(" ");
        req.text = space.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);

        req.kind = HCKeyKind::Printable as i32;
        let x = c("x");
        req.text = x.as_ptr();
        free_state(hc_session_handle_key(session, &req).state);

        req.kind = HCKeyKind::Backspace as i32;
        req.text = ptr::null();
        let back = hc_session_handle_key(session, &req);
        assert_eq!(back.state.status_flag, HCStatusFlag::InProgress as i32);
        assert_eq!(read_and_free(back.state), "");

        hc_session_free(session);
    }

    #[test]
    fn telex_double_tap_only_triggers_on_consecutive_keys() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "aa"), "â");
        hc_session_reset(session);

        assert_eq!(type_raw(session, &mut req, "aba"), "aba");
        hc_session_reset(session);

        assert_eq!(type_raw(session, &mut req, "aea"), "aea");
        hc_session_reset(session);

        assert_eq!(type_raw(session, &mut req, "dd"), "đ");
        hc_session_reset(session);

        assert_eq!(type_raw(session, &mut req, "ded"), "ded");

        hc_session_free(session);
    }

    #[test]
    fn telex_tone_placement_on_ye_clusters() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "yees"), "yế");
        hc_session_reset(session);

        assert_eq!(type_raw(session, &mut req, "yeef"), "yề");
        hc_session_reset(session);

        assert_eq!(type_raw(session, &mut req, "nyeer"), "nyể");
        hc_session_reset(session);

        assert_eq!(type_raw(session, &mut req, "mex"), "mẽ");

        hc_session_free(session);
    }

    #[test]
    fn reconversion_preserves_mixed_case() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        // Type "HaNoif" - tone goes on last vowel in "aoi" cluster
        assert_eq!(type_raw(session, &mut req, "HaNoif"), "HaNoì");

        // Commit with space
        req.kind = HCKeyKind::Space as i32;
        let space = c(" ");
        req.text = space.as_ptr();
        let commit = hc_session_handle_key(session, &req);
        assert_eq!(commit.state.status_flag, HCStatusFlag::Commit as i32);
        assert_eq!(read_and_free(commit.state), "HaNoì");

        // Backspace to reconvert - should preserve "HaNoi" case pattern
        req.kind = HCKeyKind::Backspace as i32;
        req.text = ptr::null();
        let back = hc_session_handle_key(session, &req);
        assert_eq!(
            back.state.status_flag,
            HCStatusFlag::ReconversionActive as i32
        );
        assert_eq!(read_and_free(back.state), "HaNoì");

        // Now backspace again to remove the tone mark
        req.kind = HCKeyKind::Backspace as i32;
        req.text = ptr::null();
        let back2 = hc_session_handle_key(session, &req);
        // Should show "HaNoi" without tone but with original case preserved
        assert_eq!(read_and_free(back2.state), "HaNoi");

        hc_session_free(session);
    }

    #[test]
    fn spell_check_status_is_set() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        // Type some text and verify spell check status is set
        let result_text = type_raw(session, &mut req, "test");
        assert_eq!(result_text, "tét");

        // Get the state and verify spell_check_status field exists and is set
        let key = c("t");
        req.text = key.as_ptr();
        let result = hc_session_handle_key(session, &req);
        // Just verify the field is present and has a valid value (0, 1, or 2)
        assert!(result.state.spell_check_status >= 0 && result.state.spell_check_status <= 2);
        free_state(result.state);

        hc_session_free(session);
    }

    #[test]
    fn undo_reverts_last_transformation() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "aa"), "â");

        req.kind = HCKeyKind::Undo as i32;
        req.text = ptr::null();
        let undo_result = hc_session_handle_key(session, &req);
        assert_eq!(undo_result.handled, 1);
        assert_eq!(read_and_free(undo_result.state), "a");

        hc_session_free(session);
    }

    #[test]
    fn telex_preserves_vowel_family_when_adding_tones() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "aws"), "ắ");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "aaus"), "ấu");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "muwowif"), "mười");

        hc_session_free(session);
    }

    #[test]
    fn shape_marks_preserve_existing_tones() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "asa"), "ấ");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "asw"), "ắ");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "osw"), "ớ");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "usw"), "ứ");
        hc_session_free(session);

        let session = hc_session_new(InputMode::Vni as i32, 0);
        let mut req = key_request(InputMode::Vni);
        assert_eq!(type_raw(session, &mut req, "a16"), "ấ");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "a18"), "ắ");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "o17"), "ớ");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "u17"), "ứ");
        hc_session_free(session);

        let session = hc_session_new(InputMode::Viqr as i32, 0);
        let mut req = key_request(InputMode::Viqr);
        assert_eq!(type_raw(session, &mut req, "a"), "a");
        let (preedit, _) = send_key(session, &mut req, HCKeyKind::Boundary, "'");
        assert_eq!(preedit, "á");
        let (preedit, _) = send_key(session, &mut req, HCKeyKind::Boundary, "^");
        assert_eq!(preedit, "ấ");

        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "a"), "a");
        let _ = send_key(session, &mut req, HCKeyKind::Boundary, "'");
        let (preedit, _) = send_key(session, &mut req, HCKeyKind::Boundary, "(");
        assert_eq!(preedit, "ắ");

        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "o"), "o");
        let _ = send_key(session, &mut req, HCKeyKind::Boundary, "'");
        let (preedit, _) = send_key(session, &mut req, HCKeyKind::Boundary, "+");
        assert_eq!(preedit, "ớ");

        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "u"), "u");
        let _ = send_key(session, &mut req, HCKeyKind::Boundary, "'");
        let (preedit, _) = send_key(session, &mut req, HCKeyKind::Boundary, "+");
        assert_eq!(preedit, "ứ");
        hc_session_free(session);
    }

    #[test]
    fn telex_backspace_replays_raw_history() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "hoaf"), "hoà");
        req.kind = HCKeyKind::Backspace as i32;
        req.text = ptr::null();
        let back = hc_session_handle_key(session, &req);
        assert_eq!(read_and_free(back.state), "hoa");

        hc_session_free(session);
    }

    #[test]
    fn backspace_consumes_final_preedit_character() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "a"), "a");
        req.kind = HCKeyKind::Backspace as i32;
        req.text = ptr::null();
        let back = hc_session_handle_key(session, &req);
        assert_eq!(back.handled, 1);
        assert_eq!(back.state.status_flag, HCStatusFlag::InProgress as i32);
        assert_eq!(read_and_free(back.state), "");

        hc_session_free(session);
    }

    #[test]
    fn mixed_language_model_falls_back_for_english_collisions() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "rust"), "rút");
        let (committed, status) = commit_with_space(session, &mut req);
        assert_eq!(committed, "rust");
        assert_eq!(status, HCStatusFlag::EnglishFallback as i32);

        hc_session_free(session);
    }

    #[test]
    fn auto_restore_toggle_commits_visible_text_for_collisions() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);
        req.auto_restore = 0;

        assert_eq!(type_raw(session, &mut req, "rust"), "rút");
        let (committed, status) = commit_with_space(session, &mut req);
        assert_eq!(committed, "rút");
        assert_eq!(status, HCStatusFlag::Commit as i32);

        hc_session_free(session);
    }

    #[test]
    fn spell_check_toggle_relaxes_phonotactic_fallback() {
        let strict = language::language_scores("workflow", "workflów", InputMode::Telex, true);
        let relaxed = language::language_scores("workflow", "workflów", InputMode::Telex, false);

        assert!(strict.english > strict.vietnamese);
        assert!(relaxed.vietnamese > strict.vietnamese);
    }

    #[test]
    fn terminal_telex_tone_prefers_valid_vietnamese() {
        let session = hc_session_new(InputMode::Telex as i32, 0);
        let mut req = key_request(InputMode::Telex);

        assert_eq!(type_raw(session, &mut req, "ruts"), "rút");
        let (committed, status) = commit_with_space(session, &mut req);
        assert_eq!(committed, "rút");
        assert_eq!(status, HCStatusFlag::Commit as i32);

        hc_session_free(session);
    }

    #[test]
    fn phonotactic_validation_accepts_vietnamese_shapes_and_rejects_bad_clusters() {
        for key in ["nguyen", "tieng", "quoc", "nguoi", "thich"] {
            assert!(
                language::is_valid_vietnamese_key(key),
                "{key} should be valid"
            );
        }

        for key in ["rust", "config", "workflow", "bld"] {
            assert!(
                !language::is_valid_vietnamese_key(key),
                "{key} should be invalid"
            );
        }
    }

    #[test]
    fn external_bamboo_dictionary_is_used_when_available() {
        if let Some(dictionary) = language::external_vietnamese_dictionary() {
            assert!(dictionary.len() > 1_000);
            assert!(dictionary.contains("sac"));
            assert!(language::is_valid_vietnamese_word("zắc"));
        }
    }

    #[test]
    fn checked_codas_reject_non_entering_tones() {
        assert!(language::is_valid_vietnamese_word("hót"));
        assert!(language::is_valid_vietnamese_word("họt"));
        assert!(!language::is_valid_vietnamese_word("hòt"));
        assert!(!language::is_valid_vietnamese_word("hỏt"));
    }

    #[test]
    fn context_segmentation_tracks_words_numbers_and_boundaries() {
        let segments = language::segment_context("xin_chao 123!");
        let kinds: Vec<SegmentKind> = segments.iter().map(|segment| segment.kind).collect();
        let texts: Vec<&str> = segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect();

        assert_eq!(
            kinds,
            vec![
                SegmentKind::Word,
                SegmentKind::Boundary,
                SegmentKind::Word,
                SegmentKind::Boundary,
                SegmentKind::Number,
                SegmentKind::Boundary
            ]
        );
        assert_eq!(texts, vec!["xin", "_", "chao", " ", "123", "!"]);
    }

    #[test]
    fn vni_tones_use_modern_placement() {
        let session = hc_session_new(InputMode::Vni as i32, 0);
        let mut req = key_request(InputMode::Vni);

        assert_eq!(type_raw(session, &mut req, "hoan2"), "hoàn");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "tuye6n4"), "tuyễn");

        hc_session_free(session);
    }

    #[test]
    fn viqr_composes_traditional_ascii_sequences() {
        let session = hc_session_new(InputMode::Viqr as i32, 0);
        let mut req = key_request(InputMode::Viqr);

        assert_eq!(type_raw(session, &mut req, "a^"), "â");
        let (preedit, status) = send_key(session, &mut req, HCKeyKind::Boundary, "'");
        assert_eq!(preedit, "ấ");
        assert_eq!(status, HCStatusFlag::InProgress as i32);

        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "dd"), "đ");

        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "u+"), "ư");
        let (preedit, _) = send_key(session, &mut req, HCKeyKind::Boundary, "?");
        assert_eq!(preedit, "ử");

        hc_session_free(session);
    }

    #[test]
    fn viqr_non_tone_boundary_commits_current_word() {
        let session = hc_session_new(InputMode::Viqr as i32, 0);
        let mut req = key_request(InputMode::Viqr);

        assert_eq!(type_raw(session, &mut req, "hoa`"), "hoà");
        let (committed, status) = send_key(session, &mut req, HCKeyKind::Boundary, ",");
        assert_eq!(committed, "hoà");
        assert_eq!(status, HCStatusFlag::Commit as i32);

        hc_session_free(session);
    }

    #[test]
    fn vni_d9_produces_d_stroke() {
        let session = hc_session_new(InputMode::Vni as i32, 0);
        let mut req = key_request(InputMode::Vni);
        assert_eq!(type_raw(session, &mut req, "d9"), "đ");
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, "D9"), "Đ");
        hc_session_free(session);
    }

    #[test]
    fn vni_tone_change_on_existing_stroke() {
        let session = hc_session_new(InputMode::Vni as i32, 0);
        let mut req = key_request(InputMode::Vni);
        assert_eq!(type_raw(session, &mut req, "d9uyt1"), "đuýt");
        assert_eq!(type_raw(session, &mut req, "2"), "đuỳt");
        assert_eq!(type_raw(session, &mut req, "5"), "đuỵt");
        hc_session_free(session);
    }

    #[test]
    fn vni_tone_on_ai_goes_to_a() {
        let session = hc_session_new(InputMode::Vni as i32, 0);
        let mut req = key_request(InputMode::Vni);
        assert_eq!(type_raw(session, &mut req, "cai1"), "cái");
        hc_session_free(session);
    }

    #[test]
    fn vni_tone_on_ay_goes_to_a() {
        let session = hc_session_new(InputMode::Vni as i32, 0);
        let mut req = key_request(InputMode::Vni);
        assert_eq!(type_raw(session, &mut req, "may2"), "mày");
        hc_session_free(session);
    }
}
