use crate::test_helpers::*;
use crate::*;
use std::ptr;

fn v2_text(result: &HC_HanNomResultV2) -> String {
    if result.reading.is_null() {
        return String::new();
    }
    unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(
            result.reading,
            result.reading_len as usize,
        ))
        .unwrap()
        .to_owned()
    }
}

fn v3_candidate_text(result: &HC_HanNomResultV3, index: usize) -> String {
    assert!(index < result.candidate_count as usize);
    let candidate = unsafe { &*result.candidates.add(index) };
    unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(
            candidate.text,
            candidate.text_len as usize,
        ))
        .unwrap()
        .to_owned()
    }
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
fn han_nom_telex_live_reading_populates_candidates_before_space() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    for ch in "thieen".chars() {
        let s = ch.to_string();
        let c_str = c(&s);
        req.kind = HCKeyKind::Printable as i32;
        req.text = c_str.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
    }

    let reading = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert_eq!(reading, "thiên");
    assert_eq!(res.status_flag, HCStatusFlag::InProgress as i32);
    assert!(
        res.candidate_count > 0,
        "completed reading should expose live Nôm candidates before Space"
    );
    assert!(
        !res.candidates.is_null(),
        "live candidate pointer should be populated before Space"
    );
    assert!(
        res.total_candidates >= res.candidate_count,
        "total candidate count should include the visible page"
    );

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
    assert!(
        res.candidate_count > 0,
        "live candidates populated for reading"
    );
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

#[test]
fn hannom_enter_in_candidate_phase_commits_raw_preedit() {
    // CJK IME standard: Enter in candidate phase commits raw reading (Quốc Ngữ)
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "thieen" + Space -> candidate phase
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

    // Press Enter -> should commit raw reading "thiên"
    let enter = c("");
    req.kind = HCKeyKind::Enter as i32;
    req.text = enter.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);

    assert_eq!(res.status_flag, HCStatusFlag::Commit as i32);
    assert_eq!(res.handled, 1);
    let committed = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert_eq!(
        committed, "thiên",
        "Enter in candidate phase must commit raw reading"
    );

    hc_session_free(session);
}

#[test]
fn hannom_candidate_pagination_equals_and_dash() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "nam" + Space -> candidate phase
    for ch in "nam".chars() {
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
    assert_eq!(res.page, 0);

    let total = res.total_candidates;
    if total > 9 {
        // Press '=' to advance to page 1
        let eq = c("=");
        req.kind = HCKeyKind::Printable as i32;
        req.text = eq.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
        assert_eq!(res.page, 1, "page should advance to 1 after '='");

        // Press '-' to return to page 0
        let dash = c("-");
        req.kind = HCKeyKind::Printable as i32;
        req.text = dash.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
        assert_eq!(res.page, 0, "page should return to 0 after '-'");
    }

    hc_session_free(session);
}

#[test]
fn hannom_candidate_pagination_brackets() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "nam" + Space -> candidate phase
    for ch in "nam".chars() {
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

    if res.total_candidates > 9 {
        // Press ']' to page down
        let close_b = c("]");
        req.kind = HCKeyKind::Printable as i32;
        req.text = close_b.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
        assert_eq!(res.page, 1, "page should advance to 1 after ']'");

        // Press '[' to page up
        let open_b = c("[");
        req.kind = HCKeyKind::Printable as i32;
        req.text = open_b.as_ptr();
        hc_session_handle_key_hannom(session, &req, &mut res);
        assert_eq!(res.page, 0, "page should return to 0 after '['");
    }

    hc_session_free(session);
}

#[test]
fn hannom_punctuation_autocommits_candidate_plus_punct() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut res: HC_HanNomResult = unsafe { std::mem::zeroed() };

    // Type "thieen" + Space -> candidate phase
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

    // Type period '.' -> should auto-commit candidate #1 + '.'
    let dot = c(".");
    req.kind = HCKeyKind::Printable as i32;
    req.text = dot.as_ptr();
    hc_session_handle_key_hannom(session, &req, &mut res);

    assert_eq!(res.status_flag, HCStatusFlag::Commit as i32);
    assert_eq!(res.handled, 1);
    let committed = std::str::from_utf8(&res.reading[..res.reading_len as usize]).unwrap();
    assert!(
        committed.ends_with('.'),
        "committed output should end with punctuation '.'"
    );
    assert!(
        committed.chars().count() >= 2,
        "committed output should contain candidate + punctuation"
    );

    hc_session_free(session);
}

