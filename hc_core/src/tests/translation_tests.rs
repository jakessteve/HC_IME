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
            .all(|candidate| matches!(candidate.kind, 0 | 2 | 3)),
        "an exact-match page holds dictionary phrases (0), generated pairs (2) \
         or single glyphs (3) — never predictions (1)"
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
fn hannom_default_history_path_is_absolute_on_every_platform() {
    // NOM-01, first half: `dirs::state_dir()` is None on macOS, so the default
    // history path degraded to the relative "han_nom_history.json" and learning
    // was written next to whatever the host app's working directory happened to
    // be — in practice nowhere. Every supported platform must resolve a real
    // state directory.
    assert!(
        crate::platform::state_dir().is_some(),
        "every supported platform must resolve a state directory"
    );
    let path = crate::han_nom::default_history_path();
    assert!(
        path.is_absolute(),
        "default history path must be absolute, got {path:?}"
    );
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("han_nom_history.json")
    );
}

#[test]
fn hannom_history_persists_to_a_path_without_a_parent_directory() {
    // NOM-01, second half: for a bare filename `path.parent()` is Some(""), and
    // the 0700 chmod on "" failed ENOENT before the temp file was ever written,
    // so persist() silently wrote nothing. Independent of the path fix above.
    let dir = std::env::temp_dir().join(format!("hcime-bare-history-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let previous_cwd = std::env::current_dir().unwrap();
    // The bare filename resolves against the process cwd, so point it at a temp
    // directory — never at the user's home.
    std::env::set_current_dir(&dir).unwrap();

    let bare = std::path::Path::new("han_nom_history.json");
    let mut history = crate::han_nom::PhraseHistory::default();
    history.record("thành phố", "城庯");
    let outcome = history.persist(bare);
    let reloaded = crate::han_nom::PhraseHistory::load(bare)
        .score("thành phố", "城庯")
        .0;

    // Restore the process-wide cwd before asserting so a failure cannot leak it.
    std::env::set_current_dir(&previous_cwd).unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        outcome.is_ok(),
        "persist to a parentless path must succeed, got {outcome:?}"
    );
    assert_eq!(reloaded, 1, "the entry must survive the round trip");
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
    // A second Space re-reports the state without touching it. This used to
    // press `=`, but the v3 path no longer claims the pager keys (NOM-07) and
    // an unhandled key leaves the result struct zeroed.
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
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

/// Regression: a reading with no attested phrase entry produced 27 candidates
/// all labelled `kind = 0` (exact dictionary phrase). They are generated by
/// crossing two single-glyph lookups and must be labelled fallback so
/// frontends can rank and present them as machine-generated.
#[test]
fn generated_pairs_are_labelled_fallback_not_exact() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };

    for ch in "hair".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v3(session, &req, &mut result);
    for ch in "sao".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }

    assert!(result.candidate_count > 0, "expected generated pairs");
    let candidates =
        unsafe { std::slice::from_raw_parts(result.candidates, result.candidate_count as usize) };
    assert!(
        candidates.iter().all(|candidate| candidate.kind == 2),
        "\"hải sao\" has no dictionary phrase, so every candidate is generated \
         and must carry kind = 2 (fallback), never kind = 0 (exact)"
    );

    hc_session_free(session);
}

