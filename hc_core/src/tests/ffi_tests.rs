use crate::test_helpers::*;
use crate::*;
use std::ptr;

#[test]
fn utf8_key_result_matches_state_output() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let a = c("a");
    let req = HC_KeyRequest {
        kind: HCKeyKind::Printable as i32,
        text: a.as_ptr(),
        input_mode: InputMode::Telex as i32,
        legacy_tone: 0,
        spell_check: 1,
        auto_restore: 1,
        quick_consonants: 0,
        english_protection: 0,
        macro_in_english: 0,
        esc_restore_raw: 0,
    };

    let result = hc_session_handle_key_utf8(session, &req);
    assert_eq!(result.handled, 1);
    assert_eq!(result.status_flag, HCStatusFlag::InProgress as i32);
    let slice = unsafe {
        std::slice::from_raw_parts(result.composition_string as *const u8, result.length)
    };
    assert_eq!(std::str::from_utf8(slice).unwrap(), "a");

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
