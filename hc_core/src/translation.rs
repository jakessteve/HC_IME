use crate::compose;
use crate::composition::{render_raw_input, CompositionEngine};
use crate::han_nom::{
    get_global_dict, get_global_phrase_dict, get_global_phrase_history_path, load_user_phrase_dict,
    normalize_phrase_reading, PhraseEntry, PhraseHistory,
};
use crate::types::*;
use std::any::Any;
use std::ffi::c_char;
use std::path::PathBuf;
use std::ptr;

/// How many single-glyph candidates each side of a generated pair draws from.
/// The rank stride below must match it exactly.
pub(crate) const GENERATED_PAIR_WIDTH: usize = 9;

/// Upper bound on the raw-keystroke buffer behind a Hán Nôm reading.
///
/// The test was `raw_buffer.len() < 64` — measured *before* the append, so a
/// single oversized key text still landed in full and the buffer could reach
/// 127 bytes.
///
/// The unit is bytes, deliberately (NOM-17 asks whether it should be
/// characters). A reading is raw keystrokes, and every supported method —
/// Telex, VNI, VIQR — types them as ASCII, so for real input bytes and
/// characters are the same number. The sinks downstream are byte-bounded:
/// `HC_HanNomResult.reading` is a `[u8; 256]` that truncates at 255 bytes, and
/// the ABI bounds one key event at `HC_MAX_KEY_TEXT_BYTES` bytes. A 64-*char*
/// cap would admit 256 bytes of CJK and push a reading straight into that
/// truncation, which splits UTF-8 (FFI-10). Matching units keeps the two
/// bounds composable.
const MAX_READING_BYTES: usize = 64;

// ── Public trait types ──

#[derive(Debug, Clone)]
pub struct Candidate {
    pub text: String,
    pub reading: String,
    pub kind: CandidateKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    Exact,
    Prediction,
    Fallback,
    Single,
}

