use crate::test_helpers::*;
use crate::*;
use std::ffi::c_char;
use std::ptr;

/// The shared C header is the ABI contract; the threading rules in it (FFI-02)
/// have no other executable form, so they are asserted here.
const FFI_HEADER: &str = include_str!("../../../linux_fcitx5/include/hcime/hc_core_ffi.h");

fn v2_request(kind: HCKeyKind, text: *const c_char, method: i32, target: u8) -> HC_KeyRequestV2 {
    HC_KeyRequestV2 {
        kind: kind as i32,
        text,
        composition_method: method,
        translation_target: target,
        legacy_tone: 0,
        spell_check: 1,
        auto_restore: 1,
        quick_consonants: 0,
        english_protection: 0,
        macro_in_english: 0,
        esc_restore_raw: 0,
    }
}

fn empty_v2_result() -> HC_KeyResultV2 {
    HC_KeyResultV2 {
        composition_string: ptr::null(),
        composition_len: 0,
        status_flag: 0,
        error_code: 0,
        spell_check_status: 0,
        handled: 0,
        candidates: ptr::null(),
        candidate_count: 0,
        total_candidate_count: 0,
    }
}

/// Reads a borrowed FFI string. Lossy on purpose: a use-after-free must fail an
/// equality assertion, not panic on invalid UTF-8 before it gets there.
fn read_borrowed(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned()
}

fn v4_composition(result: &HC_KeyResultV2) -> String {
    read_borrowed(
        result.composition_string as *const u8,
        result.composition_len,
    )
}

/// Hands the allocator a flood of same-sized blocks so a freed buffer of that
/// size is handed back out and overwritten. Without this, a use-after-free read
/// often still returns the original bytes and the regression test passes by
/// luck. The returned vector must stay alive for the reads under test.
#[must_use]
fn churn_allocator(byte_len: usize) -> Vec<String> {
    let filler = "Z".repeat(byte_len.max(1));
    (0..4096).map(|_| filler.clone()).collect()
}

fn type_v4(session: *mut std::ffi::c_void, text: &str, method: i32, target: u8) -> HC_KeyResultV2 {
    let mut result = empty_v2_result();
    for ch in text.chars() {
        let key = c(&ch.to_string());
        let request = v2_request(HCKeyKind::Printable, key.as_ptr(), method, target);
        result = empty_v2_result();
        hc_session_handle_key_v4(session, &request, &mut result);
    }
    result
}

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

// ── FFI-04: InputMode::try_from must be able to fail ──

#[test]
fn invalid_input_mode_is_rejected_at_every_entry_point() {
    // The header returns void*, which implies NULL on failure; every caller's
    // `if (!session)` guard was unreachable because 99 became Telex.
    for mode in [6, 99, -1, i32::MIN, i32::MAX] {
        let session = hc_session_new(mode, 0);
        assert!(
            session.is_null(),
            "hc_session_new({mode}) must fail, got a working session"
        );
    }

    let session = hc_session_new(InputMode::Telex as i32, 0);
    assert!(
        !session.is_null(),
        "a valid mode must still build a session"
    );

    let a = c("a");
    let mut req = key_request(InputMode::Telex);
    req.text = a.as_ptr();
    req.input_mode = 99;
    let result = hc_session_handle_key(session, &req);
    assert_eq!(result.handled, 0, "an invalid mode must not be handled");
    assert_eq!(
        result.state.error_code,
        HCErrorCode::InvalidInputMode as i32,
        "an invalid mode must report HC_ERROR_INVALID_INPUT_MODE"
    );
    free_state(result.state);

    // The same dead arm existed in the rehydrate path.
    let word = c("hoàn");
    let rehydrated = hc_rehydrate_apply(word.as_ptr(), 99, EditTrigger::Cancel as i32, 0);
    assert_eq!(
        rehydrated.error_code,
        HCErrorCode::InvalidInputMode as i32,
        "hc_rehydrate_apply must reject an invalid mode"
    );
    free_state(rehydrated);

    hc_session_free(session);
}

// ── FFI-03: composition_method + 3 was unvalidated ──

