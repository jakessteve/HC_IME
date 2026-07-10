use super::test_helpers::*;
use super::*;
use std::ptr;
use std::time::Duration;

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
        quick_consonants: 0,
        english_protection: 0,
        macro_in_english: 0,
        esc_restore_raw: 0,
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
fn telex_z_is_literal_unless_it_cancels_marks() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "az"), "az");
    hc_session_reset(session);

    assert_eq!(type_raw(session, &mut req, "asz"), "a");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "a");
    assert_eq!(status, HCStatusFlag::Commit as i32);

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
        quick_consonants: 0,
        english_protection: 0,
        macro_in_english: 0,
        esc_restore_raw: 0,
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
fn vni_zero_is_literal_unless_it_cancels_marks() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    for ch in ["1", "0", "1", "2", "3", "0"] {
        let key = c(ch);
        req.text = key.as_ptr();
        let res = hc_session_handle_key(session, &req);
        assert_eq!(res.handled, 0, "standalone VNI digit {ch} passes through");
        assert_eq!(read_and_free(res.state), "");
    }

    assert_eq!(type_raw(session, &mut req, "a0"), "a0");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "");
    assert_eq!(status, HCStatusFlag::InProgress as i32);
    hc_session_reset(session);

    assert_eq!(type_raw(session, &mut req, "a10"), "a");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "a");
    assert_eq!(status, HCStatusFlag::Commit as i32);
    req.kind = HCKeyKind::Printable as i32;
    hc_session_reset(session);

    assert_eq!(type_raw(session, &mut req, "u70"), "u");

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
        quick_consonants: 0,
        english_protection: 0,
        macro_in_english: 0,
        esc_restore_raw: 0,
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
fn vni_spaced_commit_can_be_reopened_for_tone_change_within_timeout() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    assert_eq!(type_raw(session, &mut req, "ca1"), "cá");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "cá");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(
        back.state.status_flag,
        HCStatusFlag::ReconversionActive as i32
    );
    assert_eq!(read_and_free(back.state), "cá");

    req.kind = HCKeyKind::Printable as i32;
    let two = c("2");
    req.text = two.as_ptr();
    let edit = hc_session_handle_key(session, &req);
    assert_eq!(edit.state.status_flag, HCStatusFlag::InProgress as i32);
    assert_eq!(read_and_free(edit.state), "cà");

    hc_session_free(session);
}

#[test]
fn spaced_commit_edit_window_expires() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    assert_eq!(type_raw(session, &mut req, "ca1"), "cá");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "cá");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    std::thread::sleep(Duration::from_millis(1600));

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(back.handled, 0);
    free_state(back.state);

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
fn telex_shape_trigger_commit_prefers_vietnamese_collision() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "moo"), "mô");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "mô");
    assert_eq!(status, HCStatusFlag::Commit as i32);

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
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "ne6u1"), "nếu");

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

#[test]
fn tone_after_qu_glide_goes_to_main_vowel() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "quas"), "quá");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "quis"), "quí");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "ques"), "qué");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "quaf"), "quà");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "quos"), "quó");

    hc_session_free(session);

    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    assert_eq!(type_raw(session, &mut req, "qua1"), "quá");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "qui1"), "quí");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "qua2"), "quà");

    hc_session_free(session);
}

#[test]
fn tone_after_gi_glide_goes_to_main_vowel() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "giar"), "giả");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "gias"), "giá");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "giof"), "giò");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "gior"), "giỏ");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "giuj"), "giụ");

    hc_session_free(session);

    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    assert_eq!(type_raw(session, &mut req, "gia3"), "giả");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "gia1"), "giá");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "giu5"), "giụ");

    hc_session_free(session);
}

#[test]
fn legacy_tone_keeps_first_vowel_for_qu_and_gi() {
    let session = hc_session_new(InputMode::Telex as i32, 1);
    let mut req = key_request(InputMode::Telex);
    req.legacy_tone = 1;

    assert_eq!(type_raw(session, &mut req, "quas"), "qúa");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "gias"), "gía");

    hc_session_free(session);

    let session = hc_session_new(InputMode::Vni as i32, 1);
    let mut req = key_request(InputMode::Vni);
    req.legacy_tone = 1;

    assert_eq!(type_raw(session, &mut req, "qua1"), "qúa");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "gia1"), "gía");

    hc_session_free(session);
}

#[test]
fn tone_after_qu_glide_handles_mixed_case() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "Quas"), "Quá");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "QUas"), "QUá");

    hc_session_free(session);
}

#[test]
fn triphthong_oay_places_tone_on_a() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "ngoays"), "ngoáy");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "ngoayr"), "ngoảy");

    hc_session_free(session);

    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    assert_eq!(type_raw(session, &mut req, "ngoay1"), "ngoáy");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "ngoay3"), "ngoảy");

    hc_session_free(session);
}

#[test]
fn vni_horn_applies_to_all_u_and_o_in_one_press() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    assert_eq!(type_raw(session, &mut req, "phuong7"), "phương");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "tuong7"), "tương");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "truong7"), "trương");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "thuong7"), "thương");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "uoc7"), "ươc");

    hc_session_free(session);
}