/// Where one published candidate's bytes live inside `ffi_candidate_arena`.
#[derive(Debug, Clone, Copy)]
struct PublishedSpan {
    /// (offset, byte length)
    text: (usize, usize),
    reading: (usize, usize),
    kind: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct NomTextCandidate {
    pub text: String,
    pub reading: String,
    pub kind: u8,
    pub system_rank: u32,
    pub source_tier: u8,
}

// ── Translator trait ──

pub trait Translator {
    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn lookup(&mut self, reading: &str) -> Vec<Candidate>;
    fn lookup_phrase(&mut self, first: &str, second: &str) -> Vec<Candidate>;
    fn select(&mut self, index: usize) -> Option<String>;
    fn record_selection(&mut self, reading: &str, glyphs: &str);
    fn flush(&mut self);
    fn reset_learning(&mut self);
}

// ── HanNomTranslator ──

/// FFI pointer lifetime contract:
/// - Pointers returned in HC_HanNomResultV3 (candidates, reading) are valid
///   until the next call that publishes a new candidate list.
/// - The underlying buffers (ffi_candidates_buf, ffi_phrase_candidates_buf,
///   ffi_candidate_arena) are owned by this struct and reused across calls.
/// - Candidate text and readings are *copied* into ffi_candidate_arena at
///   publish time. They deliberately do not borrow `phrase_candidates`, which
///   `reset()` drops on every commit, on every selection and on
///   `hc_session_reset` — none of which hand the caller a replacement.
pub(crate) struct HanNomTranslator {
    pub(crate) nom_phase: NomPhase,
    pub(crate) nom_candidates: Vec<char>,
    pub(crate) candidate_page: usize,
    pub(crate) reading_buffer: String,
    pub(crate) ffi_candidates_buf: Vec<HC_CandidateChar>,
    pub(crate) phrase_first: Option<String>,
    pub(crate) phrase_candidates: Vec<NomTextCandidate>,
    pub(crate) phrase_candidate_page: usize,
    pub(crate) ffi_phrase_candidates_buf: Vec<HC_HanNomCandidateText>,
    /// Owns every byte the published candidate pointers address. Rewritten only
    /// by `publish_candidates`, i.e. only when the caller is being handed a
    /// replacement list.
    pub(crate) ffi_candidate_arena: String,
    ffi_candidate_spans: Vec<PublishedSpan>,
    pub(crate) ffi_v2_output: String,
    pub(crate) phrase_prediction_enabled: bool,
    pub(crate) phrase_learning_enabled: bool,
    pub(crate) phrase_history_path: PathBuf,
    pub(crate) phrase_history: PhraseHistory,
    pub(crate) phrase_history_dirty: bool,
    pub(crate) phrase_history_loaded: bool,
    pub(crate) user_phrase_path: Option<PathBuf>,
    pub(crate) user_phrase_entries: Vec<PhraseEntry>,
}

impl HanNomTranslator {
    pub(crate) fn new() -> Self {
        Self {
            nom_phase: NomPhase::Reading,
            nom_candidates: Vec::new(),
            candidate_page: 0,
            reading_buffer: String::new(),
            ffi_candidates_buf: Vec::new(),
            phrase_first: None,
            phrase_candidates: Vec::new(),
            phrase_candidate_page: 0,
            ffi_phrase_candidates_buf: Vec::new(),
            ffi_candidate_arena: String::new(),
            ffi_candidate_spans: Vec::new(),
            ffi_v2_output: String::new(),
            phrase_prediction_enabled: true,
            phrase_learning_enabled: true,
            phrase_history_path: get_global_phrase_history_path(),
            phrase_history: PhraseHistory::default(),
            phrase_history_dirty: false,
            phrase_history_loaded: false,
            user_phrase_path: None,
            user_phrase_entries: Vec::new(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.nom_phase = NomPhase::Reading;
        self.nom_candidates.clear();
        self.candidate_page = 0;
        self.reading_buffer.clear();
        self.ffi_candidates_buf.clear();
        self.phrase_first = None;
        self.phrase_candidates.clear();
        self.phrase_candidate_page = 0;
        self.ffi_phrase_candidates_buf.clear();
        // `ffi_candidate_arena` is deliberately left alone. A reset publishes
        // nothing, so the caller has no replacement to move to and the bytes
        // its candidate pointers address must outlive this call (FFI-06).
    }

    fn full_reset(&mut self, composition: &mut CompositionEngine) {
        self.reset();
        composition.reset();
    }

    /// Loads the history file once per session, or once per genuine change of
    /// history path. Not per keystroke — see invariant 3.
    ///
    /// The default path used to be served from a process-wide `OnceLock`
    /// snapshot taken at process start, so a session never saw what another
    /// session had written, and on a machine where the file did not exist at
    /// start-up every session began empty and then flushed that emptiness over
    /// the real file (NOM-08).
    pub(crate) fn ensure_phrase_history_loaded(&mut self) {
        if !self.phrase_history_loaded {
            self.phrase_history = PhraseHistory::load(&self.phrase_history_path);
            self.phrase_history_loaded = true;
        }
    }

    // --- Options ---

    pub(crate) fn set_hannom_options(&mut self, options: &HC_HanNomOptions) {
        // Frontends re-apply the same options routinely (macOS calls
        // `configureHanNom()` on every controller init and on every settings
        // change). This used to clear `phrase_history_loaded` unconditionally
        // while leaving `phrase_history_dirty` set, so the next keystroke
        // reloaded a history that did not contain anything learned since the
        // last flush, and the next flush wrote that stale copy back over the
        // file — `{"entries":[]}` in the common case (NOM-08).
        //
        // Two rules keep that from happening: anything learned under the
        // current settings reaches disk before they change, and only a genuine
        // change of path invalidates the in-memory history.
        self.flush_hannom_learning();
        self.phrase_prediction_enabled = options.phrase_prediction != 0;
        self.phrase_learning_enabled = options.learning_enabled != 0;
        if !options.history_path.is_null() {
            if let Some(path) = key_text(options.history_path).filter(|path| !path.is_empty()) {
                let path = PathBuf::from(path);
                if path != self.phrase_history_path {
                    self.phrase_history_path = path;
                    self.phrase_history_loaded = false;
                }
            }
        }
    }

    pub(crate) fn set_hannom_options_v2(&mut self, options: &HC_HanNomOptionsV2) {
        self.set_hannom_options(&HC_HanNomOptions {
            phrase_prediction: options.phrase_prediction,
            learning_enabled: options.learning_enabled,
            history_path: options.history_path,
        });
        self.user_phrase_path = None;
        if !options.user_phrase_path.is_null() {
            if let Some(path) = key_text(options.user_phrase_path).filter(|path| !path.is_empty()) {
                self.user_phrase_path = Some(PathBuf::from(path));
            }
        }
        self.reload_user_phrase_entries();
    }

    pub(crate) fn reload_user_phrase_entries(&mut self) {
        self.user_phrase_entries.clear();
        if let Some(path) = self.user_phrase_path.as_deref() {
            self.user_phrase_entries = load_user_phrase_dict(path).0;
        }
    }

    // --- Phrase helpers ---

    fn phrase_reading(&self, composition: &CompositionEngine) -> String {
        match &self.phrase_first {
            Some(first) if composition.buffer.is_empty() => format!("{first} "),
            Some(first) => format!("{first} {}", composition.buffer),
            None => composition.buffer.clone(),
        }
    }

    /// Appends the single-glyph candidates for one syllable. Shared by the
    /// no-first-word path, the empty-phrase fallback and the punctuation commit
    /// so all three rank identically.
    fn push_single_glyph_candidates(&mut self, reading: &str, limit: usize) {
        let Ok(chars) = get_global_dict() else {
            return;
        };
        for (rank, ch) in chars.lookup(reading).into_iter().take(limit).enumerate() {
            self.phrase_candidates.push(NomTextCandidate {
                text: ch.to_string(),
                reading: reading.to_owned(),
                kind: 3,
                system_rank: rank as u32,
                source_tier: 1,
            });
        }
    }

    pub(crate) fn rebuild_phrase_candidates(&mut self, composition: &CompositionEngine) {
        #[cfg(test)]
        crate::han_nom::probe::note_phrase_rebuild();
        self.phrase_candidates.clear();
        let Ok(chars) = get_global_dict() else {
            return;
        };
        let full = self.phrase_reading(composition);
        if self.phrase_first.is_none() {
            let reading = composition.buffer.clone();
            self.push_single_glyph_candidates(&reading, usize::MAX);
            self.sort_phrase_candidates();
            return;
        }
        let Ok(phrases) = get_global_phrase_dict() else {
            return;
        };
        let normalized = normalize_phrase_reading(&full);
        let exact = !composition.buffer.is_empty();
        let entries = if exact {
            phrases.exact(&normalized)
        } else if self.phrase_prediction_enabled {
            phrases.prefix(&normalized)
        } else {
            Vec::new()
        };
        for entry in entries {
            self.phrase_candidates.push(NomTextCandidate {
                text: entry.glyphs,
                reading: entry.reading,
                kind: if exact { 0 } else { 1 },
                system_rank: entry.system_rank,
                source_tier: 1,
            });
        }
        for entry in &self.user_phrase_entries {
            if (exact && entry.reading == normalized)
                || (!exact
                    && self.phrase_prediction_enabled
                    && entry.reading.starts_with(&normalized))
            {
                self.phrase_candidates.push(NomTextCandidate {
                    text: entry.glyphs.clone(),
                    reading: entry.reading.clone(),
                    kind: if exact { 0 } else { 1 },
                    system_rank: entry.system_rank,
                    source_tier: 0,
                });
            }
        }
        if exact {
            let first = self.phrase_first.as_deref().unwrap_or_default();
            let left = chars.lookup(first);
            let right = chars.lookup(&composition.buffer);
            for (li, l) in left.into_iter().take(GENERATED_PAIR_WIDTH).enumerate() {
                for (ri, r) in right.iter().take(GENERATED_PAIR_WIDTH).enumerate() {
                    self.phrase_candidates.push(NomTextCandidate {
                        text: format!("{l}{r}"),
                        reading: normalized.clone(),
                        // Fallback, not Exact: these pairs are generated by
                        // crossing two single-glyph lookups and are not
                        // attested in any phrase dictionary. Labelling them 0
                        // told every frontend they were dictionary phrases.
                        kind: 2,
                        // Stride must equal the inner `take`, or ranks collide
                        // across rows (li=1,ri=0 tying with li=0,ri=3).
                        system_rank: (li * GENERATED_PAIR_WIDTH + ri) as u32,
                        source_tier: 2,
                    });
                }
            }
        }

        // Fallback: when phrase candidates are empty after space, offer single-char Nom lookup
        if self.phrase_candidates.is_empty() && self.phrase_first.is_some() {
            let first = self.phrase_first.clone().unwrap_or_default();
            self.push_single_glyph_candidates(&first, GENERATED_PAIR_WIDTH);
        }
        self.sort_phrase_candidates();
    }

    /// Replaces the candidate list with the single glyphs of the first word.
    ///
    /// Used when the phrase is committed with nothing typed for the second
    /// syllable, where every phrase candidate is a *prediction*.
    fn rebuild_first_word_candidates(&mut self) {
        #[cfg(test)]
        crate::han_nom::probe::note_phrase_rebuild();
        self.phrase_candidates.clear();
        let first = self.phrase_first.clone().unwrap_or_default();
        self.push_single_glyph_candidates(&first, usize::MAX);
        self.sort_phrase_candidates();
    }

    fn sort_phrase_candidates(&mut self) {
        self.ensure_phrase_history_loaded();
        // Decorate-sort-undecorate. The comparator used to call
        // `PhraseHistory::score` twice per comparison — O(n log n) linear scans
        // of a list bounded only by the 2,048-entry learning cap, which put a
        // 1,400-candidate keystroke at 11.2 ms (NOM-12). Scoring once per
        // candidate makes it O(n) scans.
        let learning = self.phrase_learning_enabled;
        let history = &self.phrase_history;
        let mut ranked: Vec<(u32, u64, NomTextCandidate)> =
            std::mem::take(&mut self.phrase_candidates)
                .into_iter()
                .map(|candidate| {
                    let (count, last_used) = if learning {
                        history.score(&candidate.reading, &candidate.text)
                    } else {
                        (0, 0)
                    };
                    (count, last_used, candidate)
                })
                .collect();
        ranked.sort_by(|(lc, lt, left), (rc, rt, right)| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| rc.cmp(lc))
                .then_with(|| rt.cmp(lt))
                .then_with(|| left.source_tier.cmp(&right.source_tier))
                .then_with(|| left.system_rank.cmp(&right.system_rank))
                .then_with(|| left.text.cmp(&right.text))
        });
        self.phrase_candidates = ranked
            .into_iter()
            .map(|(_, _, candidate)| candidate)
            .collect();
        // Not `dedup_by`: that only removes *adjacent* duplicates, and the
        // same glyph pair can arrive from both the phrase dictionary and the
        // generated cross product, sorting far apart once their kinds differ.
        // Retaining the first occurrence keeps the best-ranked one.
        let mut seen = std::collections::HashSet::with_capacity(self.phrase_candidates.len());
        self.phrase_candidates
            .retain(|candidate| seen.insert(candidate.text.clone()));
        if self.phrase_candidate_page * 9 >= self.phrase_candidates.len() {
            self.phrase_candidate_page = 0;
        }
    }