/// The rank stride controls candidate ordering: with `li * 3 + ri` the rows of
/// the cross product interleave (海… 駭… 海… again), because li=1,ri=0 ties with
/// li=0,ri=3. With a stride equal to the inner `take`, each left glyph's row
/// stays contiguous. Asserting on ordering is what catches the collision —
/// asserting on uniqueness of the generated texts does not, since a cross
/// product of two distinct lists yields distinct texts either way.
#[test]
fn generated_pair_rows_stay_contiguous() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };

    for ch in "hair".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    let space = c(" ");
    req.kind = HCKeyKind::Space as i32;
    req.text = space.as_ptr();
    hc_session_handle_key_hannom_v3(session, &req, &mut result);
    for ch in "sao".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }

    let candidates =
        unsafe { std::slice::from_raw_parts(result.candidates, result.candidate_count as usize) };
    let leading: Vec<char> = candidates
        .iter()
        .filter(|candidate| candidate.kind == 2)
        .filter_map(|candidate| {
            let text = unsafe {
                std::str::from_utf8(std::slice::from_raw_parts(
                    candidate.text,
                    candidate.text_len as usize,
                ))
                .ok()?
            };
            text.chars().next()
        })
        .collect();

    assert!(
        leading.len() > crate::translation::GENERATED_PAIR_WIDTH,
        "expected more than one row of generated pairs, got {}",
        leading.len()
    );

    // Walk the sequence: once a left glyph's row ends it must never resume.
    let mut finished = std::collections::HashSet::new();
    let mut current = leading[0];
    for &glyph in &leading[1..] {
        if glyph != current {
            assert!(
                finished.insert(current),
                "left glyph {current:?} row resumed after ending — rank collision"
            );
            current = glyph;
        }
    }

    hc_session_free(session);
}

/// Regression: `sort_phrase_candidates` used `dedup_by`, which only removes
/// *adjacent* duplicates. Once generated pairs were relabelled kind 2 they
/// sorted away from their kind 0 twins, so an attested phrase appeared twice —
/// once from the dictionary and once from the cross product. Frontends resolve
/// a selected candidate by matching its text, so duplicates silently commit the
/// wrong entry.
#[test]
fn attested_phrase_appears_once_and_outranks_its_generated_twin() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };

    let mut send = |req: &mut HC_KeyRequest, result: &mut HC_HanNomResultV3, keys: &str, kind| {
        for ch in keys.chars() {
            let key = c(&ch.to_string());
            req.kind = kind;
            req.text = key.as_ptr();
            hc_session_handle_key_hannom_v3(session, req, result);
        }
    };
    send(&mut req, &mut result, "thanhf", HCKeyKind::Printable as i32);
    send(&mut req, &mut result, " ", HCKeyKind::Space as i32);
    send(&mut req, &mut result, "phoos", HCKeyKind::Printable as i32);

    let candidates =
        unsafe { std::slice::from_raw_parts(result.candidates, result.candidate_count as usize) };
    let decoded: Vec<(String, u8)> = candidates
        .iter()
        .map(|candidate| unsafe {
            (
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    candidate.text,
                    candidate.text_len as usize,
                ))
                .into_owned(),
                candidate.kind,
            )
        })
        .collect();

    let mut texts: Vec<&String> = decoded.iter().map(|(text, _)| text).collect();
    let total = texts.len();
    texts.sort();
    texts.dedup();
    assert_eq!(
        total,
        texts.len(),
        "candidate texts must be unique — frontends select by text"
    );

    let attested: Vec<&(String, u8)> = decoded.iter().filter(|(text, _)| text == "城庯").collect();
    assert_eq!(attested.len(), 1, "城庯 must appear exactly once");
    assert_eq!(
        attested[0].1, 0,
        "the surviving copy must be the dictionary entry (kind 0), not the generated pair"
    );

    hc_session_free(session);
}

/// Regression: Hán Nôm backspace reported every keypress as handled, even with
/// an empty composition buffer. The client therefore never received the key and
/// could not erase text it had already committed.
#[test]
fn hannom_backspace_with_nothing_composing_is_unhandled() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };

    // Nothing typed yet.
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    assert_eq!(
        hc_session_handle_key_hannom_v3(session, &req, &mut result),
        0,
        "backspace on an empty buffer must fall through to the client"
    );

    // Compose and commit, then backspace again: the composition is empty once
    // more, so the key belongs to the client.
    for ch in "hai".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    req.kind = HCKeyKind::Enter as i32;
    req.text = ptr::null();
    hc_session_handle_key_hannom_v3(session, &req, &mut result);
    assert_eq!(result.status_flag, HCStatusFlag::Commit as i32);

    req.kind = HCKeyKind::Backspace as i32;
    assert_eq!(
        hc_session_handle_key_hannom_v3(session, &req, &mut result),
        0,
        "backspace after a commit must fall through so the client can delete"
    );

    // While composing, backspace is still ours.
    for ch in "hai".chars() {
        let key = c(&ch.to_string());
        req.kind = HCKeyKind::Printable as i32;
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, &req, &mut result);
    }
    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    assert_eq!(
        hc_session_handle_key_hannom_v3(session, &req, &mut result),
        1,
        "backspace during composition edits the reading"
    );

    hc_session_free(session);
}