#[test]
fn telex_w_applies_horn_to_both_u_and_o_when_both_present() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "phuongw"), "phương");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "truongw"), "trương");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "thuongw"), "thương");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "uocw"), "ươc");

    hc_session_free(session);
}

#[test]
fn telex_w_applies_breve_when_no_uo_pair() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "aw"), "ă");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "ow"), "ơ");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "uw"), "ư");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "hoanw"), "hoăn");

    hc_session_free(session);
}

#[test]
fn telex_w_smart_horn_ua_becomes_horn_u() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    // "muaw" → "mưa" (horn on u, not breve on a)
    assert_eq!(type_raw(session, &mut req, "muaw"), "mưa");
    hc_session_reset(session);

    // "xuaw" → "xưa" (same rule)
    assert_eq!(type_raw(session, &mut req, "xuaw"), "xưa");
    hc_session_reset(session);

    // "quaw" → "quă" (qu glide exception: breve on a)
    assert_eq!(type_raw(session, &mut req, "quaw"), "quă");
    hc_session_reset(session);

    // "luawr" → "lửa" (horn on u via w, then tone via r)
    assert_eq!(type_raw(session, &mut req, "luawr"), "lửa");

    hc_session_free(session);
}

#[test]
fn casing_preservation_all_caps_and_title_case() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    // ALL CAPS: "MUAW" → "MƯA" (horn on Ư, not breve on A)
    assert_eq!(type_raw(session, &mut req, "MUAW"), "MƯA");
    hc_session_reset(session);

    // Title case: "Muaw" → "Mưa" (horn on ư)
    assert_eq!(type_raw(session, &mut req, "Muaw"), "Mưa");
    hc_session_reset(session);

    // ALL CAPS with tone: "HOAF" → "HOÀ"
    assert_eq!(type_raw(session, &mut req, "HOAF"), "HOÀ");
    hc_session_reset(session);

    // Title case with circumflex: "Aas" → "Ấ"
    assert_eq!(type_raw(session, &mut req, "Aas"), "Ấ");
    hc_session_reset(session);

    // ALL CAPS circumflex+tone: "AAS" → "Ấ" (uppercase)
    assert_eq!(type_raw(session, &mut req, "AAS"), "Ấ");
    hc_session_reset(session);

    // ALL CAPS with ươ pair: "PHUONGW" → "PHƯƠNG"
    assert_eq!(type_raw(session, &mut req, "PHUONGW"), "PHƯƠNG");

    hc_session_free(session);
}

#[test]
fn casing_normalization_erratic_mixed_case_not_forced() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    // Mixed case like "HaNoi" (H upper, a lower, N upper) → not a uniform
    // pattern, so per-character casing is preserved
    assert_eq!(type_raw(session, &mut req, "HaNoif"), "HaNoì");
    hc_session_reset(session);

    // True Title Case: "Tieeengs" → "Tiếng" (first upper, all rest lower)
    assert_eq!(type_raw(session, &mut req, "Tieengs"), "Tiếng");

    hc_session_free(session);
}

#[test]
fn macro_expansion_replaces_raw_key_on_commit() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.macro_in_english = 1;

    let macro_key = c("vn");
    let macro_val = c("Việt Nam");
    hc_session_add_macro(session, macro_key.as_ptr(), macro_val.as_ptr());

    assert_eq!(type_raw(session, &mut req, "vn"), "vn");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "Việt Nam");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "hoaf"), "hoà");
    let (committed, _) = commit_with_space(session, &mut req);
    assert_eq!(committed, "hoà");

    hc_session_free(session);
}

#[test]
fn clear_macros_removes_all_registered_macros() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    let macro_key = c("vn");
    let macro_val = c("Việt Nam");
    hc_session_add_macro(session, macro_key.as_ptr(), macro_val.as_ptr());

    // Clear macros
    hc_session_clear_macros(session);

    // Now "vn" should NOT expand
    assert_eq!(type_raw(session, &mut req, "vn"), "vn");
    let (committed, _) = commit_with_space(session, &mut req);
    assert_eq!(committed, "vn");

    hc_session_free(session);
}

#[test]
fn backspace_deletes_visible_char_in_vni_mode() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Single base + trigger: backspace deletes entire composed char
    assert_eq!(type_raw(session, &mut req, "u7"), "ư");
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(back.handled, 1);
    assert_eq!(read_and_free(back.state), "");

    hc_session_reset(session);

    // Multi-char base + trigger: backspace deletes last visible char
    assert_eq!(type_raw(session, &mut req, "phuong7"), "phương");
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phươn");

    // Second backspace deletes 'n'
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phươ");

    // Third backspace deletes 'ơ'
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phư");

    // Fourth backspace deletes 'ư' (and its orphaned trigger)
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "ph");

    hc_session_reset(session);

    // When the deleted vowel carries the VNI tone, the tone must not jump to
    // the previous vowel before the visible character is removed.
    assert_eq!(type_raw(session, &mut req, "phuong73"), "phưởng");
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phưởn");

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phưở");

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phư");

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "ph");

    hc_session_reset(session);

    assert_eq!(type_raw(session, &mut req, "phuong37"), "phưởng");
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phưởn");

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phưở");

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phư");

    hc_session_free(session);
}

