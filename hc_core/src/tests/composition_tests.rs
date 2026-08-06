use crate::language;
use crate::test_helpers::*;
use crate::*;
use std::ptr;

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

    // "iê/yê" is NOT auto-completed to "iêu/yêu" — the u must be typed. The tone
    // still lands on ê. (Auto-u corrupted việt/viên/yên-class words: P2-5.)
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
fn qu_glide_keeps_plain_a_with_coda_p0_1() {
    // P0-1: "qua<coda>" + tone must stay plain a (quán/quạt/quát), not become
    // â (quấn/quật/quất). The bug lived in apply_vietnamese_normalization, which
    // rewrote a→â after u without excluding the qu- glide.
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    assert_eq!(type_raw(session, &mut req, "quats"), "quát"); // to shout
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "quatj"), "quạt"); // electric fan
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "quans"), "quán"); // shop
    hc_session_reset(session);
    // Controls: non-qu "uan" still becomes "uân" (tuần), and explicit "quaan"
    // (aa → â) still yields the quân family (quấn). The fix must not touch these.
    assert_eq!(type_raw(session, &mut req, "tuanf"), "tuần");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "quaans"), "quấn");
    hc_session_free(session);

    // The same normalization is shared by VNI and VIQR.
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    assert_eq!(type_raw(session, &mut req, "quan1"), "quán");
    hc_session_free(session);

    let session = hc_session_new(InputMode::Viqr as i32, 0);
    let mut req = key_request(InputMode::Viqr);
    assert_eq!(type_raw(session, &mut req, "quan'"), "quán");
    hc_session_free(session);
}

#[test]
fn repeated_tone_key_cancels_tone_p2_8() {
    // P2-8: pressing a tone key twice removes the tone and emits the letter
    // literally (standard Telex). Previously produced "cós"/"ás"/"toáns".
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    assert_eq!(type_raw(session, &mut req, "coss"), "cos");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "ass"), "as");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "toanss"), "toans");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "vangss"), "vangs"); // closed syllable
    hc_session_reset(session);
    // Controls: a single tone still applies; a different tone still replaces.
    assert_eq!(type_raw(session, &mut req, "cos"), "có");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "cosf"), "cò");
    hc_session_free(session);
}

#[test]
fn retone_closed_syllable_replaces_p1_3() {
    // P1-3: re-toning a syllable that ends in a 2-consonant coda must replace
    // the tone, not swallow the key. Previously the second tone key was eaten.
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    assert_eq!(type_raw(session, &mut req, "vangsf"), "vàng"); // váng -> vàng
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "manhsf"), "mành"); // mánh -> mành
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "toongsf"), "tồng"); // tống -> tồng
    hc_session_free(session);
}

#[test]
fn tone_reedit_hardening_across_modes() {
    // QC hardening: uppercase re-tone, cancel on a non-first vowel, and
    // re-tone / cancel across VNI and VIQR — all exercise the strip-then-apply
    // path added for P1-3 / P2-8.
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    assert_eq!(type_raw(session, &mut req, "VANGSF"), "VÀNG"); // uppercase retone
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "cuoongsf"), "cuồng"); // ô keeps its circumflex
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "tieengss"), "tiêngs"); // cancel on ê (2nd vowel)
    hc_session_free(session);

    // VNI: a different digit replaces the tone; the same digit twice cancels it.
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    assert_eq!(type_raw(session, &mut req, "quan12"), "quàn"); // sắc -> huyền
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "vang11"), "vang"); // cancel (digit consumed)
    hc_session_free(session);

    // VIQR: a different mark replaces; the same mark twice cancels + emits literal.
    let session = hc_session_new(InputMode::Viqr as i32, 0);
    let mut req = key_request(InputMode::Viqr);
    assert_eq!(type_raw(session, &mut req, "vang'?"), "vảng"); // sắc -> hỏi
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "vang''"), "vang'"); // cancel + literal mark
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

    // Second backspace deletes 'n'. Losing the coda re-opens the syllable, and
    // an open nucleus is spelled "uơ" — "phươ" is not a Vietnamese spelling
    // (VI-05), so the horn leaves the u. Still exactly one visible character.
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phuơ");

    // Third backspace deletes 'ơ'
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phu");

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

    // "phưở" would be an open "ươ", which Vietnamese does not spell; see the
    // "phuơ" step above.
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phuở");

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phu");

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
    assert_eq!(read_and_free(back.state), "phuở");

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(read_and_free(back.state), "phu");

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

    // An OPEN "uy" is the one cluster where the two Vietnamese conventions
    // disagree, so it follows `legacy_tone`. This session is the default
    // (legacy_tone = 0 = "kiểu mới"), which tones the last vowel — the same
    // convention that already produced "hoà" and "khoẻ" here. The old style
    // ("túy", "thùy", "tủy") is asserted by `legacy_tone_uses_old_style`.
    assert_eq!(type_raw(session, &mut req, "tuys"), "tuý");
    hc_session_reset(session);

    assert_eq!(type_raw(session, &mut req, "thuyf"), "thuỳ");
    hc_session_reset(session);

    // A CLOSED "uy" is spelled the same in both conventions: tone on the y.
    assert_eq!(type_raw(session, &mut req, "huynhf"), "huỳnh");
    hc_session_reset(session);

    assert_eq!(type_raw(session, &mut req, "tuyr"), "tuỷ");

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
    // s then f on a closed syllable: the later tone (huyền) must replace the
    // earlier (sắc) on ô. Previously asserted "tống" — that locked in the P1-3
    // bug where the second tone key was swallowed on 2-consonant codas.
    let result = type_raw(session, &mut req, "toongsf");
    assert_eq!(result, "tồng", "tồng: huyền replaces sắc on ô with a coda");
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
    assert_eq!(result, "yề", "yề: tone on ê, no auto-u (P2-5)");
    hc_session_free(session);
}

