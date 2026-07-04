use super::*;
use std::ffi::CString;
use std::ptr;

pub fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

pub fn read_state(state: HC_State) -> String {
    if state.composition_string.is_null() || state.length == 0 {
        return String::new();
    }
    let slice = unsafe { std::slice::from_raw_parts(state.composition_string, state.length) };
    String::from_utf16(slice).unwrap()
}

pub fn free_state(mut state: HC_State) {
    hc_state_free(&mut state);
}

pub fn read_and_free(mut state: HC_State) -> String {
    let text = read_state(state);
    hc_state_free(&mut state);
    text
}

pub fn key_request(mode: InputMode) -> HC_KeyRequest {
    HC_KeyRequest {
        kind: HCKeyKind::Printable as i32,
        text: ptr::null(),
        input_mode: mode as i32,
        legacy_tone: 0,
        spell_check: 1,
        auto_restore: 1,
        quick_consonants: 0,
        english_protection: 0,
        macro_in_english: 0,
        esc_restore_raw: 0,
    }
}

pub fn type_raw(session: *mut std::ffi::c_void, req: &mut HC_KeyRequest, raw: &str) -> String {
    let mut latest = String::new();
    req.kind = HCKeyKind::Printable as i32;
    for ch in raw.chars() {
        let key = c(&ch.to_string());
        req.text = key.as_ptr();
        latest = read_and_free(hc_session_handle_key(session, req).state);
    }
    latest
}

pub fn send_key(
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

pub fn commit_with_space(session: *mut std::ffi::c_void, req: &mut HC_KeyRequest) -> (String, i32) {
    req.kind = HCKeyKind::Space as i32;
    let space = c(" ");
    req.text = space.as_ptr();
    let result = hc_session_handle_key(session, req);
    let status = result.state.status_flag;
    (read_and_free(result.state), status)
}
