use std::cell::RefCell;
use std::ffi::{c_char, CStr};
use std::ptr;
#[cfg(test)]
use std::time::{Duration, Instant};

pub mod compose;
pub mod composition;
pub mod han_nom;
mod language;
pub mod platform;
mod quick_consonants;
mod session;
mod transform;
pub mod translation;
mod types;
mod vowel;

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

pub use types::*;

use session::Session;

use composition::get_global_macros;
use language::is_known_english_word;
use transform::{apply_circumflex, apply_telex_w, apply_tone_to_word};

use vowel::strip_all_marks;

#[cfg(test)]
pub(crate) fn hc_session_test_set_last_commit_age(session: *mut std::ffi::c_void, age_ms: u64) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        session.composition.last_commit_time = Some(
            Instant::now()
                .checked_sub(Duration::from_millis(age_ms))
                .unwrap_or_else(Instant::now),
        );
    }
}

thread_local! {
    static UTF8_RESULT_BUFFER: RefCell<String> = const { RefCell::new(String::new()) };
    /// Backing store for every pointer published in `HC_KeyResultV2`.
    static V4_RESULT_BUFFER: RefCell<V4ResultBuffer> = RefCell::new(V4ResultBuffer::default());
}

/// Largest `text` payload accepted for one key event, in bytes.
///
/// A key event carries a single keystroke; both shipped frontends send at most
/// four bytes. The ABI used to accept whatever a `const char*` could reach, and
/// the raw-buffer render is O(n²), so one 1 MB "keystroke" never returned
/// (FFI-08). Anything longer than this is rejected with
/// `HCErrorCode::TextTooLong` before it can reach the composition engine.
pub const HC_MAX_KEY_TEXT_BYTES: usize = 64;

/// Where one candidate's bytes live inside [`V4ResultBuffer::arena`].
#[derive(Clone, Copy, Default)]
struct CandidateSpan {
    text_offset: usize,
    text_len: usize,
    reading_offset: usize,
    reading_len: usize,
    kind: u8,
}

/// Owns the memory behind `HC_KeyResultV2::composition_string` and
/// `HC_KeyResultV2::candidates`.
///
/// `hc_session_handle_key_v4` used to publish a thread-local pointer on the
/// Vietnamese branch and a session-owned pointer on the Hán Nôm branch, so no
/// single caller rule was correct for the same field (FFI-05), and
/// `hc_session_reset` freed the candidate strings a caller still held (FFI-06).
/// Both branches now copy into this thread-local buffer, giving the whole
/// result struct one owner and one lifetime: valid until the next
/// `hc_session_handle_key_v4` call on the same thread, and unaffected by
/// `hc_session_reset` / `hc_session_free`.
#[derive(Default)]
struct V4ResultBuffer {
    text: String,
    arena: Vec<u8>,
    spans: Vec<CandidateSpan>,
    entries: Vec<HC_HanNomCandidateText>,
}

impl V4ResultBuffer {
    fn set_text_from_state(&mut self, state: &HC_State) {
        state_to_utf8_into(state, &mut self.text);
    }

    /// Copies UTF-8 bytes borrowed from session memory into the buffer.
    ///
    /// # Safety
    /// `ptr` must be null or point to `len` readable bytes.
    unsafe fn set_text_from_borrowed(&mut self, ptr: *const u8, len: usize) {
        self.text.clear();
        if ptr.is_null() || len == 0 {
            return;
        }
        let bytes = std::slice::from_raw_parts(ptr, len);
        match std::str::from_utf8(bytes) {
            Ok(text) => self.text.push_str(text),
            // A u16 length cut can split a codepoint (FFI-09). Publishing
            // replacement characters is still valid UTF-8; publishing a broken
            // tail is what makes the C side hard to reason about.
            Err(_) => self.text.push_str(&String::from_utf8_lossy(bytes)),
        }
    }