#[test]
fn hannom_v2_converts_common_two_word_phrase_and_predicts() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV2 = unsafe { std::mem::zeroed() };
    for ch in "thành".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        assert_eq!(
            hc_session_handle_key_hannom_v2(session, &req, &mut result),
            1
        );
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    assert!(
        result.candidate_count > 0,
        "first word exposes phrase predictions"
    );
    for ch in "phố".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    assert!(result.candidate_count > 0);
    assert_eq!(
        hc_session_select_hannom_candidate_v2(session, 0, &mut result),
        1
    );
    assert_eq!(v2_text(&result), "城庯");
    hc_session_free(session);
}

#[test]
fn hannom_v2_second_space_keeps_phrase_candidates_and_enter_commits_top_candidate() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV2 = unsafe { std::mem::zeroed() };
    for ch in "đại".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    for ch in "học".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    req.kind = HCKeyKind::Space as i32;
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    assert_eq!(result.status_flag, HCStatusFlag::InProgress as i32);
    assert!(result.candidate_count > 0);
    let top = unsafe { &*result.candidates };
    let top = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(top.text, top.text_len as usize)).unwrap()
    };
    assert_eq!(top, "大學");
    assert_eq!(v2_text(&result), "đại học");

    req.kind = HCKeyKind::Enter as i32;
    req.text = ptr::null();
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    assert_eq!(result.status_flag, HCStatusFlag::Commit as i32);
    assert_eq!(v2_text(&result), "大學");

    let mut v1: HC_HanNomResult = unsafe { std::mem::zeroed() };
    assert_eq!(
        hc_session_handle_key_hannom(session, &req, &mut v1),
        0,
        "v1 symbol remains callable after v2 commit"
    );
    hc_session_free(session);
}

#[test]
fn hannom_v2_commit_bytes_survive_handler_return_and_options_accept_null_path() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let options = HC_HanNomOptions {
        phrase_prediction: 1,
        learning_enabled: 0,
        history_path: ptr::null(),
    };
    hc_session_set_hannom_options(session, &options);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV2 = unsafe { std::mem::zeroed() };
    for ch in "Hà".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    for ch in "Nội".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    assert_eq!(
        hc_session_select_hannom_candidate_v2(session, 0, &mut result),
        1
    );
    let bytes = unsafe { std::slice::from_raw_parts(result.reading, result.reading_len as usize) };
    assert_eq!(
        std::str::from_utf8(bytes).unwrap(),
        "河内",
        "borrowed UTF-8 remains valid after FFI returns"
    );
    hc_session_free(session);
}

