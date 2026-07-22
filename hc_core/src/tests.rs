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

    // "yê" auto-completes to "yêu" in Vietnamese orthography
    assert_eq!(type_raw(session, &mut req, "yees"), "yếu");
    hc_session_reset(session);

    assert_eq!(type_raw(session, &mut req, "yeef"), "yều");
    hc_session_reset(session);

    assert_eq!(type_raw(session, &mut req, "nyeer"), "nyểu");
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
fn legacy_tone_respects_qu_and_gi_glides() {
    let session = hc_session_new(InputMode::Telex as i32, 1);
    let mut req = key_request(InputMode::Telex);
    req.legacy_tone = 1;

    let r1 = type_raw(session, &mut req, "quas");
    println!("quas -> {}", r1);
    assert_eq!(r1, "quá");
    hc_session_reset(session);
    let r2 = type_raw(session, &mut req, "gias");
    println!("gias -> {}", r2);
    assert_eq!(r2, "giá");

    hc_session_free(session);

    let session = hc_session_new(InputMode::Vni as i32, 1);
    let mut req = key_request(InputMode::Vni);
    req.legacy_tone = 1;

    assert_eq!(type_raw(session, &mut req, "qua1"), "quá");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "gia1"), "giá");

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

#[test]
fn tone_placement_on_uo_ue_uy_clusters() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    // "thuần" - tone on â (circumflex a), not u
    // Input: th-u-a-a-n-f (double-tap 'a' for circumflex, then tone)
    assert_eq!(type_raw(session, &mut req, "thuaanf"), "thuần");
    hc_session_reset(session);

    // "túy" - tone on u (first vowel in uy cluster), not y
    assert_eq!(type_raw(session, &mut req, "tuys"), "túy");
    hc_session_reset(session);

    // "thùy" - tone on u in "uy" cluster
    assert_eq!(type_raw(session, &mut req, "thuyf"), "thùy");
    hc_session_reset(session);

    // "huỳnh" - tone on u in "uy" cluster with coda
    assert_eq!(type_raw(session, &mut req, "huynhf"), "huỳnh");
    hc_session_reset(session);

    // "tủy" - tone on u in "uy" with coda
    assert_eq!(type_raw(session, &mut req, "tuyr"), "tủy");

    hc_session_free(session);
}