#[test]
fn telex_backspace_deletes_one_raw_character() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    assert_eq!(type_raw(session, &mut req, "uw"), "ư");
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "u");

    hc_session_free(session);
}

#[test]
fn quick_consonants_mid_word_cc_to_ch() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 1;

    assert_eq!(type_raw(session, &mut req, "cc"), "ch");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "cco"), "cho");

    hc_session_free(session);
}

#[test]
fn quick_consonants_mid_word_nn_to_ng() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 1;

    assert_eq!(type_raw(session, &mut req, "nn"), "ng");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "nna"), "nga");

    hc_session_free(session);
}

#[test]
fn quick_consonants_mid_word_gg_to_gi() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 1;

    assert_eq!(type_raw(session, &mut req, "gg"), "gi");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "gga"), "gia");

    hc_session_free(session);
}

#[test]
fn quick_consonants_mid_word_uu_to_uo() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 1;

    let result = type_raw(session, &mut req, "uu");
    assert!(result.contains('ư'));

    hc_session_free(session);
}

#[test]
fn quick_consonants_start_f_to_ph() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 1;

    assert_eq!(type_raw(session, &mut req, "fo"), "pho");

    hc_session_free(session);
}

#[test]
fn quick_consonants_start_j_to_gi() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 1;

    assert_eq!(type_raw(session, &mut req, "ja"), "gia");

    hc_session_free(session);
}

#[test]
fn quick_consonants_start_w_to_qu() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 1;

    assert_eq!(type_raw(session, &mut req, "wa"), "qua");

    hc_session_free(session);
}

#[test]
fn quick_consonants_end_g_to_ng() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 1;

    assert_eq!(type_raw(session, &mut req, "tag"), "tag");
    let (committed, _) = commit_with_space(session, &mut req);
    assert_eq!(committed, "tang");

    hc_session_free(session);
}

#[test]
fn quick_consonants_disabled_by_default() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.quick_consonants = 0;

    assert_eq!(type_raw(session, &mut req, "cc"), "cc");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "fo"), "fo");

    hc_session_free(session);
}

#[test]
fn english_protection_hard_rejects_impossible_starts() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.english_protection = 2;

    assert_eq!(type_raw(session, &mut req, "cl"), "cl");

    let key = c("o");
    req.text = key.as_ptr();
    let result = hc_session_handle_key(session, &req);
    assert_eq!(
        result.state.spell_check_status,
        HCSpellCheckStatus::EnglishFallback as i32
    );
    free_state(result.state);

    hc_session_free(session);
}

#[test]
fn english_protection_soft_rejects_y_vowel() {
    assert!(language::is_soft_english_pattern("ya"));
    assert!(language::is_soft_english_pattern("ye"));
    assert!(!language::is_soft_english_pattern("y"));
    assert!(!language::is_soft_english_pattern("abc"));
}

#[test]
fn english_protection_off_allows_all() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.english_protection = 0;

    assert_eq!(type_raw(session, &mut req, "cl"), "cl");

    let key = c("o");
    req.text = key.as_ptr();
    let result = hc_session_handle_key(session, &req);
    free_state(result.state);

    hc_session_free(session);
}

#[test]
fn macro_expands_in_english_mode_when_enabled() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.macro_in_english = 1;

    let macro_key = c("vn");
    let macro_val = c("Việt Nam");
    hc_session_add_macro(session, macro_key.as_ptr(), macro_val.as_ptr());

    assert_eq!(type_raw(session, &mut req, "vn"), "vn");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "Việt Nam");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    hc_session_free(session);
}

#[test]
fn macro_does_not_expand_in_english_mode_when_disabled() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.macro_in_english = 0;

    let macro_key = c("vn");
    let macro_val = c("Việt Nam");
    hc_session_add_macro(session, macro_key.as_ptr(), macro_val.as_ptr());

    assert_eq!(type_raw(session, &mut req, "vn"), "vn");
    let (committed, _) = commit_with_space(session, &mut req);
    assert_eq!(committed, "vn");

    hc_session_free(session);
}

#[test]
fn esc_restore_raw_returns_raw_keystrokes() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.esc_restore_raw = 1;

    assert_eq!(type_raw(session, &mut req, "vis"), "ví");

    req.kind = HCKeyKind::Escape as i32;
    req.text = ptr::null();
    let result = hc_session_handle_key(session, &req);
    assert_eq!(
        result.state.status_flag,
        HCStatusFlag::EscRestoredRaw as i32
    );
    assert_eq!(read_and_free(result.state), "vis");

    hc_session_free(session);
}

#[test]
fn esc_without_restore_flag_resets_normally() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.esc_restore_raw = 0;

    assert_eq!(type_raw(session, &mut req, "vis"), "ví");

    req.kind = HCKeyKind::Escape as i32;
    req.text = ptr::null();
    let result = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(result.state), "");

    hc_session_free(session);
}