// ── P2 Hán Nôm regressions ──

fn v3_texts(result: &HC_HanNomResultV3) -> Vec<String> {
    if result.candidates.is_null() {
        return Vec::new();
    }
    let candidates =
        unsafe { std::slice::from_raw_parts(result.candidates, result.candidate_count as usize) };
    candidates
        .iter()
        .map(|candidate| unsafe {
            String::from_utf8_lossy(std::slice::from_raw_parts(
                candidate.text,
                candidate.text_len as usize,
            ))
            .into_owned()
        })
        .collect()
}

fn v3_commit_text(result: &HC_HanNomResultV3) -> String {
    assert_eq!(result.status_flag, HCStatusFlag::Commit as i32);
    unsafe {
        String::from_utf8_lossy(std::slice::from_raw_parts(
            result.reading,
            result.reading_len as usize,
        ))
        .into_owned()
    }
}

/// Types a reading through the v3 handler. A space in `keys` is sent as
/// `HCKeyKind::Space`, everything else as `Printable`.
fn v3_type(
    session: *mut std::ffi::c_void,
    req: &mut HC_KeyRequest,
    result: &mut HC_HanNomResultV3,
    keys: &str,
) {
    for ch in keys.chars() {
        let key = c(&ch.to_string());
        req.kind = if ch == ' ' {
            HCKeyKind::Space as i32
        } else {
            HCKeyKind::Printable as i32
        };
        req.text = key.as_ptr();
        hc_session_handle_key_hannom_v3(session, req, result);
    }
}

fn v3_send(
    session: *mut std::ffi::c_void,
    req: &mut HC_KeyRequest,
    result: &mut HC_HanNomResultV3,
    kind: HCKeyKind,
    text: &str,
) -> i32 {
    let key = c(text);
    req.kind = kind as i32;
    req.text = if text.is_empty() {
        ptr::null()
    } else {
        key.as_ptr()
    };
    hc_session_handle_key_hannom_v3(session, req, result)
}

/// NOM-08, first half. `set_hannom_options` cleared `phrase_history_loaded` on
/// every call while leaving `phrase_history_dirty` set, so re-applying the same
/// options — which macOS does on every `configureHanNom()` — dropped everything
/// learned since the last flush and then wrote the reloaded copy back over the
/// file. Measured before the fix: ranking reverted and the file became
/// `{"entries":[]}`.
#[test]
fn hannom_reapplying_options_keeps_unflushed_learning() {
    let path = crate::han_nom::get_global_phrase_history_path();
    let _ = std::fs::remove_file(&path);

    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };

    v3_type(session, &mut req, &mut result, "hai nam");
    let ranked = v3_texts(&result);
    assert!(ranked.len() > 5, "expected a ranked candidate list");
    let learned = ranked[4].clone();
    assert_ne!(learned, ranked[0], "the learned glyph must not already win");
    assert_eq!(
        hc_session_select_hannom_candidate_v3(session, 4, &mut result),
        1
    );

    hc_session_reset(session);
    v3_type(session, &mut req, &mut result, "hai nam");
    assert_eq!(
        v3_texts(&result)[0],
        learned,
        "the selection must be learned in memory"
    );

    // Exactly what HCIMEInputController does on init and on every settings change.
    hc_session_set_hannom_options(
        session,
        &HC_HanNomOptions {
            phrase_prediction: 1,
            learning_enabled: 1,
            history_path: ptr::null(),
        },
    );
    hc_session_reset(session);
    v3_type(session, &mut req, &mut result, "hai nam");
    assert_eq!(
        v3_texts(&result)[0],
        learned,
        "re-applying options must not discard learning that has not reached disk yet"
    );

    hc_session_flush_hannom_learning(session);
    assert_eq!(
        crate::han_nom::PhraseHistory::load(&path)
            .score("hai nam", &learned)
            .0,
        1,
        "flush must persist the learned selection, not an emptied reload"
    );
    hc_session_free(session);
}