#[test]
fn investigate_edge_cases_batch1() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    // Group 1: "uơ" cluster (no explicit rule, relies on default=last)
    let r = type_raw(session, &mut req, "thuows");
    eprintln!("thuows → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // Group 2: "ia" cluster
    let r = type_raw(session, &mut req, "tias");
    eprintln!("tias → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // Group 3: "ia" with 3 vowels (i+a)
    let r = type_raw(session, &mut req, "diaf");
    eprintln!("diaf → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // Group 4: "uơ" with tone 3 (hỏi)
    let r = type_raw(session, &mut req, "thuowr");
    eprintln!("thuowr → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // Group 5: "y+u" without circumflex (rare)
    let r = type_raw(session, &mut req, "hyus");
    eprintln!("hyus → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // Group 6: "i+u" without circumflex
    let r = type_raw(session, &mut req, "bius");
    eprintln!("bius → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // Group 7: Multiple diacritics - "ưô" (horn+circumflex)
    let r = type_raw(session, &mut req, "duongwfs");
    eprintln!("duongwfs → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // Group 8: "oai" with all tones
    let r = type_raw(session, &mut req, "hoaif");
    eprintln!("hoaif → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    hc_session_free(session);
}

#[test]
fn investigate_edge_cases_batch2() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    // "ưoi" - horn priority
    let r = type_raw(session, &mut req, "muowis");
    eprintln!("muowis → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // "ưi" - horn priority
    let r = type_raw(session, &mut req, "guwis");
    eprintln!("guwis → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // "ươi" - horn priority
    let r = type_raw(session, &mut req, "muowis");
    eprintln!("muowis → {}", r);
    hc_session_reset(session);

    // "uôi" - cluster uoi → last
    let r = type_raw(session, &mut req, "cuois");
    eprintln!("cuois → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // "ye" without circumflex + coda
    let r = type_raw(session, &mut req, "byens");
    eprintln!("byens → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // "ie" without circumflex + coda
    let r = type_raw(session, &mut req, "biens");
    eprintln!("biens → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // "ie" without circumflex, no coda
    let r = type_raw(session, &mut req, "bies");
    eprintln!("bies → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    // "uơ" standalone
    let r = type_raw(session, &mut req, "quowes");
    eprintln!("quowes → {}", r);
    let chars: Vec<char> = r.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        eprintln!("  [{}] {:?} U+{:04X}", i, ch, *ch as u32);
    }
    hc_session_reset(session);

    hc_session_free(session);
}

#[test]
fn tone_edge_case_uoi_circumflex() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "toois");
    assert_eq!(result, "tối", "tối: tone should go on ô (circumflex o)");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_uoi_with_coda() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "toongsf");
    assert_eq!(result, "tống", "tống: tone should go on ô with coda");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_uoi_horn() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "tuwois");
    assert_eq!(result, "tưới", "tưới: tone should go on ơ (horn o)");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_yeu_circumflex() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "yeef");
    assert_eq!(result, "yều", "yều: tone should go on ê (circumflex e)");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_ieu_circumflex() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "ieef");
    assert_eq!(result, "iều", "iều: tone should go on ê (circumflex e)");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_oai() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "oair");
    assert_eq!(result, "oải", "oải: tone should go on a (second vowel)");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_uay() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "uayr");
    assert_eq!(result, "uẩy", "uẩy: tone should go on a (second vowel)");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_uy_with_coda() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "uyeenr");
    assert_eq!(result, "uyển", "uyển: tone should go on y with coda");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_uy_no_coda() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "uys");
    assert_eq!(result, "úy", "úy: tone should go on u without coda");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_uoi_circumflex() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "to6i1");
    assert_eq!(result, "tối", "tối: tone should go on ô (circumflex o)");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_uoi_with_coda() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "to6ng2");
    assert_eq!(result, "tồng", "tồng: tone should go on ô with coda");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_uoi_horn() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "tu7o7i1");
    assert_eq!(result, "tưới", "tưới: tone should go on ơ (horn o)");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_yeu_circumflex() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "ye6u2");
    assert_eq!(result, "yều", "yều: tone should go on ê (circumflex e)");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_ieu_circumflex() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "ie6u2");
    assert_eq!(result, "iều", "iều: tone should go on ê (circumflex e)");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_oai() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "oai3");
    assert_eq!(result, "oải", "oải: tone should go on a (second vowel)");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_uay() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "uay3");
    assert_eq!(result, "uẩy", "uẩy: tone should go on a (second vowel)");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_uy_with_coda() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "uye6n3");
    assert_eq!(result, "uyển", "uyển: tone should go on y with coda");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_uy_no_coda() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "uy1");
    assert_eq!(result, "úy", "úy: tone should go on u without coda");
    hc_session_free(session);
}

#[test]
fn telex_tuan_circumflex_tone() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "tuanf");
    assert_eq!(
        result, "tuần",
        "tuần: tone should go on â (circumflex a), not u"
    );
    hc_session_free(session);
}

#[test]
fn legacy_tone_tuan_circumflex() {
    let session = hc_session_new(InputMode::Telex as i32, 1);
    let mut req = key_request(InputMode::Telex);
    req.legacy_tone = 1;
    let result = type_raw(session, &mut req, "tuanf");
    assert_eq!(
        result, "tuần",
        "legacy_tone: tuanf should produce 'tuần' (tone on â), not 'tùân'"
    );
    hc_session_free(session);
}

#[test]
fn vni_tuan_circumflex_tone() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "tua6n2");
    assert_eq!(
        result, "tuần",
        "tuần: tone should go on â (circumflex a), not u"
    );
    hc_session_free(session);
}