#[test]
fn hannom_v2_generates_bounded_fallback_and_history_recovers_from_corruption() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV2 = unsafe { std::mem::zeroed() };
    for ch in "nam".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    for ch in "nam".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    assert!(result.candidate_count <= 9);
    assert!(result.candidate_count > 0);
    let kinds =
        unsafe { std::slice::from_raw_parts(result.candidates, result.candidate_count as usize) };
    assert!(
        kinds
            .iter()
            .all(|candidate| candidate.kind == 0 || candidate.kind == 3),
        "phrase candidates use the current exact/single kind encoding"
    );
    hc_session_free(session);

    let temp_dir = std::env::temp_dir().join(format!("hcime-history-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let temp = temp_dir.join("history.json");
    std::fs::write(&temp, b"not json").unwrap();
    let mut history = crate::han_nom::PhraseHistory::load(&temp);
    assert!(history.entries.is_empty());
    history.record("thành phố", "城庯");
    history.persist(&temp).unwrap();
    assert_eq!(
        crate::han_nom::PhraseHistory::load(&temp)
            .score("thành phố", "城庯")
            .0,
        1
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&temp_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&temp).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    history.reset(&temp);
    assert!(!temp.exists());
}

#[test]
fn hannom_v2_pages_single_glyph_candidates_without_treating_navigation_as_text() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV2 = unsafe { std::mem::zeroed() };
    for ch in "nhân".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    assert_eq!(result.candidate_count, 9, "test reading has a second page");
    let first_candidate = unsafe { *result.candidates };
    let first = unsafe {
        std::slice::from_raw_parts(first_candidate.text, first_candidate.text_len as usize)
    }
    .to_vec();
    let next = c("=");
    req.text = next.as_ptr();
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    assert!(result.candidate_count > 0);
    let second_candidate = unsafe { *result.candidates };
    let second = unsafe {
        std::slice::from_raw_parts(second_candidate.text, second_candidate.text_len as usize)
    };
    assert_ne!(
        first.as_slice(),
        second,
        "page navigation changes the candidate slice"
    );
    let previous = c("-");
    req.text = previous.as_ptr();
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    let restored_candidate = unsafe { *result.candidates };
    let restored = unsafe {
        std::slice::from_raw_parts(
            restored_candidate.text,
            restored_candidate.text_len as usize,
        )
    };
    assert_eq!(first.as_slice(), restored);
    hc_session_free(session);
}

#[test]
fn hannom_v2_defers_learning_file_io_until_explicit_flush() {
    let dir = std::env::temp_dir().join(format!("hcime-deferred-history-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("history.json");
    let path_c = c(path.to_str().unwrap());
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let options = HC_HanNomOptions {
        phrase_prediction: 1,
        learning_enabled: 1,
        history_path: path_c.as_ptr(),
    };
    hc_session_set_hannom_options(session, &options);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV2 = unsafe { std::mem::zeroed() };
    for ch in "Hà".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v2(session, &req, &mut result);
    for ch in "Nội".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v2(session, &req, &mut result);
    }
    assert_eq!(
        hc_session_select_hannom_candidate_v2(session, 0, &mut result),
        1
    );
    assert!(
        !path.exists(),
        "selection updates memory without file I/O in the key path"
    );
    hc_session_flush_hannom_learning(session);
    assert!(path.exists(), "lifecycle flush persists deferred learning");
    hc_session_free(session);
}

#[test]
fn hannom_v3_exposes_full_candidates_and_selects_absolute_index() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    for ch in "nhân".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        assert_eq!(
            hc_session_handle_key_hannom_v3(session, &req, &mut result),
            1
        );
    }
    assert_eq!(result.page_size, 9);
    assert!(
        result.candidate_count > 9,
        "V3 returns Fcitx-owned full list"
    );
    assert_eq!(result.candidate_count, result.total_candidate_count);
    assert_eq!(result.truncated, 0);
    let page_two = v3_candidate_text(&result, 9);
    assert_eq!(
        hc_session_select_hannom_candidate_v3(session, 9, &mut result),
        1
    );
    assert_eq!(
        v2_text(&HC_HanNomResultV2 {
            status_flag: result.status_flag,
            error_code: result.error_code,
            reading: result.reading,
            reading_len: result.reading_len,
            candidates: ptr::null(),
            candidate_count: 0,
            handled: result.handled,
        }),
        page_two
    );
    hc_session_free(session);
}

#[test]
fn hannom_v3_second_space_keeps_phrase_candidates_and_enter_commits_top_candidate() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };

    for ch in "thành".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v3(session, &req, &mut result);
    for ch in "phố".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }

    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v3(session, &req, &mut result);
    assert_eq!(result.status_flag, HCStatusFlag::InProgress as i32);
    assert!(result.candidate_count > 0);
    assert_eq!(v3_candidate_text(&result, 0), "城庯");

    req.kind = HCKeyKind::Enter as i32;
    req.text = ptr::null();
    hc_session_handle_key_hannom_v3(session, &req, &mut result);
    assert_eq!(result.status_flag, HCStatusFlag::Commit as i32);
    assert_eq!(
        v2_text(&HC_HanNomResultV2 {
            status_flag: result.status_flag,
            error_code: result.error_code,
            reading: result.reading,
            reading_len: result.reading_len,
            candidates: ptr::null(),
            candidate_count: 0,
            handled: result.handled,
        }),
        "城庯"
    );

    hc_session_free(session);
}