#[test]
fn tone_edge_case_ieu_circumflex() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    let result = type_raw(session, &mut req, "ieef");
    assert_eq!(result, "iề", "iề: tone on ê, no auto-u (P2-5)");
    hc_session_free(session);
}

#[test]
fn ie_ye_no_spurious_u_p2_5() {
    // P2-5: applying a tone to a syllable ending in "iê"/"yê" must NOT append a
    // stray u. "viê" + nặng is "việ" (awaiting a coda), not "việu".
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    assert_eq!(type_raw(session, &mut req, "vieej"), "việ");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "vieejt"), "việt");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "Vieej"), "Việ"); // uppercase preserved
    hc_session_reset(session);
    // Genuine iêu/yêu words still compose when the u IS typed.
    assert_eq!(type_raw(session, &mut req, "yeeus"), "yếu");
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "nhieeuf"), "nhiều");
    // Tone-first, then coda must complete cleanly (guards against re-introducing
    // any iê/yê auto-completion).
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "bieejn"), "biện");
    hc_session_free(session);

    // The normalization is shared, so VNI and VIQR must not append a u either.
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    assert_eq!(type_raw(session, &mut req, "vie65"), "việ");
    hc_session_free(session);

    let session = hc_session_new(InputMode::Viqr as i32, 0);
    let mut req = key_request(InputMode::Viqr);
    assert_eq!(type_raw(session, &mut req, "vie^."), "việ");
    hc_session_free(session);
}

#[test]
fn dd_triple_tap_cancels_dstroke() {
    // ddd → dd: a 3rd d reverts đ→d and emits the key literally, mirroring the
    // circumflex toggle (aaa → aa). Was "đd" (đ-cancel branch was unreachable).
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    assert_eq!(type_raw(session, &mut req, "dd"), "đ"); // 2 taps still make đ
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "ddd"), "dd"); // 3rd tap cancels
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "DDD"), "DD"); // uppercase preserved
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "ddi"), "đi"); // normal đ word unaffected
    hc_session_reset(session);
    assert_eq!(type_raw(session, &mut req, "aaa"), "aa"); // vowel toggle unchanged
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
    assert_eq!(
        result, "uý",
        "uý: an open uy follows the configured convention, and legacy_tone=0 is \
         the new style (last vowel) — the same one that spells hoà and khoẻ"
    );
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
    assert_eq!(result, "tồng", "tồng: huyen tone should go on ô with coda");
    hc_session_free(session);
}