/// NOM-08, second half. The default history path was served from a process-wide
/// `OnceLock` snapshot of the file taken at process start, so no session ever
/// observed another session's writes. IMK creates one session per client
/// application, so the second application to start would flush its stale
/// snapshot over whatever the first had learned.
#[test]
fn hannom_default_history_is_reread_not_snapshotted_per_process() {
    let path = crate::han_nom::get_global_phrase_history_path();
    let _ = std::fs::remove_file(&path);

    let first = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    v3_type(first, &mut req, &mut result, "hai nam");
    let ranked = v3_texts(&result);
    let learned = ranked[4].clone();
    assert_ne!(learned, ranked[0]);
    assert_eq!(
        hc_session_select_hannom_candidate_v3(first, 4, &mut result),
        1
    );
    hc_session_flush_hannom_learning(first);
    hc_session_free(first);
    assert_eq!(
        crate::han_nom::PhraseHistory::load(&path)
            .score("hai nam", &learned)
            .0,
        1,
        "precondition: the first session wrote its learning to the shared file"
    );

    let second = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut second_result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    v3_type(second, &mut req, &mut second_result, "hai nam");
    assert_eq!(
        v3_texts(&second_result)[0],
        learned,
        "a session started after the write must read the file, not a snapshot \
         of it taken when the process began"
    );
    hc_session_free(second);
}

/// NOM-05. The toneless fallback iterated `entries`, a `HashMap` whose
/// `RandomState` is reseeded per process, so identical keystrokes produced a
/// different candidate order — and, because callers take only the first
/// `GENERATED_PAIR_WIDTH` of the list, a different candidate *set* — on every
/// launch. A freshly constructed dictionary reproduces a fresh process's
/// hashing seed.
#[test]
fn nom_toneless_fallback_is_identical_across_dictionary_instances() {
    let reference =
        crate::han_nom::EmbeddedNomDict::from_binary(crate::han_nom::EMBEDDED_DICT_DATA).unwrap();
    let baseline = reference.lookup("nàm");
    assert!(
        baseline.len() > crate::translation::GENERATED_PAIR_WIDTH,
        "the probe reading must miss the exact lookup and return more candidates \
         than the cross product consumes, or a reordering would be invisible"
    );

    for attempt in 0..8 {
        let other =
            crate::han_nom::EmbeddedNomDict::from_binary(crate::han_nom::EMBEDDED_DICT_DATA)
                .unwrap();
        assert_eq!(
            other.lookup("nàm"),
            baseline,
            "attempt {attempt}: the toneless fallback must not depend on hash order"
        );
    }

    // The merge itself is unchanged: every toned spelling still contributes.
    for reading in ["nam", "nắm", "năm"] {
        for glyph in reference.lookup(reading) {
            assert!(
                baseline.contains(&glyph),
                "{reading} contributes {glyph} to the toneless fallback"
            );
        }
    }
    // NOM-13 (backlog) — the fallback still merges readings that differ only by
    // đ/d. Asserted so a future change to the index cannot quietly widen it.
    assert_eq!(reference.lookup("dá"), reference.lookup("đạ"));
}

/// PERF-03. Every incomplete reading misses the exact lookup, so the fallback
/// ran 2–4 times per Hán Nôm keystroke; it scanned all 7,079 keys and allocated
/// a `String` per key, measured at 410–440 µs against 0.04 µs for a hit.
#[test]
fn nom_dictionary_miss_does_not_strip_every_key() {
    let dict = crate::han_nom::get_global_dict().unwrap();
    assert!(
        dict.len() > 1000,
        "precondition: a real dictionary is loaded"
    );

    crate::han_nom::probe::reset();
    let fallback = dict.lookup("nàm");
    let strips = crate::han_nom::probe::toneless_strips();
    assert!(
        !fallback.is_empty(),
        "the probe reading must actually take the fallback path"
    );
    assert!(
        strips <= 2,
        "a dictionary miss stripped marks {strips} times over {} entries — \
         the fallback is scanning every key instead of using an index",
        dict.len()
    );
}