    fn populate_nom_result_v2(
        &mut self,
        composition: &CompositionEngine,
        result: &mut HC_HanNomResultV2,
        handled: u8,
    ) {
        self.rebuild_phrase_candidates(composition);
        self.reading_buffer = self.phrase_reading(composition);
        result.status_flag = HCStatusFlag::InProgress as i32;
        result.error_code = HCErrorCode::None as i32;
        result.reading = self.reading_buffer.as_ptr();
        result.reading_len = self.reading_buffer.len().min(u16::MAX as usize) as u16;
        result.handled = handled;
        let start = self.phrase_candidate_page * 9;
        let end = (start + 9).min(self.phrase_candidates.len());
        self.publish_candidates(start, end);
        result.candidates = self.published_candidates_ptr();
        result.candidate_count = self.ffi_phrase_candidates_buf.len() as u16;
    }

    /// Copies `phrase_candidates[start..end]` into the arena and rebuilds the
    /// FFI pointer table from it.
    ///
    /// Copying rather than borrowing is the point: the pointers handed to the
    /// caller must not die when `reset()` drops `phrase_candidates`, which
    /// happens on every commit, on every `hc_session_select_hannom_candidate_*`
    /// and on `hc_session_reset` — none of which publish a replacement the
    /// caller could move to (FFI-06).
    fn publish_candidates(&mut self, start: usize, end: usize) {
        self.ffi_candidate_arena.clear();
        self.ffi_candidate_spans.clear();
        for candidate in &self.phrase_candidates[start..end] {
            let text = (self.ffi_candidate_arena.len(), candidate.text.len());
            self.ffi_candidate_arena.push_str(&candidate.text);
            let reading = (self.ffi_candidate_arena.len(), candidate.reading.len());
            self.ffi_candidate_arena.push_str(&candidate.reading);
            self.ffi_candidate_spans.push(PublishedSpan {
                text,
                reading,
                kind: candidate.kind,
            });
        }
        // Second pass: the arena is complete, so it can no longer reallocate
        // and pointers into it are stable for the lifetime of this generation.
        let base = self.ffi_candidate_arena.as_ptr();
        self.ffi_phrase_candidates_buf.clear();
        self.ffi_phrase_candidates_buf
            .reserve(self.ffi_candidate_spans.len());
        for span in &self.ffi_candidate_spans {
            self.ffi_phrase_candidates_buf.push(HC_HanNomCandidateText {
                // SAFETY: both offsets index bytes just written to the arena.
                text: unsafe { base.add(span.text.0) },
                text_len: span.text.1.min(u16::MAX as usize) as u16,
                reading: unsafe { base.add(span.reading.0) },
                reading_len: span.reading.1.min(u16::MAX as usize) as u16,
                kind: span.kind,
            });
        }
    }

