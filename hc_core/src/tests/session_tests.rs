use crate::test_helpers::*;
use crate::*;
use std::ptr;

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

    hc_session_test_set_last_commit_age(session, 2_001);

    req.kind = HCKeyKind::Backspace as i32;
    req.text = ptr::null();
    let back = hc_session_handle_key(session, &req);
    assert_eq!(back.handled, 0);
    free_state(back.state);

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
    assert_eq!(
        edit.state.status_flag,
        HCStatusFlag::ReconversionActive as i32
    );
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

    hc_session_test_set_last_commit_age(session, 2_001);

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
    assert_eq!(
        edit.state.status_flag,
        HCStatusFlag::ReconversionActive as i32
    );
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
fn macros_are_shared_across_sessions() {
    let session1 = hc_session_new(InputMode::Telex as i32, 0);
    let session2 = hc_session_new(InputMode::Telex as i32, 0);

    let macro_key = c("dc");
    let macro_val = c("Đà Nẵng");
    hc_session_add_macro(session1, macro_key.as_ptr(), macro_val.as_ptr());

    let mut req = key_request(InputMode::Telex);
    req.macro_in_english = 1;
    assert_eq!(type_raw(session2, &mut req, "dc"), "dc");
    let (committed, status) = commit_with_space(session2, &mut req);
    assert_eq!(committed, "Đà Nẵng");
    assert_eq!(status, HCStatusFlag::Commit as i32);

    hc_session_free(session1);
    hc_session_free(session2);
}