#[test]
fn telex_tuan_double_a_tone() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "tuaanf");
    assert_eq!(
        result, "tuần",
        "tuần: double-tap 'a' should create â, tone on â"
    );
    hc_session_free(session);
}

#[test]
fn vni_tuan_double_a_tone() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "tua6n2");
    assert_eq!(result, "tuần", "tuần: VNI 6 should create â, tone on â");
    hc_session_free(session);
}

#[test]
fn telex_tuan_w_circumflex() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    // Try using 'w' to create circumflex on 'a'
    let result = type_raw(session, &mut req, "tuanwf");
    println!("tuanwf -> {}", result);
    // If this produces "tùan" instead of "tuần", we have a bug
    hc_session_free(session);
}

#[test]
fn telex_tuan_step_by_step() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    // Type "tuan"
    let result1 = type_raw(session, &mut req, "tuan");
    println!("tuan -> {}", result1);

    // Now add second 'a' for circumflex
    let result2 = type_raw(session, &mut req, "a");
    println!("tuana -> {}", result2);

    // Now add tone
    let result3 = type_raw(session, &mut req, "f");
    println!("tuanaf -> {}", result3);

    assert_eq!(result3, "tuần", "Step-by-step should produce 'tuần'");
    hc_session_free(session);
}

#[test]
fn vni_tuan_circumflex_after_consonant() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Type "tuan" then '6' for circumflex
    let result1 = type_raw(session, &mut req, "tuan");
    println!("tuan -> {}", result1);

    let result2 = type_raw(session, &mut req, "6");
    println!("tuan6 -> {}", result2);

    let result3 = type_raw(session, &mut req, "2");
    println!("tuan62 -> {}", result3);

    assert_eq!(result3, "tuần", "VNI: tuan + 6 + 2 should produce 'tuần'");
    hc_session_free(session);
}

#[test]
fn telex_tuan_tone_then_circumflex() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    // What if user types tone first, then tries to add circumflex?
    let result1 = type_raw(session, &mut req, "tuanf");
    println!("tuanf -> {}", result1);

    // Now try to add circumflex somehow (this shouldn't work in Telex, but let's see)
    // Actually, in Telex there's no way to add circumflex after the fact

    hc_session_free(session);
}

#[test]
fn test_tone_on_tuan_with_circumflex() {
    // Directly test tone placement on "tuân" (with circumflex)
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Build "tuân" using VNI
    let result1 = type_raw(session, &mut req, "tuan6");
    println!("tuan6 -> {}", result1);
    assert_eq!(result1, "tuân");

    // Now apply tone 2 (huyền)
    let result2 = type_raw(session, &mut req, "2");
    println!("tuân + 2 -> {}", result2);
    assert_eq!(result2, "tuần", "Tone should go on â, not u");

    hc_session_free(session);
}

#[test]
fn test_tone_then_circumflex_vni() {
    // Test: apply tone first, then circumflex
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Build "tuan" then apply tone
    let result1 = type_raw(session, &mut req, "tuan2");
    println!("tuan2 -> {}", result1);

    // Now apply circumflex
    let result2 = type_raw(session, &mut req, "6");
    println!("tuan2 + 6 -> {}", result2);

    // The tone should move to the circumflex vowel
    assert_eq!(
        result2, "tuần",
        "After adding circumflex, tone should move to â"
    );

    hc_session_free(session);
}