    fn published_candidates_ptr(&self) -> *const HC_HanNomCandidateText {
        if self.ffi_phrase_candidates_buf.is_empty() {
            ptr::null()
        } else {
            self.ffi_phrase_candidates_buf.as_ptr()
        }
    }

    fn populate_nom_result_v3(
        &mut self,
        composition: &CompositionEngine,
        result: &mut HC_HanNomResultV3,
        handled: u8,
    ) {
        const MAX_CANDIDATES: usize = 256;
        // No rebuild here. This is only ever reached from
        // `handle_han_nom_key_v3`, and every path in `handle_han_nom_key_v2`
        // that returns "handled, still composing" has already run
        // `populate_nom_result_v2`, which rebuilds and sets `reading_buffer`.
        // Rebuilding a second time and discarding the first result was ~50% of
        // an 882 µs Hán Nôm keystroke (PERF-04). `debug_assert` pins the
        // contract so a future v2 path that skips the populate is caught.
        debug_assert_eq!(
            self.reading_buffer,
            self.phrase_reading(composition),
            "populate_nom_result_v3 requires populate_nom_result_v2 to have run"
        );
        let total = self.phrase_candidates.len();
        self.publish_candidates(0, total.min(MAX_CANDIDATES));
        result.status_flag = HCStatusFlag::InProgress as i32;
        result.error_code = HCErrorCode::None as i32;
        result.reading = self.reading_buffer.as_ptr();
        result.reading_len = self.reading_buffer.len().min(u16::MAX as usize) as u16;
        result.candidates = self.published_candidates_ptr();
        result.candidate_count = self.ffi_phrase_candidates_buf.len() as u16;
        result.total_candidate_count = total.min(u16::MAX as usize) as u16;
        result.page_size = 9;
        result.truncated = (total > MAX_CANDIDATES) as u8;
        result.handled = handled;
    }

    fn commit_phrase_text(
        &mut self,
        composition: &mut CompositionEngine,
        text: String,
        _reading: String,
        result: &mut HC_HanNomResultV2,
    ) {
        self.ffi_v2_output = text;
        result.status_flag = HCStatusFlag::Commit as i32;
        result.error_code = HCErrorCode::None as i32;
        result.reading = self.ffi_v2_output.as_ptr();
        result.reading_len = self.ffi_v2_output.len().min(u16::MAX as usize) as u16;
        result.candidates = ptr::null();
        result.candidate_count = 0;
        result.handled = 1;
        self.full_reset(composition);
    }

    pub(crate) fn record_phrase_selection(&mut self, reading: &str, glyphs: &str) {
        self.ensure_phrase_history_loaded();
        if self.phrase_learning_enabled && !reading.is_empty() {
            self.phrase_history.record(reading, glyphs);
            self.phrase_history_dirty = true;
        }
    }

    pub(crate) fn flush_hannom_learning(&mut self) {
        if !self.phrase_history_dirty || !self.phrase_learning_enabled {
            return;
        }
        match self.phrase_history.persist(&self.phrase_history_path) {
            Ok(()) => self.phrase_history_dirty = false,
            Err(err) => {
                // Keep the dirty flag set so the next flush retries; the entries
                // are still in memory and a later flush (or session teardown) can
                // save them. A silent failure here is how NOM-01 stayed invisible.
                // `writeln!` rather than `eprintln!`: this runs under `extern "C"`
                // (hc_session_free / hc_session_flush_hannom_learning) and a panic
                // on a closed stderr must not cross the FFI boundary.
                use std::io::Write;
                let _ = writeln!(
                    std::io::stderr(),
                    "[HC_IME] failed to persist Hán Nôm learning to {}: {err}",
                    self.phrase_history_path.display()
                );
            }
        }
    }

    /// True while this engine owns some composition state — a reading, a raw
    /// buffer, or a first word waiting for its partner. When it is false the
    /// engine has nothing to act on and editing keys belong to the client.
    fn is_composing(&self, composition: &CompositionEngine) -> bool {
        !composition.buffer.is_empty()
            || !composition.raw_buffer.is_empty()
            || self.phrase_first.is_some()
    }

    // --- V2 key handler ---