#[test]
fn vni_edge_case_uoi_horn() {
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);
    let result = type_raw(session, &mut req, "tuo7i1");
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
    assert_eq!(
        result, "uý",
        "uý: an open uy follows the configured convention, and legacy_tone=0 is \
         the new style (last vowel) — the same one that spells hoà and khoẻ"
    );
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
    let session = hc_session_new(InputMode::Vni as i32, 0);
    let mut req = key_request(InputMode::Vni);

    let result1 = type_raw(session, &mut req, "tuan2");
    assert_eq!(result1, "tuần", "tuan2 should produce tuần");

    let result2 = type_raw(session, &mut req, "6");
    assert_eq!(
        result2, "tuàn6",
        "circumflex toggle on tuần should strip â to a and emit digit 6"
    );

    let result3 = type_raw(session, &mut req, "6");
    assert_eq!(
        result3, "tuần6",
        "circumflex toggle on tuàn should re-add â and emit digit 6"
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

// ---------------------------------------------------------------------------
// P2 regressions — QC_FINDINGS.md VI-03, VI-04/VI-06, VI-05, FFI-07, PERF-02
// ---------------------------------------------------------------------------

/// VI-03: `englishProtection` used to tint the preedit and nothing else, so
/// `craws` committed as `crắ` at Off, Soft *and* Hard even though `cr` is one
/// of the starts `is_hard_english_raw_start` explicitly blocks.
#[test]
fn english_protection_gates_the_commit_not_just_the_preedit() {
    // Off is the default and must stay byte-identical: the language scores
    // alone decide, and "craws" scores Vietnamese.
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.english_protection = 0;
    for (word, expected) in [("craws", "crắ"), ("swims", "swím"), ("yates", "yaté")] {
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, word), expected);
        let (committed, status) = commit_with_space(session, &mut req);
        assert_eq!(committed, expected, "Off must not change today's behaviour");
        assert_eq!(status, HCStatusFlag::Commit as i32);
    }
    hc_session_free(session);

    // Hard restores the raw keystrokes for an impossible Vietnamese onset.
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.english_protection = 2;
    for word in ["craws", "swims"] {
        hc_session_reset(session);
        type_raw(session, &mut req, word);
        let (committed, status) = commit_with_space(session, &mut req);
        assert_eq!(committed, word, "Hard protection must commit '{word}' raw");
        assert_eq!(status, HCStatusFlag::EnglishFallback as i32);
    }
    hc_session_free(session);

    // Soft only covers the y+vowel pattern, so "craws" still composes there.
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);
    req.english_protection = 1;
    type_raw(session, &mut req, "craws");
    let (committed, _) = commit_with_space(session, &mut req);
    assert_eq!(committed, "crắ", "Soft does not claim consonant clusters");

    hc_session_reset(session);
    type_raw(session, &mut req, "yates");
    let (committed, status) = commit_with_space(session, &mut req);
    assert_eq!(committed, "yates", "Soft protection covers y + vowel");
    assert_eq!(status, HCStatusFlag::EnglishFallback as i32);
    hc_session_free(session);
}

/// VI-04: `legacy_tone` short-circuited to the first vowel before any cluster
/// analysis, so closed syllables came out as `hòang`/`tóan`/`ngọai`/`xóay` —
/// misspellings in *both* Vietnamese conventions. Old style differs from new
/// style only for an OPEN `oa`/`oe`/`uy`.
#[test]
fn legacy_tone_uses_old_style_without_misspelling_closed_syllables() {
    let session = hc_session_new(InputMode::Telex as i32, 1);
    let mut req = key_request(InputMode::Telex);
    req.legacy_tone = 1;

    // Closed syllables: identical in both conventions.
    for (keys, expected) in [
        ("hoangf", "hoàng"),
        ("toans", "toán"),
        ("ngoaij", "ngoại"),
        ("xoays", "xoáy"),
        ("huynhf", "huỳnh"),
        ("quangr", "quảng"),
    ] {
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, keys), expected, "{keys}");
    }

    // Open oa/oe/uy: this is what "kiểu cũ" actually means.
    for (keys, expected) in [
        ("hoaf", "hòa"),
        ("khoer", "khỏe"),
        ("thuyr", "thủy"),
        ("tuys", "túy"),
    ] {
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, keys), expected, "{keys}");
    }

    hc_session_free(session);
}

/// VI-06: the default (`legacy_tone = 0`) applied the new style to `oa`/`oe`
/// but the old style to `uy`, producing the `hoà` + `thủy` mix that no
/// mainstream IME emits. One convention has to cover all three.
#[test]
fn default_tone_style_is_consistent_across_oa_oe_and_uy() {
    let session = hc_session_new(InputMode::Telex as i32, 0);
    let mut req = key_request(InputMode::Telex);

    for (keys, expected) in [
        ("hoaf", "hoà"),
        ("khoer", "khoẻ"),
        ("thuyr", "thuỷ"),
        ("tuys", "tuý"),
    ] {
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, keys), expected, "{keys}");
    }

    // Closed syllables are convention-independent and must not move.
    for (keys, expected) in [("hoangf", "hoàng"), ("huynhf", "huỳnh")] {
        hc_session_reset(session);
        assert_eq!(type_raw(session, &mut req, keys), expected, "{keys}");
    }

    hc_session_free(session);
}