/// NOM-06. Every arm of the Escape handler returned 1, so in Hán Nôm mode
/// Escape never reached the application: no dialog dismissal, no vim normal
/// mode. Vietnamese and the Hán Nôm backspace path both fall through here.
#[test]
fn hannom_escape_with_nothing_composing_is_unhandled() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };

    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Escape, ""),
        0,
        "escape on a fresh session belongs to the application"
    );

    v3_type(session, &mut req, &mut result, "hai");
    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Escape, ""),
        1,
        "escape cancels a live composition"
    );
    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Escape, ""),
        0,
        "once the composition is cancelled the next escape is the application's"
    );

    v3_type(session, &mut req, &mut result, "hai");
    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Enter, ""),
        1
    );
    assert_eq!(result.status_flag, HCStatusFlag::Commit as i32);
    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Escape, ""),
        0,
        "escape after a commit belongs to the application"
    );

    // Two-word state: the first escape drops the pending first word, the second
    // has nothing left to cancel.
    v3_type(session, &mut req, &mut result, "hai ");
    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Escape, ""),
        1
    );
    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Escape, ""),
        0
    );

    hc_session_free(session);
}

/// NOM-07. `=`, `+`, `[`, `]` and `-` were intercepted as candidate-page
/// navigation before any emptiness check, so on a fresh session all five
/// returned handled with no state change and `1+1=2` lost its operators. On the
/// shipping v3/v4 path they did nothing at all, because `populate_nom_result_v3`
/// ignores `phrase_candidate_page`.
#[test]
fn hannom_pager_keys_reach_the_client_when_they_cannot_page() {
    for key in ["=", "+", "[", "]", "-"] {
        let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
        let mut req = key_request(InputMode::HanNomTelex);
        let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
        assert_eq!(
            v3_send(session, &mut req, &mut result, HCKeyKind::Printable, key),
            0,
            "{key:?} on a fresh session has no candidate list to page and must \
             reach the application"
        );
        hc_session_free(session);

        // The deprecated v2 entry point still pages, but only when there is
        // something to page — see
        // hannom_v2_pages_single_glyph_candidates_without_treating_navigation_as_text.
        let v2_session = hc_session_new(InputMode::HanNomTelex as i32, 0);
        let mut v2_req = key_request(InputMode::HanNomTelex);
        let mut v2_result: HC_HanNomResultV2 = unsafe { std::mem::zeroed() };
        let text = c(key);
        v2_req.kind = HCKeyKind::Printable as i32;
        v2_req.text = text.as_ptr();
        #[allow(deprecated)]
        let handled = hc_session_handle_key_hannom_v2(v2_session, &v2_req, &mut v2_result);
        assert_eq!(
            handled, 0,
            "{key:?} on a fresh v2 session is the client's too"
        );
        hc_session_free(v2_session);
    }

    // `1+1=2`: every character the engine does not own reaches the application.
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    for key in ["1", "+", "1", "=", "2"] {
        assert_eq!(
            v3_send(session, &mut req, &mut result, HCKeyKind::Printable, key),
            0,
            "typing 1+1=2 must not lose {key:?}"
        );
    }
    hc_session_free(session);

    // While composing on the v3 path the key is still the client's, and it must
    // not be absorbed into the reading either: the commit is unchanged.
    let control = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut control_result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    v3_type(control, &mut req, &mut control_result, "nhaan");
    v3_send(control, &mut req, &mut control_result, HCKeyKind::Enter, "");
    let expected = v3_commit_text(&control_result);
    hc_session_free(control);

    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    v3_type(session, &mut req, &mut result, "nhaan");
    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Printable, "="),
        0,
        "v3 hands the whole candidate list to the frontend, so `=` never paged \
         anything and must not be swallowed"
    );
    v3_send(session, &mut req, &mut result, HCKeyKind::Enter, "");
    assert_eq!(
        v3_commit_text(&result),
        expected,
        "the declined key must not have entered the reading"
    );
    hc_session_free(session);
}