    /// Copies a borrowed candidate array — entries and the bytes they point at.
    ///
    /// # Safety
    /// `src` must be null or point to `count` readable `HC_HanNomCandidateText`
    /// entries whose own pointers are readable for their declared lengths.
    unsafe fn set_candidates(&mut self, src: *const HC_HanNomCandidateText, count: u16) {
        self.arena.clear();
        self.spans.clear();
        self.entries.clear();
        if src.is_null() || count == 0 {
            return;
        }
        for candidate in std::slice::from_raw_parts(src, count as usize) {
            let (text_offset, text_len) = self.push_bytes(candidate.text, candidate.text_len);
            let (reading_offset, reading_len) =
                self.push_bytes(candidate.reading, candidate.reading_len);
            self.spans.push(CandidateSpan {
                text_offset,
                text_len,
                reading_offset,
                reading_len,
                kind: candidate.kind,
            });
        }
        // The arena is complete, so its address is now stable for this call.
        let base = self.arena.as_ptr();
        let entries = &mut self.entries;
        for span in &self.spans {
            entries.push(HC_HanNomCandidateText {
                text: if span.text_len == 0 {
                    ptr::null()
                } else {
                    base.add(span.text_offset)
                },
                text_len: span.text_len as u16,
                reading: if span.reading_len == 0 {
                    ptr::null()
                } else {
                    base.add(span.reading_offset)
                },
                reading_len: span.reading_len as u16,
                kind: span.kind,
            });
        }
    }

    /// # Safety
    /// `ptr` must be null or point to `len` readable bytes.
    unsafe fn push_bytes(&mut self, ptr: *const u8, len: u16) -> (usize, usize) {
        let len = len as usize;
        if ptr.is_null() || len == 0 {
            return (self.arena.len(), 0);
        }
        let offset = self.arena.len();
        self.arena
            .extend_from_slice(std::slice::from_raw_parts(ptr, len));
        (offset, len)
    }

    fn text_ptr(&self) -> *const u8 {
        if self.text.is_empty() {
            ptr::null()
        } else {
            self.text.as_ptr()
        }
    }

    fn candidates_ptr(&self) -> *const HC_HanNomCandidateText {
        if self.entries.is_empty() {
            ptr::null()
        } else {
            self.entries.as_ptr()
        }
    }
}

/// A `HC_KeyResultV2` that carries only an error — every field initialised, no
/// pointer published.
fn v4_error_result(error: HCErrorCode) -> HC_KeyResultV2 {
    HC_KeyResultV2 {
        composition_string: ptr::null(),
        composition_len: 0,
        status_flag: HCStatusFlag::InProgress as i32,
        error_code: error as i32,
        spell_check_status: HCSpellCheckStatus::Valid as i32,
        handled: 0,
        candidates: ptr::null(),
        candidate_count: 0,
        total_candidate_count: 0,
    }
}

/// Rejects a key event whose text exceeds [`HC_MAX_KEY_TEXT_BYTES`] (FFI-08).
///
/// The NUL scan is unavoidable — a `const char*` carries no length — but it is
/// linear and cheap. The unbounded cost was in composing the text afterwards,
/// so the length is checked before UTF-8 validation and before any push.
fn check_key_text_len(ptr: *const c_char) -> Result<(), HCErrorCode> {
    if ptr.is_null() {
        return Ok(());
    }
    let len = unsafe { CStr::from_ptr(ptr) }.to_bytes().len();
    if len > HC_MAX_KEY_TEXT_BYTES {
        Err(HCErrorCode::TextTooLong)
    } else {
        Ok(())
    }
}