/// VI-05: the horn was applied to every plain u and o in the buffer, with no
/// `qu-` exclusion — `quowr` → `qưở` (`qư` is not a legal sequence), `ruouwj`
/// → `rượư`, `uouws` → `ướư`, and the VNI toggle path spelled `hu7o7u7` as
/// `hươư`.
#[test]
fn horn_applies_to_the_nucleus_only() {
    let cases: [(InputMode, &[(&str, &str)]); 3] = [
        (
            InputMode::Telex,
            &[
                ("quowr", "quở"),
                ("thuowr", "thuở"),
                ("huow", "huơ"),
                ("ruouwj", "rượu"),
                ("uouws", "ướu"),
                // The deliberate both-vowel horn must survive.
                ("nguoiw", "ngươi"),
                ("nguwowif", "người"),
                ("thuongw", "thương"),
                ("thuowng", "thương"),
            ],
        ),
        (
            InputMode::Vni,
            &[
                ("quo73", "quở"),
                ("thuo73", "thuở"),
                ("huo7", "huơ"),
                ("uou71", "ướu"),
                ("nguoi7", "ngươi"),
                ("thu7o7ng", "thương"),
                // The P0 toggle fix surfaced this bug as "hươư".
                ("hu7o7u7", "hươu"),
            ],
        ),
        (
            InputMode::Viqr,
            &[
                ("quo+?", "quở"),
                ("thuo+?", "thuở"),
                ("huo+", "huơ"),
                ("ruou+.", "rượu"),
                ("uou+'", "ướu"),
                ("nguoi+", "ngươi"),
            ],
        ),
    ];

    for (mode, expectations) in cases {
        let session = hc_session_new(mode as i32, 0);
        let mut req = key_request(mode);
        for (keys, expected) in expectations {
            hc_session_reset(session);
            assert_eq!(
                &type_raw(session, &mut req, keys),
                expected,
                "{mode:?}: {keys}"
            );
        }
        hc_session_free(session);
    }
}

/// FFI-07: `committed_raw_history` grew by the raw keystrokes of every commit,
/// was never read anywhere in the crate, was not cleared by `reset()` and had
/// no bound — ~31 B per commit, +30 MB per million.
///
/// The property is scale-free: two identical batches of commits must retain
/// the same amount of state, so the second batch may not make the engine any
/// bigger than the first did.
#[test]
fn committing_retains_no_per_commit_state() {
    fn commit_batch(engine: &mut composition::CompositionEngine, count: usize) {
        for _ in 0..count {
            engine.raw_buffer.push_str("tieengs");
            engine.render_from_raw();
            let mut state = engine.commit_current();
            hc_state_free(&mut state);
        }
    }

    let mut engine = composition::CompositionEngine::new(InputMode::Telex, false);
    commit_batch(&mut engine, 100);
    let after_first_batch = format!("{engine:?}").len();
    commit_batch(&mut engine, 900);
    let after_second_batch = format!("{engine:?}").len();

    assert!(
        after_second_batch <= after_first_batch + 64,
        "900 further commits grew the engine from {after_first_batch} to \
         {after_second_batch} bytes of retained state",
    );
}

/// PERF-02: `is_known_english_word` used to parse `/usr/share/dict/words`
/// (2.49 MB, 235,976 lines) inline, so keystroke #1 cost 30–44 ms against
/// ~10 µs for keystroke #2 — a violation of AGENTS.md invariant 3. And even
/// once cached, every lookup rebuilt the search-path list.
///
/// Both are asserted as properties rather than wall-clock numbers: the parse
/// must not happen on the calling thread, and a warm lookup must not rebuild
/// the paths.
#[test]
fn dictionary_lookups_never_read_files_on_the_typing_path() {
    // Touch the lookup path the way a keystroke does, then let the background
    // load land and ask which thread actually did the parsing.
    assert!(!language::is_known_english_word("zzzz-not-a-word"));
    let loader = language::english_dictionary_load_thread();
    assert_ne!(
        loader,
        std::thread::current().id(),
        "the OS word list must not be parsed on the thread that types",
    );

    // Warm state: the search paths (a Vec<PathBuf> plus dirs::data_dir()) are
    // built once per dictionary, not once per lookup. The probe words must miss
    // the built-in tables, or the `||` short-circuits before the external
    // dictionary is consulted at all.
    assert!(!language::is_known_english_word("qwertzuiop"));
    assert!(!language::is_dictionary_vietnamese_word("qwertzuiop"));
    let paths_built = language::dictionary_path_queries();
    for _ in 0..1_000 {
        let _ = language::is_known_english_word("qwertzuiop");
        let _ = language::is_dictionary_vietnamese_word("qwertzuiop");
    }
    assert_eq!(
        language::dictionary_path_queries(),
        paths_built,
        "a cached lookup must not rebuild the dictionary search paths",
    );
}