/// NOM-09. The punctuation commit took `phrase_candidates.first()` without
/// checking that a second syllable had been typed, so after `hai` + Space every
/// candidate was a *prediction* and `(` committed `𠄩𤖸(` — "hai chấm" — plus a
/// learned selection for a word the user never typed.
#[test]
fn hannom_punctuation_after_space_commits_only_the_typed_word() {
    let path = crate::han_nom::get_global_phrase_history_path();
    let _ = std::fs::remove_file(&path);

    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    v3_type(session, &mut req, &mut result, "hai ");
    assert!(
        v3_texts(&result)
            .iter()
            .any(|text| text.chars().count() == 2),
        "precondition: the prediction list offers two-glyph phrases"
    );

    assert_eq!(
        v3_send(session, &mut req, &mut result, HCKeyKind::Printable, "("),
        1
    );
    let committed = v3_commit_text(&result);
    let glyphs: Vec<char> = committed.chars().collect();
    assert_eq!(
        glyphs.len(),
        2,
        "committed {committed:?} — only the typed syllable and the punctuation \
         belong here, never a predicted second syllable"
    );
    assert_eq!(glyphs[1], '(');
    assert!(
        crate::han_nom::get_global_dict()
            .unwrap()
            .lookup("hai")
            .contains(&glyphs[0]),
        "{:?} must be a Nôm glyph for the syllable that was typed",
        glyphs[0]
    );

    hc_session_flush_hannom_learning(session);
    let saved = crate::han_nom::PhraseHistory::load(&path);
    assert!(
        saved.entries.iter().all(|entry| entry.reading == "hai"),
        "learning must be keyed to the syllable the user typed, got {:?}",
        saved.entries
    );
    assert_eq!(saved.score("hai", &glyphs[0].to_string()).0, 1);
    hc_session_free(session);
}

/// PERF-04 and NOM-12. `populate_nom_result_v2` rebuilt the candidate list and
/// `populate_nom_result_v3` immediately rebuilt it again, discarding the first
/// result — instrumented at `rebuild=2 sort=2` on every key. And the sort
/// comparator called `PhraseHistory::score` twice per comparison, making the
/// number of history scans Θ(n log n) in the candidate count.
#[test]
fn hannom_keystroke_rebuilds_once_and_scores_each_candidate_once() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    v3_type(session, &mut req, &mut result, "n");

    crate::han_nom::probe::reset();
    v3_send(session, &mut req, &mut result, HCKeyKind::Space, " ");
    let rebuilds = crate::han_nom::probe::phrase_rebuilds();
    let scores = crate::han_nom::probe::history_score_calls();
    let candidates = result.total_candidate_count as usize;

    assert!(
        candidates > 64,
        "the probe keystroke must produce a large candidate list, got {candidates}"
    );
    assert_eq!(
        rebuilds, 1,
        "one keystroke must rebuild the candidate list once, not {rebuilds} times"
    );
    // One score per candidate, plus the duplicates the ranker discards after
    // sorting. Scoring inside the comparator instead measured 4,040 lookups on
    // this exact keystroke.
    assert!(
        scores <= candidates + candidates / 8,
        "ranking {candidates} candidates took {scores} history lookups — the \
         comparator is scoring per comparison instead of per candidate"
    );
    hc_session_free(session);
}