#[test]
fn hannom_v3_out_of_range_selection_is_non_mutating_and_user_tsv_wins() {
    let dir = std::env::temp_dir().join(format!("hcime-v3-tsv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("phrases.tsv");
    std::fs::write(&path, "# preferred\nhóa trang\t化妝\ninvalid\n").unwrap();
    let path_c = c(path.to_str().unwrap());
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    hc_session_set_hannom_options_v2(
        session,
        &HC_HanNomOptionsV2 {
            phrase_prediction: 1,
            learning_enabled: 0,
            history_path: ptr::null(),
            user_phrase_path: path_c.as_ptr(),
        },
    );
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    for ch in "hóa".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v3(session, &req, &mut result);
    for ch in "trang".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    assert_eq!(v3_candidate_text(&result, 0), "化妝");
    let reading_before =
        unsafe { std::slice::from_raw_parts(result.reading, result.reading_len as usize) }.to_vec();
    assert_eq!(
        hc_session_select_hannom_candidate_v3(session, 255, &mut result),
        0
    );
    let mut after: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    let equals = c("=");
    req.kind = HCKeyKind::Printable as i32;
    req.text = equals.as_ptr();
    hc_session_handle_key_hannom_v3(session, &req, &mut after);
    let reading_after =
        unsafe { std::slice::from_raw_parts(after.reading, after.reading_len as usize) };
    assert_eq!(reading_before, reading_after);

    std::fs::write(&path, "hóa trang\t化裝\n").unwrap();
    hc_session_reset(session);
    for ch in "hóa".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v3(session, &req, &mut result);
    for ch in "trang".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    assert_eq!(
        v3_candidate_text(&result, 0),
        "化裝",
        "reset reloads configured TSV"
    );
    hc_session_free(session);
}

#[test]
fn user_phrase_loader_rejects_bad_files_and_preserves_first_duplicate() {
    let dir = std::env::temp_dir().join(format!("hcime-user-phrases-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let valid = dir.join("valid.tsv");
    std::fs::write(
        &valid,
        "hóa trang\t化妝\nhóa trang\t化妝\na b\t甲乙\nhóa trang\tA乙\n",
    )
    .unwrap();
    let (entries, summary) = crate::han_nom::load_user_phrase_dict(&valid);
    assert_eq!(summary.loaded, 1);
    assert_eq!(summary.malformed, 2);
    assert_eq!(entries[0].glyphs, "化妝");
    let invalid = dir.join("invalid.tsv");
    std::fs::write(&invalid, [0xff, 0xfe]).unwrap();
    let (entries, summary) = crate::han_nom::load_user_phrase_dict(&invalid);
    assert!(entries.is_empty() && summary.invalid_utf8);
    let (entries, summary) = crate::han_nom::load_user_phrase_dict(&dir);
    assert!(
        entries.is_empty() && summary.unreadable,
        "non-regular path is rejected"
    );
}

#[test]
fn bundled_phrase_dictionary_has_aligned_nomstd_pairs_and_no_phrase_spill() {
    let phrases = crate::han_nom::get_global_phrase_dict().unwrap();
    assert!(phrases
        .exact("âm học")
        .iter()
        .any(|entry| entry.glyphs == "音學"));
    let hoa_trang = phrases.exact("hóa trang");
    assert!(hoa_trang.iter().any(|entry| entry.glyphs == "化裝"));
    assert!(hoa_trang.iter().any(|entry| entry.glyphs == "化妝"));
    let chars = crate::han_nom::get_global_dict().unwrap();
    assert!(chars.lookup("học").iter().all(|ch| !ch.is_ascii()));
    assert!(
        chars.lookup("học").len() <= 40,
        "phrase components must not pollute single candidates"
    );
}