#[test]
fn v4_rejects_out_of_range_composition_method() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let a = c("a");

    // i32::MAX overflowed the `+ 3` outright: an abort in debug builds, a
    // wrapped negative mode in release. The rest silently became Telex.
    for method in [3, 99, -1, i32::MAX, i32::MIN] {
        let request = v2_request(
            HCKeyKind::Printable,
            a.as_ptr(),
            method,
            TRANSLATION_TARGET_HAN_NOM,
        );
        let mut result = empty_v2_result();
        let handled = hc_session_handle_key_v4(session, &request, &mut result);
        assert_eq!(
            handled, 0,
            "composition_method {method} must not be handled"
        );
        assert_eq!(result.handled, 0, "composition_method {method}: handled");
        assert_eq!(
            result.error_code,
            HCErrorCode::InvalidInputMode as i32,
            "composition_method {method} must report HC_ERROR_INVALID_INPUT_MODE"
        );
        assert!(result.composition_string.is_null());
        assert!(result.candidates.is_null());
    }

    // The Vietnamese target is validated on the same range.
    let request = v2_request(
        HCKeyKind::Printable,
        a.as_ptr(),
        99,
        TRANSLATION_TARGET_VIETNAMESE,
    );
    let mut result = empty_v2_result();
    assert_eq!(hc_session_handle_key_v4(session, &request, &mut result), 0);
    assert_eq!(result.error_code, HCErrorCode::InvalidInputMode as i32);

    // A valid method still works after the rejections.
    let request = v2_request(
        HCKeyKind::Printable,
        a.as_ptr(),
        0,
        TRANSLATION_TARGET_HAN_NOM,
    );
    let mut result = empty_v2_result();
    assert_eq!(hc_session_handle_key_v4(session, &request, &mut result), 1);
    assert_eq!(result.error_code, HCErrorCode::None as i32);

    hc_session_free(session);
}

// ── FFI-08: no length bound at the ABI boundary ──

#[test]
fn oversized_key_text_is_rejected_before_composition() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let huge = c(&"a".repeat(100_000));

    let mut req = key_request(InputMode::Telex);
    req.text = huge.as_ptr();
    let result = hc_session_handle_key(session, &req);
    assert_eq!(result.handled, 0, "an oversized key must not be handled");
    assert_eq!(
        result.state.error_code,
        HCErrorCode::TextTooLong as i32,
        "an oversized key must report HC_ERROR_TEXT_TOO_LONG"
    );
    free_state(result.state);

    let request = v2_request(
        HCKeyKind::Printable,
        huge.as_ptr(),
        0,
        TRANSLATION_TARGET_VIETNAMESE,
    );
    let mut result = empty_v2_result();
    assert_eq!(
        hc_session_handle_key_v4(session, &request, &mut result),
        0,
        "v4 must decline an oversized key"
    );
    assert_eq!(result.error_code, HCErrorCode::TextTooLong as i32);

    // Rejected at the boundary means the composition never saw it.
    let after = type_v4(session, "a", 0, TRANSLATION_TARGET_VIETNAMESE);
    assert_eq!(v4_composition(&after), "a");

    hc_session_free(session);

    // The Hán Nôm branch is bounded by the same check, one call earlier than
    // the translator's own raw_buffer guard.
    let nom = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let request = v2_request(
        HCKeyKind::Printable,
        huge.as_ptr(),
        0,
        TRANSLATION_TARGET_HAN_NOM,
    );
    let mut result = empty_v2_result();
    assert_eq!(hc_session_handle_key_v4(nom, &request, &mut result), 0);
    assert_eq!(result.error_code, HCErrorCode::TextTooLong as i32);
    hc_session_free(nom);

    // A key at the limit is still accepted: the bound is on garbage, not on
    // legitimate multi-byte keystrokes.
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let at_limit = c(&"a".repeat(HC_MAX_KEY_TEXT_BYTES));
    let request = v2_request(
        HCKeyKind::Printable,
        at_limit.as_ptr(),
        0,
        TRANSLATION_TARGET_VIETNAMESE,
    );
    let mut result = empty_v2_result();
    assert_eq!(hc_session_handle_key_v4(session, &request, &mut result), 1);
    assert_eq!(result.error_code, HCErrorCode::None as i32);
    hc_session_free(session);
}

// ── FFI-05: v4's output pointer had two owners and two lifetimes ──