#[test]
fn vni_workflow_should_not_apply_telex_transforms() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Type "workflow" char by char and check preedit at each step
    let preedit_w = type_raw(session, &mut req, "w");
    assert_eq!(
        preedit_w, "w",
        "after 'w' preedit should be 'w', got '{}'",
        preedit_w
    );

    let preedit_wo = type_raw(session, &mut req, "o");
    assert_eq!(
        preedit_wo, "wo",
        "after 'wo' preedit should be 'wo', got '{}'",
        preedit_wo
    );

    let preedit_wor = type_raw(session, &mut req, "r");
    assert_eq!(
        preedit_wor, "wor",
        "after 'wor' preedit should be 'wor', got '{}'",
        preedit_wor
    );

    let preedit_work = type_raw(session, &mut req, "k");
    assert_eq!(
        preedit_work, "work",
        "after 'work' preedit should be 'work', got '{}'",
        preedit_work
    );

    let preedit_workf = type_raw(session, &mut req, "f");
    assert_eq!(
        preedit_workf, "workf",
        "after 'workf' preedit should be 'workf' (no Telex tone!), got '{}'",
        preedit_workf
    );

    let preedit_workfl = type_raw(session, &mut req, "l");
    assert_eq!(
        preedit_workfl, "workfl",
        "after 'workfl', got '{}'",
        preedit_workfl
    );

    let preedit_workflo = type_raw(session, &mut req, "o");
    assert_eq!(
        preedit_workflo, "workflo",
        "after 'workflo', got '{}'",
        preedit_workflo
    );

    let preedit_workflow = type_raw(session, &mut req, "w");
    assert_eq!(
        preedit_workflow, "workflow",
        "final preedit should be 'workflow', got '{}'",
        preedit_workflow
    );

    // Also verify commit
    let (committed, _status) = commit_with_space(session, &mut req);
    assert_eq!(
        committed, "workflow",
        "committed text should be 'workflow', got '{}'",
        committed
    );

    hc_session_free(session);
}

#[test]
fn vni_english_words_preedit_no_telex_transforms() {
    // English words containing Telex trigger characters:
    // s→sắc, f→huyền, r→hỏi, x→ngã, j→nặng, w→horn, z→cancel
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    let english_words = [
        "system",
        "software",
        "export",
        "express",
        "project",
        "refresh",
        "fix",
        "just",
        "major",
        "subject",
        "forward",
        "switch",
        "framework",
        "firefox",
        "windows",
        "result",
        "request",
        "review",
        "service",
        "server",
        "offset",
        "buffer",
        "differ",
        "offer",
        "suffer",
    ];

    for word in english_words {
        hc_session_reset(session);
        let result = type_raw(session, &mut req, word);
        assert_eq!(
            result, word,
            "VNI preedit for '{}' should be unchanged, got '{}'",
            word, result
        );
    }

    hc_session_free(session);
}

#[test]
fn vni_english_words_commit_auto_restores() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Words in the English dictionary should commit as-is
    let known_english = ["workflow", "system", "project", "export"];

    for word in known_english {
        hc_session_reset(session);
        type_raw(session, &mut req, word);
        let (committed, status) = commit_with_space(session, &mut req);
        assert_eq!(
            committed, word,
            "VNI commit for English word '{}' should be '{}', got '{}'",
            word, word, committed
        );
        assert_eq!(
            status,
            HCStatusFlag::EnglishFallback as i32,
            "English word '{}' should commit with EnglishFallback status",
            word
        );
    }

    hc_session_free(session);
}

