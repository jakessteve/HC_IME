use hc_core::*;

#[test]
fn test_workflow_vni() {
    let mut session = hc_session_new(1, 0); // 1 is VNI
    let reqs = ["w", "o", "r", "k", "f", "l", "o", "w"];
    for ch in reqs {
        let req = HC_KeyRequest {
            kind: HCKeyKind::Printable as i32,
            text: std::ffi::CString::new(ch).unwrap().into_raw(),
            input_mode: 1, // VNI
            legacy_tone: 0,
            spell_check: 0, // off
            auto_restore: 0, // off
            quick_consonants: 0,
            english_protection: 0,
            macro_in_english: 0,
            esc_restore_raw: 0,
        };
        let res = hc_session_handle_key_utf8(session, &req);
        let s = unsafe { std::ffi::CStr::from_ptr(res.composition_string).to_str().unwrap() };
        println!("After {}: {}", ch, s);
    }
}