impl Session {
    fn handle_key(&mut self, request: &HC_KeyRequest) -> HC_KeyResult {
        self.composition.mode = match InputMode::try_from(request.input_mode) {
            Ok(mode) => mode,
            Err(_) => {
                return HC_KeyResult {
                    state: hc_error_state(HCErrorCode::InvalidInputMode),
                    handled: 0,
                }
            }
        };
        if let Err(err) = check_key_text_len(request.text) {
            return HC_KeyResult {
                state: hc_error_state(err),
                handled: 0,
            };
        }
        self.composition.legacy_tone = request.legacy_tone != 0;
        self.composition.spell_check = request.spell_check != 0;
        self.composition.auto_restore = request.auto_restore != 0;
        self.composition.quick_consonants_enabled = request.quick_consonants != 0;
        self.composition.english_protection =
            EnglishProtectionLevel::from(request.english_protection);
        self.composition.macro_in_english = request.macro_in_english != 0;
        self.composition.esc_restore_raw = request.esc_restore_raw != 0;

        if let Some(kind) = key_kind(request.kind) {
            match kind {
                HCKeyKind::Other => {
                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
                HCKeyKind::Escape => {
                    if let Some(raw) = self.composition.try_esc_restore_raw() {
                        return HC_KeyResult {
                            state: hc_state_from_string(
                                &raw,
                                HCStatusFlag::EscRestoredRaw,
                                HCErrorCode::None,
                            ),
                            handled: 1,
                        };
                    }
                    if self.composition.buffer.is_empty() && self.composition.last_commit.is_empty()
                    {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::None),
                            handled: 0,
                        };
                    }
                    self.reset();
                    return HC_KeyResult {
                        state: hc_state_from_string(
                            "",
                            HCStatusFlag::InProgress,
                            HCErrorCode::None,
                        ),
                        handled: 1,
                    };
                }
                HCKeyKind::Backspace => {
                    if !self.composition.raw_buffer.is_empty() {
                        match self.composition.mode {
                            InputMode::Vni => {
                                self.composition.raw_buffer = vni_raw_after_visible_backspace(
                                    &self.composition.raw_buffer,
                                    &self.composition.buffer,
                                    self.composition.legacy_tone,
                                );
                                self.composition.render_from_raw();
                            }
                            _ => {
                                self.composition.raw_buffer.pop();
                                if self.composition.quick_consonants_enabled {
                                    self.composition.quick_consonant_lock = self
                                        .composition
                                        .quick_consonant_lock
                                        .min(self.composition.raw_buffer.len());
                                }
                                self.composition.render_from_raw();
                            }
                        }
                        if self.composition.raw_buffer.is_empty() {
                            self.composition.reconversion_active = false;
                        }
                        return self.composition.emit_preedit(true);
                    }

                    if self.composition.can_edit_last_commit() {
                        self.composition.buffer = self.composition.last_commit.clone();
                        self.composition.raw_buffer = if self.composition.last_raw.is_empty() {
                            strip_all_marks(&self.composition.buffer)
                        } else {
                            self.composition.last_raw.clone()
                        };
                        self.composition.reconversion_active = true;
                        self.composition.last_commit.clear();
                        self.composition.last_raw.clear();
                        self.composition.last_commit_time = None;
                        return HC_KeyResult {
                            state: hc_state_from_string(
                                &self.composition.buffer,
                                HCStatusFlag::ReconversionActive,
                                HCErrorCode::None,
                            ),
                            handled: 1,
                        };
                    }

                    // Expired reconversion state must not linger after a rejected
                    // backspace; the client receives its normal key handling.
                    self.composition.clear_last_commit();

                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
                HCKeyKind::Enter | HCKeyKind::Space | HCKeyKind::Boundary => {
                    if self.composition.buffer.is_empty() {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::None),
                            handled: 0,
                        };
                    }

                    if kind == HCKeyKind::Boundary
                        && self.composition.mode == InputMode::Viqr
                        && self.composition.try_boundary_trigger(request.text)
                    {
                        return self.composition.emit_preedit(true);
                    }

                    self.composition.apply_end_quick_consonants_if_enabled();

                    let commit = self.composition.commit_current();
                    return HC_KeyResult {
                        state: commit,
                        handled: 1,
                    };
                }
                HCKeyKind::Printable => {
                    let Some(text) = key_text(request.text) else {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::InvalidUtf8),
                            handled: 0,
                        };
                    };

                    self.composition.reconversion_active = false;
                    let mut chars = text.chars();
                    let Some(first_char) = chars.next() else {
                        return HC_KeyResult {
                            state: hc_error_state(HCErrorCode::InvalidUtf8),
                            handled: 0,
                        };
                    };
                    let single_char = chars.next().is_none();

                    self.composition.last_commit.clear();
                    self.composition.last_raw.clear();
                    self.composition.last_commit_time = None;

                    if self.composition.mode == InputMode::Vni
                        && single_char
                        && first_char.is_ascii_digit()
                    {
                        if self.composition.buffer.is_empty()
                            && self.composition.raw_buffer.is_empty()
                        {
                            return HC_KeyResult {
                                state: hc_error_state(HCErrorCode::None),
                                handled: 0,
                            };
                        }

                        let mut probe = self.composition.buffer.clone();
                        let transforms = crate::compose::TypingEngine::apply_vni_trigger(
                            &mut probe,
                            first_char,
                            self.composition.legacy_tone,
                        );
                        if !transforms {
                            if probe != self.composition.buffer {
                                self.composition.buffer = probe;
                                self.composition.raw_buffer.push(first_char);
                                self.composition.buffer.push(first_char);
                                self.composition.update_spell_check_status();
                                return self.composition.emit_preedit(true);
                            }
                            self.composition.raw_buffer.push(first_char);
                            self.composition.buffer.push(first_char);
                            let commit = self.composition.commit_current();
                            return HC_KeyResult {
                                state: commit,
                                handled: 1,
                            };
                        }
                    }

                    self.composition.raw_buffer.push_str(text);
                    self.composition.render_from_raw();
                    return self.composition.emit_preedit(true);
                }
                HCKeyKind::Undo => {
                    if self.composition.undo() {
                        return self.composition.emit_preedit(true);
                    }
                    return HC_KeyResult {
                        state: hc_error_state(HCErrorCode::None),
                        handled: 0,
                    };
                }
            }
        }

        HC_KeyResult {
            state: hc_error_state(HCErrorCode::InvalidEditTrigger),
            handled: 0,
        }
    }
}