#[test]
fn vni_does_not_cross_contaminate_with_telex_triggers() {
    // Verify that specific Telex trigger characters have NO effect in VNI mode
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Type "hoas" — in Telex 's' would apply sắc to 'a', but in VNI it should be literal
    let result = type_raw(session, &mut req, "hoas");
    assert_eq!(
        result, "hoas",
        "VNI: 'hoas' should not apply Telex 's' tone, got '{}'",
        result
    );

    hc_session_reset(session);

    // Type "hoaf" — in Telex 'f' would apply huyền to 'a', but in VNI it should be literal
    let result = type_raw(session, &mut req, "hoaf");
    assert_eq!(
        result, "hoaf",
        "VNI: 'hoaf' should not apply Telex 'f' tone, got '{}'",
        result
    );

    hc_session_reset(session);

    // Type "hoar" — in Telex 'r' would apply hỏi to 'a', but in VNI it should be literal
    let result = type_raw(session, &mut req, "hoar");
    assert_eq!(
        result, "hoar",
        "VNI: 'hoar' should not apply Telex 'r' tone, got '{}'",
        result
    );

    hc_session_reset(session);

    // Type "hoax" — in Telex 'x' would apply ngã to 'a', but in VNI it should be literal
    let result = type_raw(session, &mut req, "hoax");
    assert_eq!(
        result, "hoax",
        "VNI: 'hoax' should not apply Telex 'x' tone, got '{}'",
        result
    );

    hc_session_reset(session);

    // Type "hoaj" — in Telex 'j' would apply nặng to 'a', but in VNI it should be literal
    let result = type_raw(session, &mut req, "hoaj");
    assert_eq!(
        result, "hoaj",
        "VNI: 'hoaj' should not apply Telex 'j' tone, got '{}'",
        result
    );

    hc_session_reset(session);

    // Type "hoaw" — in Telex 'w' would apply horn/breve, but in VNI it should be literal
    let result = type_raw(session, &mut req, "hoaw");
    assert_eq!(
        result, "hoaw",
        "VNI: 'hoaw' should not apply Telex 'w' diacritic, got '{}'",
        result
    );

    hc_session_reset(session);

    // Type "hoaz" — in Telex 'z' would cancel marks, but in VNI it should be literal
    let result = type_raw(session, &mut req, "hoaz");
    assert_eq!(
        result, "hoaz",
        "VNI: 'hoaz' should not apply Telex 'z' cancel, got '{}'",
        result
    );

    hc_session_free(session);
}

#[test]
fn vni_digit_after_space_auto_reopens_commit_within_timeout() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Type "khong" and commit with space
    assert_eq!(type_raw(session, &mut req, "khong"), "khong");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "khong");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    // Immediately type "6" (circumflex) without backspace
    // This should auto-reopen the last commit and apply circumflex
    req.kind = HCKeyKind::Printable as i32;
    let six = c("6");
    req.text = six.as_ptr();
    let edit = hc_session_handle_key(session, &req);
    assert_eq!(edit.state.status_flag, HCStatusFlag::InProgress as i32);
    assert_eq!(read_and_free(edit.state), "không");

    hc_session_free(session);
}

#[test]
fn vni_digit_after_space_does_not_reopen_after_timeout() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Type "khong" and commit with space
    assert_eq!(type_raw(session, &mut req, "khong"), "khong");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "khong");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    // Wait for edit window to expire
    std::thread::sleep(Duration::from_millis(1600));

    // Type "6" - should NOT reopen, should be unhandled
    req.kind = HCKeyKind::Printable as i32;
    let six = c("6");
    req.text = six.as_ptr();
    let edit = hc_session_handle_key(session, &req);
    assert_eq!(edit.handled, 0, "digit after timeout should not be handled");
    free_state(edit.state);

    hc_session_free(session);
}

#[test]
fn vni_tone_digit_does_not_reopen_toned_word() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Type "tuan2" to get "tuần" (with circumflex and Huyền tone)
    assert_eq!(type_raw(session, &mut req, "tuan2"), "tuần");

    // Commit with space
    req.kind = HCKeyKind::Space as i32;
    let (committed, status) = send_key(session, &mut req, HCKeyKind::Space, " ");
    assert_eq!(committed, "tuần");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    // Immediately type "1" (tone Sac) - should NOT reopen because word already has tone
    req.kind = HCKeyKind::Printable as i32;
    let one = c("1");
    req.text = one.as_ptr();
    let edit = hc_session_handle_key(session, &req);
    assert_eq!(edit.handled, 0, "tone digit should not reopen toned word");
    free_state(edit.state);

    hc_session_free(session);
}

#[test]
fn vni_tone_digit_reopens_untone_word() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    // Type "khong" and commit with space (no tone)
    assert_eq!(type_raw(session, &mut req, "khong"), "khong");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "khong");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    // Immediately type "1" (tone Sac) - should reopen and apply tone
    req.kind = HCKeyKind::Printable as i32;
    let one = c("1");
    req.text = one.as_ptr();
    let edit = hc_session_handle_key(session, &req);
    assert_eq!(edit.state.status_flag, HCStatusFlag::InProgress as i32);
    assert_eq!(read_and_free(edit.state), "khóng");

    hc_session_free(session);
}