    pub(crate) fn handle_han_nom_key_v2(
        &mut self,
        composition: &mut CompositionEngine,
        request: &HC_KeyRequest,
        result: &mut HC_HanNomResultV2,
    ) -> i32 {
        // The v2 result carries one nine-wide page, so `=`/`+`/`]` and `-`/`[`
        // are the only way a v2 client can reach the rest of the list.
        self.handle_han_nom_key_v2_inner(composition, request, result, true)
    }

    fn handle_han_nom_key_v2_inner(
        &mut self,
        composition: &mut CompositionEngine,
        request: &HC_KeyRequest,
        result: &mut HC_HanNomResultV2,
        core_paging: bool,
    ) -> i32 {
        self.ffi_v2_output.clear();
        *result = HC_HanNomResultV2 {
            status_flag: HCStatusFlag::InProgress as i32,
            error_code: HCErrorCode::None as i32,
            reading: ptr::null(),
            reading_len: 0,
            candidates: ptr::null(),
            candidate_count: 0,
            handled: 0,
        };
        composition.mode = match InputMode::try_from(request.input_mode) {
            Ok(value) => value,
            Err(_) => {
                result.error_code = HCErrorCode::InvalidInputMode as i32;
                return 0;
            }
        };
        let Some(kind) = key_kind(request.kind) else {
            return 0;
        };
        let text = key_text(request.text).unwrap_or("");
        match kind {
            HCKeyKind::Escape => {
                if !self.is_composing(composition) {
                    // Nothing to cancel, so the key is the application's:
                    // dismissing a dialog, leaving vim insert mode. Every path
                    // here used to return 1 and Escape never reached the app
                    // in Hán Nôm mode (NOM-06). Vietnamese and the Hán Nôm
                    // backspace path both fall through here.
                    result.handled = 0;
                    return 0;
                }
                if self.phrase_first.is_some() && composition.buffer.is_empty() {
                    self.phrase_first = None;
                    self.phrase_candidate_page = 0;
                } else {
                    self.full_reset(composition);
                }
                self.populate_nom_result_v2(composition, result, 1);
                1
            }
            HCKeyKind::Backspace => {
                self.phrase_candidate_page = 0;
                if composition.raw_buffer.is_empty() && self.phrase_first.is_some() {
                    composition.buffer = self.phrase_first.take().unwrap_or_default();
                    composition.raw_buffer = composition.buffer.clone();
                    composition.rendered_raw_len = composition.raw_buffer.len();
                } else if !composition.raw_buffer.is_empty() {
                    composition.raw_buffer.pop();
                    composition.render_from_raw();
                } else {
                    // Nothing is being composed, so there is nothing for this
                    // engine to delete. Reporting the key as handled swallowed
                    // it and left the client unable to erase already-committed
                    // text — the Vietnamese path returns unhandled here too.
                    result.handled = 0;
                    return 0;
                }
                self.populate_nom_result_v2(composition, result, 1);
                1
            }
            HCKeyKind::Space => {
                if composition.buffer.is_empty() && self.phrase_first.is_none() {
                    return 0;
                }
                if self.phrase_first.is_none() {
                    self.phrase_first = Some(normalize_phrase_reading(&composition.buffer));
                    self.phrase_candidate_page = 0;
                    composition.buffer.clear();
                    composition.raw_buffer.clear();
                    composition.rendered_raw_len = 0;
                    self.populate_nom_result_v2(composition, result, 1);
                    return 1;
                }
                self.populate_nom_result_v2(composition, result, 1);
                1
            }
            HCKeyKind::Enter => {
                if composition.buffer.is_empty() && self.phrase_first.is_none() {
                    return 0;
                }
                let raw = normalize_phrase_reading(&self.phrase_reading(composition));
                if self.phrase_first.is_some() && !composition.buffer.is_empty() {
                    self.rebuild_phrase_candidates(composition);
                    let output = self
                        .phrase_candidates
                        .first()
                        .map(|candidate| candidate.text.clone())
                        .unwrap_or_else(|| raw.clone());
                    if !self.phrase_candidates.is_empty() {
                        self.record_phrase_selection(&raw, &output);
                    }
                    self.commit_phrase_text(composition, output, raw, result);
                } else {
                    self.commit_phrase_text(composition, raw, String::new(), result);
                }
                1
            }
            HCKeyKind::Printable => {
                let ch = text.chars().next().unwrap_or('\0');
                if is_pager_key(ch) {
                    // These are ordinary characters — `1+1=2`, `a[0]` — and the
                    // engine may only take them when it has a candidate list to
                    // page. It used to take them unconditionally: on a fresh
                    // session all five returned handled with no state change,
                    // and on the shipping v3/v4 path they did nothing at all
                    // because `populate_nom_result_v3` ignores
                    // `phrase_candidate_page` and always starts at index 0
                    // (NOM-07). v3 hands the frontend the whole list and every
                    // frontend pages in its own candidate UI, so core paging is
                    // v2-only.
                    if !core_paging || !self.is_composing(composition) {
                        result.handled = 0;
                        return 0;
                    }
                    self.rebuild_phrase_candidates(composition);
                    if matches!(ch, '=' | ']' | '+') {
                        if (self.phrase_candidate_page + 1) * 9 < self.phrase_candidates.len() {
                            self.phrase_candidate_page += 1;
                        }
                    } else {
                        self.phrase_candidate_page = self.phrase_candidate_page.saturating_sub(1);
                    }
                    self.populate_nom_result_v2(composition, result, 1);
                    return 1;
                }
                self.phrase_candidate_page = 0;
                if is_nom_punctuation(ch) && self.phrase_first.is_some() {
                    let reading = normalize_phrase_reading(&self.phrase_reading(composition));
                    if composition.buffer.is_empty() {
                        // Nothing was typed for the second syllable, so every
                        // phrase candidate is a *prediction*. Committing the
                        // top one inserted a whole word the user never typed —
                        // `hai` Space `(` produced `𠄩𤖸(` — and learned it as
                        // a selection (NOM-09). Only the first word is real.
                        self.rebuild_first_word_candidates();
                    } else {
                        self.rebuild_phrase_candidates(composition);
                    }
                    let output = self
                        .phrase_candidates
                        .first()
                        .map(|candidate| candidate.text.clone())
                        .unwrap_or(reading.clone());
                    if !self.phrase_candidates.is_empty() {
                        self.record_phrase_selection(&reading, &output);
                    }
                    self.commit_phrase_text(composition, format!("{output}{ch}"), reading, result);
                    return 1;
                }
                if matches!(composition.mode, InputMode::HanNomVni | InputMode::Vni)
                    && ch.is_ascii_digit()
                {
                    if composition.raw_buffer.is_empty() {
                        result.handled = 0;
                        return 0;
                    }
                    compose::TypingEngine::apply_vni_trigger(
                        &mut composition.buffer,
                        ch,
                        composition.legacy_tone,
                    );
                    composition.raw_buffer.push(ch);
                    self.populate_nom_result_v2(composition, result, 1);
                    return 1;
                }
                if ch.is_ascii_digit() {
                    result.handled = 0;
                    return 0;
                }
                if composition.raw_buffer.len() + text.len() <= MAX_READING_BYTES {
                    composition.raw_buffer.push_str(text);
                    composition.render_from_raw();
                }
                self.populate_nom_result_v2(composition, result, 1);
                1
            }
            _ => 0,
        }
    }