fn vni_raw_after_visible_backspace(raw: &str, rendered: &str, legacy_tone: bool) -> String {
    let mut target = rendered.to_string();
    if target.pop().is_none() {
        let mut fallback = raw.to_string();
        fallback.pop();
        return fallback;
    }

    let raw_chars: Vec<char> = raw.chars().collect();
    for primary_idx in (0..raw_chars.len()).rev() {
        let extra_digit_indices: Vec<usize> = ((primary_idx + 1)..raw_chars.len())
            .filter(|&idx| raw_chars[idx].is_ascii_digit())
            .collect();

        if let Some(candidate) = matching_vni_backspace_candidate(
            &raw_chars,
            primary_idx,
            &extra_digit_indices,
            &target,
            legacy_tone,
        ) {
            return candidate;
        }
    }

    let mut fallback = raw.to_string();
    fallback.pop();
    fallback
}

fn matching_vni_backspace_candidate(
    raw_chars: &[char],
    primary_idx: usize,
    extra_digit_indices: &[usize],
    target: &str,
    legacy_tone: bool,
) -> Option<String> {
    const MAX_EXTRA_DIGITS_FOR_EXACT_SEARCH: usize = 12;

    if extra_digit_indices.len() > MAX_EXTRA_DIGITS_FOR_EXACT_SEARCH {
        return candidate_if_matches(raw_chars, primary_idx, &[], target, legacy_tone);
    }

    let subset_count = 1usize << extra_digit_indices.len();
    for removed_extra_count in 0..=extra_digit_indices.len() {
        for mask in 0..subset_count {
            if mask.count_ones() as usize != removed_extra_count {
                continue;
            }
            let removed_extras: Vec<usize> = extra_digit_indices
                .iter()
                .enumerate()
                .filter_map(|(bit, idx)| ((mask & (1usize << bit)) != 0).then_some(*idx))
                .collect();
            if let Some(candidate) =
                candidate_if_matches(raw_chars, primary_idx, &removed_extras, target, legacy_tone)
            {
                return Some(candidate);
            }
        }
    }

    None
}

fn candidate_if_matches(
    raw_chars: &[char],
    primary_idx: usize,
    removed_extras: &[usize],
    target: &str,
    legacy_tone: bool,
) -> Option<String> {
    let candidate: String = raw_chars
        .iter()
        .enumerate()
        .filter_map(|(idx, ch)| {
            (idx != primary_idx && !removed_extras.contains(&idx)).then_some(*ch)
        })
        .collect();

    (composition::render_raw_input(&candidate, InputMode::Vni, legacy_tone) == target)
        .then_some(candidate)
}

/// Creates a session, or returns NULL if `input_mode` is not a valid
/// `HC_InputMode` (0–5). Every caller's `if (!session)` guard used to be
/// unreachable because any value produced a Telex session (FFI-04).
///
/// Threading: a session is not internally synchronised. Two threads calling
/// into the same session pointer is undefined behaviour and has been observed
/// to fault inside the engine; separate sessions on separate threads are safe,
/// and the global macro map and dictionaries are internally synchronised
/// (FFI-02). See the threading contract in `hc_core_ffi.h`.
#[no_mangle]
pub extern "C" fn hc_session_new(input_mode: i32, legacy_tone: u8) -> *mut std::ffi::c_void {
    let mode = match InputMode::try_from(input_mode) {
        Ok(mode) => mode,
        Err(_) => return ptr::null_mut(),
    };
    Box::into_raw(Box::new(Session::new(mode, legacy_tone != 0))) as *mut std::ffi::c_void
}

#[no_mangle]
pub extern "C" fn hc_session_free(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let mut session = Box::from_raw(session as *mut Session);
        if let Some(t) = session.translator_mut() {
            t.flush_hannom_learning();
        }
        drop(session);
    }
}