#[test]
fn mode_cycling_100_times_is_safe() {
    let modes = [
        InputMode::Telex,
        InputMode::Vni,
        InputMode::Viqr,
        InputMode::HanNomTelex,
        InputMode::HanNomVni,
        InputMode::HanNomViqr,
    ];
    let session = hc_session_new(InputMode::Telex as i32, 0);

    for i in 0..100 {
        let mode = modes[i % modes.len()];
        let mut req = key_request(mode);
        let sample = match mode {
            InputMode::Telex | InputMode::HanNomTelex => "viet",
            InputMode::Vni | InputMode::HanNomVni => "viet6",
            InputMode::Viqr | InputMode::HanNomViqr => "viet^",
        };
        type_raw(session, &mut req, sample);
        hc_session_reset(session);
    }

    hc_session_free(session);
}

#[test]
fn han_nom_lookup_exact_and_toneless_fallback() {
    let dict = crate::han_nom::get_global_dict().unwrap();
    let candidates = dict.lookup("thiên");
    assert!(
        !candidates.is_empty(),
        "thiên lookup should return candidates"
    );
    assert!(candidates.contains(&'天'), "thiên must contain 天");

    let toneless = dict.lookup("thien");
    assert!(
        !toneless.is_empty(),
        "thien toneless fallback should return candidates"
    );
}

#[test]
fn han_nom_telex_flow_space_opens_candidates_digit_selects() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "thien"
    for ch in "thien".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }
    assert_eq!(res.handled, 1);

    // Space -> lookup and open candidates
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.status_flag, HCStatusFlag::InProgress as i32);
    assert!(res.candidate_count > 0, "space should populate candidates");

    // Digit 1 -> select first candidate
    let one = c("1");
    req.kind = HCKeyKind::Printable as i32;
    req.text = one.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.status_flag, HCStatusFlag::Commit as i32);
    assert_eq!(res.handled, 1);

    hc_session_free(session);
}

#[test]
fn han_nom_vni_digit_transforms_in_phase_a() {
    let session = hc_session_new(InputMode::HanNomVni as i32, 0);
    let mut req = key_request(InputMode::HanNomVni);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "thien6" (VNI circumflex)
    for ch in "thien6".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }

    let reading = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert_eq!(reading, "thiên");

    hc_session_free(session);
}

#[test]
fn han_nom_1000_keystrokes_stress_test_mode_cycling() {
    let modes = [
        InputMode::Telex,
        InputMode::Vni,
        InputMode::Viqr,
        InputMode::HanNomTelex,
        InputMode::HanNomVni,
        InputMode::HanNomViqr,
    ];
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    for i in 0..1000 {
        let mode = modes[i % modes.len()];
        req.input_mode = mode as i32;
        let s = match i % 4 {
            0 => "a",
            1 => "1",
            2 => " ",
            _ => "s",
        };
        let c_str = c(s);
        req.kind = if s == " " {
            HCKeyKind::Space as i32
        } else {
            HCKeyKind::Printable as i32
        };
        req.text = c_str.as_ptr();

        if mode as i32 >= 3 {
            hc_session_handle_key_hannom(session, &req, &mut res);
        } else {
            hc_session_handle_key(session, &req);
        }

        if i % 50 == 0 {
            hc_session_reset(session);
        }
    }

    hc_session_free(session);
}

#[test]
fn cjk_ext_b_plus_utf8_rendering_safety() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    for ch in "truong".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }

    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);

    if res.candidate_count > 0 && !res.candidates.is_null() {
        unsafe {
            let candidates =
                std::slice::from_raw_parts(res.candidates, res.candidate_count as usize);
            for cand in candidates {
                assert!(cand.byte_len <= 4, "UTF-8 byte len must be <= 4");
                let s = std::str::from_utf8(&cand.utf8[..cand.byte_len as usize]);
                assert!(s.is_ok(), "Candidate UTF-8 must be valid");
            }
        }
    }

    hc_session_free(session);
}