#[test]
fn v4_composition_pointer_is_library_owned_on_both_branches() {
    // Vietnamese branch: thread-local, survives the session.
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let result = type_v4(session, "a", 0, TRANSLATION_TARGET_VIETNAMESE);
    assert_eq!(v4_composition(&result), "a");
    let (vi_ptr, vi_len) = (
        result.composition_string as *const u8,
        result.composition_len,
    );
    hc_session_free(session);
    let _churn = churn_allocator(vi_len);
    assert_eq!(
        read_borrowed(vi_ptr, vi_len),
        "a",
        "the Vietnamese branch's composition_string must outlive the session"
    );

    // Hán Nôm branch: used to point straight into the translator, so the same
    // field died with the session while the Vietnamese one did not.
    let nom = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let result = type_v4(nom, "thien", 0, TRANSLATION_TARGET_HAN_NOM);
    let reading = v4_composition(&result);
    assert_eq!(
        reading, "thien",
        "Hán Nôm reading before the session is freed"
    );
    let (nom_ptr, nom_len) = (
        result.composition_string as *const u8,
        result.composition_len,
    );
    hc_session_free(nom);
    let _churn = churn_allocator(nom_len);
    assert_eq!(
        read_borrowed(nom_ptr, nom_len),
        reading,
        "the Hán Nôm branch must follow the same rule as the Vietnamese one: \
         composition_string is library-owned and is not freed with the session"
    );
}

// ── FFI-06: hc_session_reset invalidated borrowed candidate pointers ──

#[test]
fn v4_candidate_pointers_survive_session_reset() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let result = type_v4(session, "thien", 0, TRANSLATION_TARGET_HAN_NOM);
    assert!(
        result.candidate_count > 0 && !result.candidates.is_null(),
        "a completed reading must expose candidates"
    );

    // Exactly what a frontend caches: the outer array and the first entry.
    let array = result.candidates;
    let first = unsafe { *result.candidates };
    let before = read_borrowed(first.text, first.text_len as usize);
    let reading_before = read_borrowed(first.reading, first.reading_len as usize);
    assert!(!before.is_empty(), "the first candidate must have text");

    hc_session_reset(session);
    // The outer array keeps its address across the invalidation, so a cached
    // pointer looks healthy while the strings inside it are freed.
    let _churn = churn_allocator(first.text_len as usize);
    let _churn_reading = churn_allocator(first.reading_len as usize);

    let after = unsafe { *array };
    assert_eq!(
        read_borrowed(after.text, after.text_len as usize),
        before,
        "hc_session_reset must not invalidate candidate text handed to the caller"
    );
    assert_eq!(
        read_borrowed(after.reading, after.reading_len as usize),
        reading_before,
        "hc_session_reset must not invalidate candidate readings"
    );

    hc_session_free(session);
}

// ── FFI-02: the threading contract was undocumented ──

#[test]
fn header_documents_the_threading_contract() {
    for phrase in [
        "Threading contract",
        "must be serialised",
        "undefined behaviour",
        "Distinct sessions may be used concurrently",
        "Pointer ownership and lifetime",
    ] {
        assert!(
            FFI_HEADER.contains(phrase),
            "hc_core_ffi.h must state the threading/ownership contract; missing: {phrase:?}"
        );
    }

    // The supported model, exercised: separate sessions on separate threads,
    // including concurrent macro-map mutation.
    let threads: Vec<_> = (0..2)
        .map(|id| {
            std::thread::spawn(move || {
                let session = hc_session_new(InputMode::Telex as i32, 0);
                assert!(!session.is_null());
                let key = c("t");
                let value = c(&format!("thread{id}"));
                for _ in 0..500 {
                    hc_session_add_macro(session, key.as_ptr(), value.as_ptr());
                    let request = v2_request(
                        HCKeyKind::Printable,
                        key.as_ptr(),
                        0,
                        TRANSLATION_TARGET_VIETNAMESE,
                    );
                    let mut result = empty_v2_result();
                    hc_session_handle_key_v4(session, &request, &mut result);
                    assert_eq!(result.error_code, HCErrorCode::None as i32);
                }
                hc_session_free(session);
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("separate sessions must not interfere");
    }
}