/// Clears the composing state of `session`.
///
/// Invalidates every pointer previously published in an `HC_HanNomResult`,
/// `HC_HanNomResultV2` or `HC_HanNomResultV3` for this session: the candidate
/// strings are dropped here (FFI-06). The outer candidate array keeps its
/// address because the vector's allocation is reused, so a cached array pointer
/// looks valid while the text pointers inside it dangle — callers must copy the
/// strings out before any other call on the session, as the header now states.
///
/// `HC_KeyResultV2` (`hc_session_handle_key_v4`) is *not* affected: that result
/// is copied into a thread-local buffer owned by this library.
#[no_mangle]
pub extern "C" fn hc_session_reset(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        session.composition.reset();
        if let Some(t) = session.translator_mut() {
            t.reset();
            t.reload_user_phrase_entries();
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_add_macro(
    _session: *mut std::ffi::c_void,
    key: *const c_char,
    value: *const c_char,
) {
    if key.is_null() || value.is_null() {
        return;
    }
    unsafe {
        let key_str = match CStr::from_ptr(key).to_str() {
            Ok(s) => s,
            Err(_) => return,
        };
        let value_str = match CStr::from_ptr(value).to_str() {
            Ok(s) => s,
            Err(_) => return,
        };
        get_global_macros()
            .write()
            .unwrap()
            .insert(key_str.to_lowercase(), value_str.to_string());
    }
}

#[no_mangle]
pub extern "C" fn hc_session_clear_macros(_session: *mut std::ffi::c_void) {
    get_global_macros().write().unwrap().clear();
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
) -> HC_KeyResult {
    if session.is_null() || request.is_null() {
        return HC_KeyResult {
            state: hc_error_state(HCErrorCode::NullPointer),
            handled: 0,
        };
    }

    unsafe {
        let session = &mut *(session as *mut Session);
        session.handle_key(&*request)
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key_utf8(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
) -> HC_Utf8KeyResult {
    let result = hc_session_handle_key(session, request);
    let mut utf8_result = HC_Utf8KeyResult {
        composition_string: ptr::null(),
        length: 0,
        status_flag: result.state.status_flag,
        error_code: result.state.error_code,
        spell_check_status: result.state.spell_check_status,
        handled: result.handled,
    };

    UTF8_RESULT_BUFFER.with(|buffer| {
        let mut buffer = buffer.borrow_mut();
        state_to_utf8_into(&result.state, &mut buffer);
        utf8_result.composition_string = buffer.as_ptr() as *const c_char;
        utf8_result.length = buffer.len();
    });

    let mut state = result.state;
    hc_state_free(&mut state);
    utf8_result
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key_v4(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequestV2,
    result: *mut HC_KeyResultV2,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        if !result.is_null() {
            unsafe {
                *result = v4_error_result(HCErrorCode::NullPointer);
            }
        }
        return 0;
    }

    unsafe {
        let session = &mut *(session as *mut Session);
        let req = &*request;

        let is_han_nom = req.translation_target == TRANSLATION_TARGET_HAN_NOM;

        // Validated before use: `composition_method + 3` accepted anything the
        // caller sent, overflowed on i32::MAX (an abort in debug builds) and
        // otherwise resolved to a silent Telex session (FFI-03).
        let mode = match InputMode::from_composition_method(req.composition_method, is_han_nom) {
            Ok(mode) => mode,
            Err(err) => {
                *result = v4_error_result(err);
                return 0;
            }
        };

        if let Err(err) = check_key_text_len(req.text) {
            *result = v4_error_result(err);
            return 0;
        }

        let key_request = HC_KeyRequest {
            kind: req.kind,
            text: req.text,
            input_mode: mode as i32,
            legacy_tone: req.legacy_tone,
            spell_check: req.spell_check,
            auto_restore: req.auto_restore,
            quick_consonants: req.quick_consonants,
            english_protection: req.english_protection,
            macro_in_english: req.macro_in_english,
            esc_restore_raw: req.esc_restore_raw,
        };

        if is_han_nom {
            let translator = session.translator.as_mut().and_then(|t| {
                t.as_any_mut()
                    .downcast_mut::<crate::translation::HanNomTranslator>()
            });
            let composition = &mut session.composition;

            match translator {
                Some(t) => {
                    let mut nom_result = HC_HanNomResultV3 {
                        status_flag: HCStatusFlag::InProgress as i32,
                        error_code: HCErrorCode::None as i32,
                        reading: ptr::null(),
                        reading_len: 0,
                        candidates: ptr::null(),
                        candidate_count: 0,
                        total_candidate_count: 0,
                        page_size: 9,
                        truncated: 0,
                        handled: 0,
                    };
                    let handled =
                        t.handle_han_nom_key_v3(composition, &key_request, &mut nom_result);

                    // The translator hands back pointers into its own buffers,
                    // which `hc_session_reset` clears and `hc_session_free`
                    // drops. Copy them out so the caller's rule is the same one
                    // the Vietnamese branch below uses (FFI-05, FFI-06).
                    *result = V4_RESULT_BUFFER.with(|buffer| {
                        let mut buffer = buffer.borrow_mut();
                        buffer.set_text_from_borrowed(
                            nom_result.reading,
                            nom_result.reading_len as usize,
                        );
                        buffer.set_candidates(nom_result.candidates, nom_result.candidate_count);
                        HC_KeyResultV2 {
                            composition_string: buffer.text_ptr(),
                            composition_len: buffer.text.len(),
                            status_flag: nom_result.status_flag,
                            error_code: nom_result.error_code,
                            spell_check_status: HCSpellCheckStatus::Valid as i32,
                            handled: nom_result.handled,
                            candidates: buffer.candidates_ptr(),
                            candidate_count: buffer.entries.len() as u16,
                            total_candidate_count: nom_result.total_candidate_count,
                        }
                    });
                    handled
                }
                None => {
                    *result = v4_error_result(HCErrorCode::InvalidInputMode);
                    0
                }
            }
        } else {
            let key_result = session.handle_key(&key_request);

            V4_RESULT_BUFFER.with(|buffer| {
                let mut buffer = buffer.borrow_mut();
                buffer.set_text_from_state(&key_result.state);
                buffer.set_candidates(ptr::null(), 0);

                *result = HC_KeyResultV2 {
                    composition_string: buffer.text_ptr(),
                    composition_len: buffer.text.len(),
                    status_flag: key_result.state.status_flag,
                    error_code: key_result.state.error_code,
                    spell_check_status: key_result.state.spell_check_status,
                    handled: key_result.handled,
                    candidates: ptr::null(),
                    candidate_count: 0,
                    total_candidate_count: 0,
                };
            });

            let mut state = key_result.state;
            hc_state_free(&mut state);

            if (*result).handled != 0 {
                1
            } else {
                0
            }
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_compose_with_request(request: *const HC_ComposeRequest) -> HC_State {
    if request.is_null() {
        return hc_error_state(HCErrorCode::NullPointer);
    }

    let request = unsafe { &*request };
    let tone = match parse_tone(request.tone) {
        Ok(tone) => tone,
        Err(err) => return hc_error_state(err),
    };
    let onset = match required_cstr(request.onset) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let medial = match optional_cstr(request.medial) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let nucleus = match required_cstr(request.nucleus) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let coda = match optional_cstr(request.coda) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let trigger_case = match required_cstr(request.trigger_case) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };
    let raw_input = match required_cstr(request.raw_input) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };

    let mut text = format!(
        "{}{}{}{}",
        onset,
        medial.unwrap_or(""),
        nucleus,
        coda.unwrap_or("")
    );
    if tone != Tone::Flat {
        let _ = apply_tone_to_word(&mut text, tone, request.legacy_tone != 0);
    }

    let rendered = mirror_capitalization(trigger_case, &text);

    let lower = vowel::strip_marks_ascii_lower(raw_input);
    if is_known_english_word(&lower) {
        hc_state_from_string(raw_input, HCStatusFlag::EnglishFallback, HCErrorCode::None)
    } else {
        hc_state_from_string(&rendered, HCStatusFlag::Commit, HCErrorCode::None)
    }
}

#[no_mangle]
pub extern "C" fn hc_compose_from_parts(
    onset: *const c_char,
    medial: *const c_char,
    nucleus: *const c_char,
    coda: *const c_char,
    tone: i32,
    trigger_case: *const c_char,
    raw_input: *const c_char,
    legacy_tone: u8,
    boundary: i32,
) -> HC_State {
    let request = HC_ComposeRequest {
        onset,
        medial,
        nucleus,
        coda,
        tone,
        trigger_case,
        raw_input,
        legacy_tone,
        boundary,
    };
    hc_compose_with_request(&request)
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_rehydrate_with_request(request: *const HC_RehydrateRequest) -> HC_State {
    if request.is_null() {
        return hc_error_state(HCErrorCode::NullPointer);
    }

    let request = unsafe { &*request };
    let word = match required_cstr(request.committed_word) {
        Ok(value) => value,
        Err(err) => return hc_error_state(err),
    };

    let mode = match InputMode::try_from(request.input_mode) {
        Ok(mode) => mode,
        Err(_) => return hc_error_state(HCErrorCode::InvalidInputMode),
    };
    let trigger = match parse_edit_trigger(request.trigger_kind, request.trigger_value) {
        Ok(trigger) => trigger,
        Err(err) => return hc_error_state(err),
    };

    let edited = apply_edit_trigger_to_word(word, mode, trigger);
    hc_state_from_string(&edited, HCStatusFlag::ReconversionActive, HCErrorCode::None)
}

#[no_mangle]
pub extern "C" fn hc_rehydrate_apply(
    committed_word: *const c_char,
    input_mode: i32,
    trigger_kind: i32,
    trigger_value: i32,
) -> HC_State {
    let request = HC_RehydrateRequest {
        committed_word,
        input_mode,
        trigger_kind,
        trigger_value,
    };
    hc_rehydrate_with_request(&request)
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_state_free(state: *mut HC_State) {
    if state.is_null() {
        return;
    }
    unsafe {
        let state = &mut *state;
        if !state.composition_string.is_null() && state.length > 0 {
            let slice_ptr = std::ptr::slice_from_raw_parts_mut(
                state.composition_string as *mut u16,
                state.length,
            );
            drop(Box::from_raw(slice_ptr));
        }
        state.composition_string = ptr::null();
        state.length = 0;
        state.status_flag = HCStatusFlag::InProgress as i32;
        state.error_code = HCErrorCode::None as i32;
    }
}

fn key_text(ptr: *const c_char) -> Option<&'static str> {
    if ptr.is_null() {
        return None;
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    cstr.to_str().ok()
}

fn required_cstr(ptr: *const c_char) -> Result<&'static str, HCErrorCode> {
    if ptr.is_null() {
        return Err(HCErrorCode::MissingRequiredField);
    }
    optional_cstr(ptr)?.ok_or(HCErrorCode::MissingRequiredField)
}

fn optional_cstr(ptr: *const c_char) -> Result<Option<&'static str>, HCErrorCode> {
    if ptr.is_null() {
        return Ok(None);
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(Some)
        .map_err(|_| HCErrorCode::InvalidUtf8)
}

pub fn hc_state_from_string(text: &str, status: HCStatusFlag, error: HCErrorCode) -> HC_State {
    let utf16: Box<[u16]> = text.encode_utf16().collect::<Vec<_>>().into_boxed_slice();
    let length = utf16.len();
    let ptr = Box::into_raw(utf16) as *mut u16;
    HC_State {
        composition_string: ptr,
        length,
        status_flag: status as i32,
        error_code: error as i32,
        spell_check_status: HCSpellCheckStatus::Valid as i32,
    }
}

fn state_to_utf8_into(state: &HC_State, out: &mut String) {
    out.clear();
    if state.composition_string.is_null() || state.length == 0 {
        return;
    }
    out.reserve(state.length * 3);
    let data = unsafe { std::slice::from_raw_parts(state.composition_string, state.length) };
    let mut i = 0;
    while i < data.len() {
        let mut cp = data[i] as u32;
        if (0xD800..=0xDBFF).contains(&cp) {
            if i + 1 < data.len() {
                let low = data[i + 1] as u32;
                if (0xDC00..=0xDFFF).contains(&low) {
                    cp = 0x10000 + ((cp - 0xD800) << 10) + (low - 0xDC00);
                    i += 1;
                } else {
                    cp = 0xFFFD;
                }
            } else {
                cp = 0xFFFD;
            }
        } else if (0xDC00..=0xDFFF).contains(&cp) {
            cp = 0xFFFD;
        }
        out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
        i += 1;
    }
}

pub fn hc_state_from_string_with_spell_check(
    text: &str,
    status: HCStatusFlag,
    error: HCErrorCode,
    spell_check: HCSpellCheckStatus,
) -> HC_State {
    let utf16: Box<[u16]> = text.encode_utf16().collect::<Vec<_>>().into_boxed_slice();
    let length = utf16.len();
    let ptr = Box::into_raw(utf16) as *mut u16;
    HC_State {
        composition_string: ptr,
        length,
        status_flag: status as i32,
        error_code: error as i32,
        spell_check_status: spell_check as i32,
    }
}

fn hc_error_state(error: HCErrorCode) -> HC_State {
    HC_State {
        composition_string: ptr::null(),
        length: 0,
        status_flag: HCStatusFlag::InProgress as i32,
        error_code: error as i32,
        spell_check_status: HCSpellCheckStatus::Valid as i32,
    }
}

fn mirror_capitalization(trigger_case: &str, output: &str) -> String {
    let mut chars = trigger_case.chars();
    let first = chars.next();
    let second = chars.next();
    if first.is_some_and(char::is_uppercase) && second.is_some_and(char::is_uppercase) {
        output.to_uppercase()
    } else if first.is_some_and(char::is_uppercase) {
        let mut rendered = output.chars();
        match rendered.next() {
            Some(head) => {
                let mut result = head.to_uppercase().collect::<String>();
                result.push_str(rendered.as_str());
                result
            }
            None => String::new(),
        }
    } else {
        output.to_string()
    }
}

fn apply_edit_trigger_to_word(word: &str, mode: InputMode, trigger: EditTrigger) -> String {
    match trigger {
        EditTrigger::Cancel => strip_all_marks(word),
        EditTrigger::TelexW => {
            let mut clone = word.to_string();
            if apply_telex_w(&mut clone) {
                clone
            } else {
                word.to_string()
            }
        }
        EditTrigger::Tone => {
            let mut clone = word.to_string();
            if transform::apply_tone(&mut clone, Tone::Sac, false) {
                clone
            } else {
                word.to_string()
            }
        }
        EditTrigger::VniDiacritic => {
            let mut clone = word.to_string();
            let _ = match mode {
                InputMode::Telex | InputMode::HanNomTelex => apply_telex_w(&mut clone),
                InputMode::Vni | InputMode::HanNomVni => apply_circumflex(&mut clone),
                InputMode::Viqr | InputMode::HanNomViqr => apply_circumflex(&mut clone),
            };
            clone
        }
        EditTrigger::LiteralNumber => word.to_string(),
        EditTrigger::Escape => word.to_string(),
    }
}

// ── Hán Nôm FFI (delegates to HanNomTranslator) ──

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[deprecated(note = "Use hc_session_handle_key_v4")]
pub extern "C" fn hc_session_handle_key_hannom(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
    result: *mut HC_HanNomResult,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        if let Err(err) = check_key_text_len((*request).text) {
            (*result).error_code = err as i32;
            (*result).handled = 0;
            return 0;
        }
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.handle_han_nom_key(composition, &*request, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[deprecated(note = "Use hc_session_handle_key_v4")]
pub extern "C" fn hc_session_handle_key_hannom_v2(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
    result: *mut HC_HanNomResultV2,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        if let Err(err) = check_key_text_len((*request).text) {
            (*result).error_code = err as i32;
            (*result).handled = 0;
            return 0;
        }
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.handle_han_nom_key_v2(composition, &*request, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_select_hannom_candidate_v2(
    session: *mut std::ffi::c_void,
    index: u16,
    result: *mut HC_HanNomResultV2,
) -> i32 {
    if session.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.select_han_nom_candidate_v2(composition, index as usize, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_set_hannom_options(
    session: *mut std::ffi::c_void,
    options: *const HC_HanNomOptions,
) {
    if session.is_null() || options.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        if let Some(t) = session.translator_mut() {
            t.set_hannom_options(&*options);
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_handle_key_hannom_v3(
    session: *mut std::ffi::c_void,
    request: *const HC_KeyRequest,
    result: *mut HC_HanNomResultV3,
) -> i32 {
    if session.is_null() || request.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        if let Err(err) = check_key_text_len((*request).text) {
            (*result).error_code = err as i32;
            (*result).handled = 0;
            return 0;
        }
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.handle_han_nom_key_v3(composition, &*request, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_select_hannom_candidate_v3(
    session: *mut std::ffi::c_void,
    index: u16,
    result: *mut HC_HanNomResultV3,
) -> i32 {
    if session.is_null() || result.is_null() {
        return 0;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        let translator = session.translator.as_mut().and_then(|t| {
            t.as_any_mut()
                .downcast_mut::<crate::translation::HanNomTranslator>()
        });
        let composition = &mut session.composition;
        match translator {
            Some(t) => t.select_han_nom_candidate_v3(composition, index as usize, &mut *result),
            None => {
                (*result).error_code = HCErrorCode::InvalidInputMode as i32;
                0
            }
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_set_hannom_options_v2(
    session: *mut std::ffi::c_void,
    options: *const HC_HanNomOptionsV2,
) {
    if session.is_null() || options.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        if let Some(t) = session.translator_mut() {
            t.set_hannom_options_v2(&*options);
        }
    }
}

#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn hc_session_reset_hannom_learning(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        if let Some(t) = session.translator_mut() {
            t.reset_learning_data();
        }
    }
}

#[no_mangle]
pub extern "C" fn hc_session_flush_hannom_learning(session: *mut std::ffi::c_void) {
    if session.is_null() {
        return;
    }
    unsafe {
        let session = &mut *(session as *mut Session);
        if let Some(t) = session.translator_mut() {
            t.flush_hannom_learning();
        }
    }
}

#[no_mangle]
pub extern "C" fn hc_nom_dict_status(_session: *mut std::ffi::c_void) -> i32 {
    match han_nom::get_global_dict() {
        Ok(_) => 0,
        Err(err) => err as i32,
    }
}
