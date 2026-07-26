# ADR-001: Separate Composition from Translation

- **Status:** accepted
- **Date:** 2026-07-26
- **Context:**
  The `Session` struct has 39 fields, 17 of which are Han-Nom-only. The `InputMode` enum has 6 variants (3 pure Vietnamese + 3 Han Nom), requiring every match arm in `compose.rs` and `language.rs` to handle both `InputMode::Telex | InputMode::HanNomTelex` identically. The `CompositionMode::Inline | ::Dictionary` enum at `compose.rs:9-24` already provides a working seam — the codebase already distinguishes between direct composition and dictionary-based translation in the composition state machine. Extracting this seam into a structural boundary eliminates the combinatorial explosion of input mode × composition mode branching.
- **Decision:**
  Split into `CompositionEngine` (Telex/VNI/VIQR keystroke→text, ~22 fields, zero Han Nom knowledge) and a pluggable `TranslationEngine` (reading→glyph lookup). The `Session` becomes a thin coordinator: `composition: CompositionEngine` + `translator: Option<Box<dyn Translator>>`.
- **Consequences:**
  - Positive: Independently testable composition, zero Han Nom allocation in pure Vietnamese mode, clean extension point for future QuocNgu prediction.
  - Negative: Han Nom handlers that call `self.render_from_raw()` must be updated to `self.composition.render_from_raw()`.
  - Neutral: 16 bytes vtable overhead per session for `Box<dyn Translator>`.
- **Alternatives Considered:**
  - Keep monolithic Session with conditional compilation: Rejected — doesn't fix the architectural problem, still bloated, conditional compilation obscures the real data-flow.
  - Fork into two separate crates: Rejected — overkill for a ~22 field struct extraction; the boundary is clean enough without introducing crate-level dependency management overhead.