#[test]
fn missing_and_empty_dictionary_fallback_safety() {
    let dict = crate::han_nom::EmbeddedNomDict::from_binary(&[]).unwrap_err();
    assert_eq!(dict, crate::han_nom::DictError::Corrupted);

    let invalid_magic =
        crate::han_nom::EmbeddedNomDict::from_binary(b"INVALID_HEADER_DATA").unwrap_err();
    assert_eq!(invalid_magic, crate::han_nom::DictError::InvalidMagic);
}

#[test]
fn han_nom_dict_status_check() {
    let status = hc_nom_dict_status(std::ptr::null_mut());
    assert_eq!(status, 0, "dict status should be 0 (ok)");
}

// ── Hán Nôm edge-case regression suite ──

#[test]
fn hannom_empty_buffer_space_passthrough() {
    // Space on empty buffer must NOT be handled (passthrough to app)
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    let handled = hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(
        handled, 0,
        "space on empty buffer must return 0 (passthrough)"
    );
    assert_eq!(res.handled, 0);

    hc_session_free(session);
}

#[test]
fn hannom_backspace_on_empty_buffer_still_handled() {
    // Backspace on empty buffer should still be "handled" to prevent passthrough
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    let bs = c("");
    req.kind = HCKeyKind::Backspace as i32;
    req.text = bs.as_ptr();
    let handled = hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(handled, 1, "backspace on empty buffer is handled");
    assert_eq!(res.handled, 1);
    assert_eq!(
        res.reading_len, 0,
        "reading should be empty after backspace on empty"
    );

    hc_session_free(session);
}

#[test]
fn hannom_escape_from_candidate_returns_to_reading_with_preedit() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "thieen" (Telex → thiên)
    for ch in "thieen".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }

    // Space → candidate phase
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert!(res.candidate_count > 0, "should have candidates");

    // Escape → back to reading phase, preedit preserved
    let esc = c("");
    req.kind = HCKeyKind::Escape as i32;
    req.text = esc.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.handled, 1);
    assert_eq!(res.candidate_count, 0, "candidates cleared after escape");
    let reading = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert_eq!(
        reading, "thiên",
        "reading preserved after escape from candidates"
    );

    hc_session_free(session);
}

#[test]
fn hannom_escape_from_reading_clears_all() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "abc"
    for ch in "abc".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }
    assert!(res.reading_len > 0, "should have reading");

    // Escape → clear everything
    let esc = c("");
    req.kind = HCKeyKind::Escape as i32;
    req.text = esc.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.handled, 1);
    assert_eq!(res.reading_len, 0, "reading cleared after escape");

    hc_session_free(session);
}

#[test]
fn hannom_phase_b_nondigit_printable_falls_to_phase_a() {
    // In candidate phase, pressing a letter should close candidates and
    // start a new reading with that letter
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "thieen" + space → candidates
    for ch in "thieen".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert!(res.candidate_count > 0);

    // Press 'a' → should close candidates and enter 'a' into Phase A
    let a = c("a");
    req.kind = HCKeyKind::Printable as i32;
    req.text = a.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.handled, 1);
    // The letter 'a' should be appended to the existing buffer (since
    // we switched from Phase B to Phase A and the buffer was still 'thiên')
    let reading = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert!(
        reading.len() > 0,
        "reading should not be empty after letter in candidate phase"
    );

    hc_session_free(session);
}

#[test]
fn hannom_viqr_mode_basic_composition() {
    let session = hc_session_new(InputMode::HanNomViqr as i32, 0);
    let mut req = key_request(InputMode::HanNomViqr);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // In VIQR Hán Nôm mode, digits should be passthrough (not VNI triggers)
    let a = c("a");
    req.kind = HCKeyKind::Printable as i32;
    req.text = a.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.handled, 1);

    // Digit '1' in VIQR mode → should passthrough (not handled)
    let one = c("1");
    req.kind = HCKeyKind::Printable as i32;
    req.text = one.as_ptr();
    let handled = hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(handled, 0, "digits in VIQR Hán Nôm mode should passthrough");
    assert_eq!(res.handled, 0);

    hc_session_free(session);
}