    // --- V2 select ---

    pub(crate) fn select_han_nom_candidate_v2(
        &mut self,
        composition: &mut CompositionEngine,
        index: usize,
        result: &mut HC_HanNomResultV2,
    ) -> i32 {
        self.rebuild_phrase_candidates(composition);
        let index = self.phrase_candidate_page * 9 + index;
        let Some(candidate) = self.phrase_candidates.get(index).cloned() else {
            return 0;
        };
        self.record_phrase_selection(&candidate.reading, &candidate.text);
        self.commit_phrase_text(composition, candidate.text, candidate.reading, result);
        1
    }

    // --- V3 key handler (wraps V2) ---

    pub(crate) fn handle_han_nom_key_v3(
        &mut self,
        composition: &mut CompositionEngine,
        request: &HC_KeyRequest,
        result: &mut HC_HanNomResultV3,
    ) -> i32 {
        *result = HC_HanNomResultV3 {
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
        let mut v2 = HC_HanNomResultV2 {
            status_flag: 0,
            error_code: 0,
            reading: ptr::null(),
            reading_len: 0,
            candidates: ptr::null(),
            candidate_count: 0,
            handled: 0,
        };
        let handled = self.handle_han_nom_key_v2_inner(composition, request, &mut v2, false);
        if handled == 0 {
            return 0;
        }
        if v2.status_flag == HCStatusFlag::Commit as i32 {
            result.status_flag = v2.status_flag;
            result.error_code = v2.error_code;
            result.reading = v2.reading;
            result.reading_len = v2.reading_len;
            result.handled = v2.handled;
        } else {
            self.populate_nom_result_v3(composition, result, v2.handled);
        }
        handled
    }

    // --- V3 select ---

    pub(crate) fn select_han_nom_candidate_v3(
        &mut self,
        composition: &mut CompositionEngine,
        index: usize,
        result: &mut HC_HanNomResultV3,
    ) -> i32 {
        self.rebuild_phrase_candidates(composition);
        let Some(candidate) = self.phrase_candidates.get(index).cloned() else {
            return 0;
        };
        self.record_phrase_selection(&candidate.reading, &candidate.text);
        self.ffi_v2_output = candidate.text;
        result.status_flag = HCStatusFlag::Commit as i32;
        result.error_code = HCErrorCode::None as i32;
        result.reading = self.ffi_v2_output.as_ptr();
        result.reading_len = self.ffi_v2_output.len().min(u16::MAX as usize) as u16;
        result.candidates = ptr::null();
        result.candidate_count = 0;
        result.total_candidate_count = 0;
        result.page_size = 9;
        result.truncated = 0;
        result.handled = 1;
        self.full_reset(composition);
        1
    }

    // --- V1 key handler ---

    pub(crate) fn handle_han_nom_key(
        &mut self,
        composition: &mut CompositionEngine,
        request: &HC_KeyRequest,
        result: &mut HC_HanNomResult,
    ) -> i32 {
        result.status_flag = HCStatusFlag::InProgress as i32;
        result.error_code = HCErrorCode::None as i32;
        result.handled = 0;
        result.reading_len = 0;
        result.candidate_count = 0;
        result.page = 0;
        result.total_candidates = 0;
        result.has_more = 0;
        result.candidates = ptr::null();

        let mode = match InputMode::try_from(request.input_mode) {
            Ok(m) => m,
            Err(_) => {
                result.error_code = HCErrorCode::InvalidInputMode as i32;
                return 0;
            }
        };
        composition.mode = mode;

        let dict = match get_global_dict() {
            Ok(d) => d,
            Err(err) => {
                result.error_code = err as i32;
                return 0;
            }
        };

        let kind = match key_kind(request.kind) {
            Some(k) => k,
            None => return 0,
        };

        let text = key_text(request.text).unwrap_or("");

        if self.nom_phase == NomPhase::Candidate {
            match kind {
                HCKeyKind::Space => {
                    if !self.nom_candidates.is_empty() {
                        let idx = self.candidate_page * 9;
                        if idx < self.nom_candidates.len() {
                            let selected_char = self.nom_candidates[idx];
                            self.commit_nom_char(composition, selected_char, result);
                            return 1;
                        }
                    }
                    self.full_reset(composition);
                    result.handled = 1;
                    return 1;
                }
                HCKeyKind::Enter => {
                    let commit_str = composition.buffer.clone();
                    self.full_reset(composition);
                    result.status_flag = HCStatusFlag::Commit as i32;
                    let bytes = commit_str.as_bytes();
                    let len = bytes.len().min(255);
                    result.reading[..len].copy_from_slice(&bytes[..len]);
                    result.reading_len = len as u16;
                    result.handled = 1;
                    return 1;
                }
                HCKeyKind::Escape => {
                    self.nom_phase = NomPhase::Reading;
                    self.populate_nom_result(composition, result, 1);
                    return 1;
                }
                HCKeyKind::Backspace => {
                    self.nom_phase = NomPhase::Reading;
                    if !composition.raw_buffer.is_empty() {
                        composition.raw_buffer.pop();
                        composition.render_from_raw();
                    }
                    self.populate_nom_result(composition, result, 1);
                    return 1;
                }
                HCKeyKind::Printable => {
                    let first_ch = text.chars().next().unwrap_or('\0');
                    if first_ch.is_ascii_digit() && first_ch != '0' {
                        let digit_val = first_ch.to_digit(10).unwrap() as usize;
                        let idx = self.candidate_page * 9 + (digit_val - 1);
                        if idx < self.nom_candidates.len() {
                            let selected_char = self.nom_candidates[idx];
                            self.commit_nom_char(composition, selected_char, result);
                            return 1;
                        }
                        self.populate_nom_result(composition, result, 1);
                        return 1;
                    }
                    if first_ch == '=' || first_ch == ']' || first_ch == '+' {
                        if (self.candidate_page + 1) * 9 < self.nom_candidates.len() {
                            self.candidate_page += 1;
                        }
                        self.populate_nom_result(composition, result, 1);
                        return 1;
                    }
                    if first_ch == '-' || first_ch == '[' {
                        if self.candidate_page > 0 {
                            self.candidate_page -= 1;
                        }
                        self.populate_nom_result(composition, result, 1);
                        return 1;
                    }
                    if is_nom_punctuation(first_ch) {
                        let mut output = String::new();
                        let idx = self.candidate_page * 9;
                        if idx < self.nom_candidates.len() {
                            output.push(self.nom_candidates[idx]);
                        } else {
                            output.push_str(&composition.buffer);
                        }
                        output.push(first_ch);

                        result.status_flag = HCStatusFlag::Commit as i32;
                        let bytes = output.as_bytes();
                        let len = bytes.len().min(255);
                        result.reading[..len].copy_from_slice(&bytes[..len]);
                        result.reading_len = len as u16;
                        result.handled = 1;
                        self.full_reset(composition);
                        return 1;
                    }
                    self.nom_phase = NomPhase::Reading;
                }
                _ => {
                    self.nom_phase = NomPhase::Reading;
                }
            }
        }

        match kind {
            HCKeyKind::Escape => {
                self.full_reset(composition);
                self.populate_nom_result(composition, result, 1);
                1
            }
            HCKeyKind::Backspace => {
                if !composition.raw_buffer.is_empty() {
                    match composition.mode {
                        InputMode::Vni | InputMode::HanNomVni => {
                            composition.raw_buffer = vni_raw_after_visible_backspace(
                                &composition.raw_buffer,
                                &composition.buffer,
                                composition.legacy_tone,
                            );
                            composition.render_from_raw();
                        }
                        _ => {
                            composition.raw_buffer.pop();
                            composition.render_from_raw();
                        }
                    }
                }
                self.populate_nom_result(composition, result, 1);
                1
            }
            HCKeyKind::Space | HCKeyKind::Enter => {
                if composition.buffer.is_empty() {
                    result.handled = 0;
                    0
                } else {
                    let candidates = dict.lookup(&composition.buffer);
                    if !candidates.is_empty() {
                        self.nom_candidates = candidates;
                        self.nom_phase = NomPhase::Candidate;
                        self.candidate_page = 0;
                        self.reading_buffer = composition.buffer.clone();
                        self.populate_nom_result(composition, result, 1);
                        1
                    } else {
                        let commit_str = composition.buffer.clone();
                        self.full_reset(composition);
                        result.status_flag = HCStatusFlag::Commit as i32;
                        let bytes = commit_str.as_bytes();
                        let len = bytes.len().min(255);
                        result.reading[..len].copy_from_slice(&bytes[..len]);
                        result.reading_len = len as u16;
                        result.handled = 1;
                        1
                    }
                }
            }
            HCKeyKind::Printable => {
                let first_ch = text.chars().next().unwrap_or('\0');
                match composition.mode {
                    InputMode::HanNomVni | InputMode::Vni => {
                        if first_ch.is_ascii_digit() {
                            if composition.raw_buffer.is_empty() {
                                result.handled = 0;
                                return 0;
                            }
                            compose::TypingEngine::apply_vni_trigger(
                                &mut composition.buffer,
                                first_ch,
                                composition.legacy_tone,
                            );
                            composition.raw_buffer.push(first_ch);
                            self.populate_nom_result(composition, result, 1);
                            return 1;
                        }
                    }
                    _ => {
                        if first_ch.is_ascii_digit() {
                            result.handled = 0;
                            return 0;
                        }
                    }
                }

                if composition.raw_buffer.len() + text.len() <= MAX_READING_BYTES {
                    composition.raw_buffer.push_str(text);
                    composition.render_from_raw();
                }
                self.populate_nom_result(composition, result, 1);
                1
            }
            _ => {
                result.handled = 0;
                0
            }
        }
    }

    fn commit_nom_char(
        &mut self,
        composition: &mut CompositionEngine,
        ch: char,
        result: &mut HC_HanNomResult,
    ) {
        let mut utf8_bytes = [0u8; 5];
        let s = ch.to_string();
        let bytes = s.as_bytes();
        let len = bytes.len().min(4);
        utf8_bytes[..len].copy_from_slice(&bytes[..len]);

        result.status_flag = HCStatusFlag::Commit as i32;
        result.reading[..len].copy_from_slice(&bytes[..len]);
        result.reading_len = len as u16;
        result.handled = 1;

        self.full_reset(composition);
    }

    fn populate_nom_result(
        &mut self,
        composition: &CompositionEngine,
        result: &mut HC_HanNomResult,
        handled: u8,
    ) {
        result.handled = handled;
        let r_bytes = composition.buffer.as_bytes();
        let r_len = r_bytes.len().min(255);
        result.reading[..r_len].copy_from_slice(&r_bytes[..r_len]);
        result.reading_len = r_len as u16;

        if self.nom_phase == NomPhase::Reading && !composition.buffer.is_empty() {
            if let Ok(dict) = get_global_dict() {
                self.nom_candidates = dict.lookup(&composition.buffer);
            }
        }

        if !self.nom_candidates.is_empty() && !composition.buffer.is_empty() {
            let start = self.candidate_page * 9;
            let end = (start + 9).min(self.nom_candidates.len());
            let page_candidates = &self.nom_candidates[start..end];

            self.ffi_candidates_buf.clear();
            for &ch in page_candidates {
                let mut candidate_char = HC_CandidateChar {
                    utf8: [0u8; 5],
                    byte_len: 0,
                };
                let s = ch.to_string();
                let b = s.as_bytes();
                let b_len = b.len().min(4);
                candidate_char.utf8[..b_len].copy_from_slice(&b[..b_len]);
                candidate_char.byte_len = b_len as u8;
                self.ffi_candidates_buf.push(candidate_char);
            }

            result.candidates = self.ffi_candidates_buf.as_ptr();
            result.candidate_count = self.ffi_candidates_buf.len() as u16;
            result.page = self.candidate_page as u16;
            result.total_candidates = self.nom_candidates.len() as u16;
            result.has_more = if end < self.nom_candidates.len() {
                1
            } else {
                0
            };
        } else {
            result.candidates = ptr::null();
            result.candidate_count = 0;
            result.page = 0;
            result.total_candidates = 0;
            result.has_more = 0;
        }
    }

    // --- Phrase history management ---

    pub(crate) fn reset_learning_data(&mut self) {
        self.phrase_history.reset(&self.phrase_history_path);
    }
}

// ── Translator trait impl ──

impl Translator for HanNomTranslator {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn lookup(&mut self, reading: &str) -> Vec<Candidate> {
        let Ok(dict) = get_global_dict() else {
            return Vec::new();
        };
        dict.lookup(reading)
            .into_iter()
            .map(|ch| Candidate {
                text: ch.to_string(),
                reading: reading.to_string(),
                kind: CandidateKind::Exact,
            })
            .collect()
    }