/// NOM-12. `record` capped the history at 2,048 entries but `load` applied no
/// bound at all, so a file that grew by any other route was taken whole and
/// every ranking comparison walked it.
#[test]
fn phrase_history_load_is_bounded_like_record() {
    let dir = std::env::temp_dir().join(format!("hcime-history-cap-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("oversized.json");
    let oversized = crate::han_nom::PhraseHistory {
        entries: (0..5000)
            .map(|index| crate::han_nom::PhraseHistoryEntry {
                reading: format!("reading{index} second{index}"),
                glyphs: format!("glyph{index}"),
                count: 1,
                last_used: index as u64,
            })
            .collect(),
    };
    std::fs::write(&path, serde_json::to_vec(&oversized).unwrap()).unwrap();

    let loaded = crate::han_nom::PhraseHistory::load(&path);
    assert_eq!(
        loaded.entries.len(),
        crate::han_nom::PHRASE_HISTORY_MAX_ENTRIES,
        "an oversized history file must be truncated on load"
    );
    assert_eq!(
        loaded.score("reading4999 second4999", "glyph4999").0,
        1,
        "the most recently used entries survive"
    );
    assert_eq!(
        loaded.score("reading0 second0", "glyph0"),
        (0, 0),
        "the least recently used entries are evicted"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// NOM-17 / the composition cap. `raw_buffer.len() < 64` was measured *before*
/// the append, so a key text that straddled the limit landed in full: 63 bytes
/// plus a four-byte scalar produced a 67-byte reading. The cap is in bytes —
/// see `MAX_READING_BYTES`.
#[test]
fn hannom_reading_cap_counts_the_key_being_added() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };

    // `b` is not a Telex trigger, so the rendered reading tracks the raw buffer
    // byte for byte.
    for _ in 0..63 {
        v3_send(session, &mut req, &mut result, HCKeyKind::Printable, "b");
    }
    assert_eq!(
        result.reading_len, 63,
        "precondition: the buffer sits one byte below the cap"
    );

    // A four-byte scalar cannot fit in the one remaining byte.
    v3_send(session, &mut req, &mut result, HCKeyKind::Printable, "𠄩");
    assert_eq!(
        result.reading_len, 63,
        "a key that does not fit must be rejected whole, not appended past the cap"
    );

    hc_session_free(session);
}

/// FFI-06. The published candidate pointers borrowed the `String`s inside
/// `phrase_candidates`, which `reset()` drops — and `reset()` runs on every
/// commit, on `hc_session_select_hannom_candidate_v3` (both frontends call it)
/// and on `hc_session_reset`, none of which hand the caller a replacement list.
/// The bytes are copied into an arena that is rewritten only when a new list is
/// published.
#[test]
fn hannom_published_candidates_do_not_borrow_the_list_reset_drops() {
    let session = hc_session_new(InputMode::HanNomTelex as i32, 0);
    let mut req = key_request(InputMode::HanNomTelex);
    let mut result: HC_HanNomResultV3 = unsafe { std::mem::zeroed() };
    v3_type(session, &mut req, &mut result, "hai nam");
    assert!(result.candidate_count > 0);

    let published = v3_texts(&result);
    let candidates = result.candidates;
    let count = result.candidate_count as usize;
    let published_bytes = unsafe { (*candidates).text };

    let live_bytes = {
        let session = unsafe { &mut *(session as *mut crate::session::Session) };
        let translator = session.translator_mut().expect("Hán Nôm session");
        translator.phrase_candidates[0].text.as_ptr()
    };
    assert_ne!(
        published_bytes, live_bytes,
        "published candidate pointers must not address the candidate list that \
         reset() drops"
    );

    // Nothing below publishes a replacement, so the pointers stay readable.
    hc_session_reset(session);
    let after_reset: Vec<String> = unsafe { std::slice::from_raw_parts(candidates, count) }
        .iter()
        .map(|candidate| unsafe {
            String::from_utf8_lossy(std::slice::from_raw_parts(
                candidate.text,
                candidate.text_len as usize,
            ))
            .into_owned()
        })
        .collect();
    assert_eq!(
        after_reset, published,
        "hc_session_reset must not free them"
    );

    v3_type(session, &mut req, &mut result, "hai nam");
    let candidates = result.candidates;
    let count = result.candidate_count as usize;
    let published = v3_texts(&result);
    assert_eq!(
        hc_session_select_hannom_candidate_v3(session, 0, &mut result),
        1
    );
    let after_select: Vec<String> = unsafe { std::slice::from_raw_parts(candidates, count) }
        .iter()
        .map(|candidate| unsafe {
            String::from_utf8_lossy(std::slice::from_raw_parts(
                candidate.text,
                candidate.text_len as usize,
            ))
            .into_owned()
        })
        .collect();
    assert_eq!(
        after_select, published,
        "selecting a candidate must not free the list the frontend is still \
         showing"
    );

    hc_session_free(session);
}