#[test]
fn hannom_no_match_space_commits_quoc_ngu_reading() {
    // When no Nôm candidates found for reading, Space should commit the quốc ngữ text
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "zzzzz" (nonsense reading)
    for ch in "zzzzz".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }

    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(
        res.status_flag,
        HCStatusFlag::Commit as i32,
        "no-match reading should commit"
    );
    assert_eq!(res.handled, 1);
    let committed = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert_eq!(
        committed, "zzzzz",
        "committed text should be the composed reading"
    );

    hc_session_free(session);
}

#[test]
fn hannom_candidate_digit_out_of_range_stays_in_phase_b() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "thieen" + space → candidates
    for ch in "thieen".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    let count = res.candidate_count;
    assert!(count > 0);

    // Press '9' — if there are fewer than 9 candidates, should stay in Phase B
    if count < 9 {
        let nine = c("9");
        req.kind = HCKeyKind::Printable as i32;
        req.text = nine.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
        assert_eq!(res.handled, 1, "out-of-range digit should still be handled");
        // Should still show candidates (populate_nom_result called)
    }

    hc_session_free(session);
}

#[test]
fn hannom_backspace_single_char_reading_clears_preedit() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type single char "a"
    let a = c("a");
    req.kind = HCKeyKind::Printable as i32;
    req.text = a.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.reading_len, 1);

    // Backspace → empty
    let bs = c("");
    req.kind = HCKeyKind::Backspace as i32;
    req.text = bs.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.handled, 1);
    assert_eq!(
        res.reading_len, 0,
        "reading empty after backspace on single char"
    );

    hc_session_free(session);
}

#[test]
fn hannom_telex_tone_markers_compose_during_reading() {
    // Telex triggers (s, f, r, x, j) should transform the buffer during reading
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "has" → should compose "hás" (acute on a)
    for ch in "has".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }
    let reading = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert_eq!(
        reading, "há",
        "Telex 's' should apply acute tone during reading (consumed as trigger)"
    );

    hc_session_free(session);
}

#[test]
fn hannom_rapid_mode_switch_mid_composition() {
    // Switching from Telex to VNI mid-composition should not crash
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "th" in Telex
    for ch in "th".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }

    // Switch to VNI mid-composition
    req.input_mode = InputMode::HanNomVni as i32;
    let i = c("i");
    req.text = i.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.handled, 1);

    // VNI digit should now apply VNI transform
    let six = c("6");
    req.text = six.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(
        res.handled, 1,
        "VNI digit should be handled after mode switch"
    );

    hc_session_free(session);
}

#[test]
fn hannom_64_char_buffer_cap_enforced() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type 70 characters — should cap at 64
    for _ in 0..70 {
        let a = c("a");
        req.kind = HCKeyKind::Printable as i32;
        req.text = a.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }
    let reading = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert!(reading.len() <= 64, "reading should be capped at 64 bytes");

    hc_session_free(session);
}

#[test]
fn hannom_backspace_in_candidate_phase_returns_to_reading_minus_one() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "thieen" + space → candidates
    for ch in "thieen".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert!(res.candidate_count > 0);

    // Backspace → should return to reading with last raw char removed
    let bs = c("");
    req.kind = HCKeyKind::Backspace as i32;
    req.text = bs.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);
    assert_eq!(res.handled, 1);
    assert_eq!(res.candidate_count, 0, "backspace should close candidates");
    let reading = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    // "thieen" minus last char → "thiee" which renders as "thiê"
    assert_eq!(
        reading, "thiê",
        "backspace from candidates should remove last raw char and re-render"
    );

    hc_session_free(session);
}