    fn lookup_phrase(&mut self, first: &str, second: &str) -> Vec<Candidate> {
        let combined = format!("{first} {second}");
        let normalized = normalize_phrase_reading(&combined);
        let Ok(phrases) = get_global_phrase_dict() else {
            return Vec::new();
        };
        phrases
            .exact(&normalized)
            .into_iter()
            .map(|entry| Candidate {
                text: entry.glyphs,
                reading: entry.reading,
                kind: CandidateKind::Exact,
            })
            .collect()
    }

    fn select(&mut self, index: usize) -> Option<String> {
        let candidate = self.phrase_candidates.get(index)?;
        let reading = candidate.reading.clone();
        let glyphs = candidate.text.clone();
        self.record_phrase_selection(&reading, &glyphs);
        Some(glyphs)
    }

    fn record_selection(&mut self, reading: &str, glyphs: &str) {
        self.record_phrase_selection(reading, glyphs);
    }

    fn flush(&mut self) {
        self.flush_hannom_learning();
    }

    fn reset_learning(&mut self) {
        self.phrase_history.reset(&self.phrase_history_path);
        self.phrase_history_dirty = false;
    }
}

// ── Free helper functions ──

fn key_text(ptr: *const c_char) -> Option<&'static str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { std::ffi::CStr::from_ptr(ptr).to_str().ok() }
}

/// Candidate-page navigation on the v2 result. `-` and `]` normally reach the
/// core as `HCKeyKind::Boundary`, not `Printable`; they are listed for the
/// clients that do send them as text.
fn is_pager_key(ch: char) -> bool {
    matches!(ch, '=' | ']' | '+' | '-' | '[')
}

fn is_nom_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '.' | ','
            | '!'
            | '?'
            | ';'
            | ':'
            | '('
            | ')'
            | '"'
            | '\''
            | '/'
            | '\\'
            | '@'
            | '#'
            | '$'
            | '%'
            | '^'
            | '&'
            | '*'
            | '~'
    )
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

    (render_raw_input(&candidate, InputMode::Vni, legacy_tone) == target).then_some(candidate)
}
