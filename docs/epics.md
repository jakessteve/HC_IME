# HC_IME Refactoring — Epic Specifications

This document defines the epic-level scope, dependencies, and validation gates
for the HC_IME refactoring project. Each epic decomposes into user stories
documented in [`stories.md`](stories.md).

---

## Epic 0: Platform Abstraction Layer

**Epic ID:** E0
**Dependencies:** None (new module)

### Summary

Introduce `hc_core/src/platform.rs` backed by the `dirs` crate for cross-platform
path resolution. Replace all manual `XDG_*` / `HOME` environment variable
construction in `han_nom.rs`, `language.rs`, and any other modules with
`platform::*()` calls. Add `#[cfg(unix)]` guards for file permission operations.

### User Stories

- **US-0.1** — Add `dirs` dependency and `platform.rs` module
- **US-0.2** — Migrate `default_history_path()` in `han_nom.rs`
- **US-0.3** — Migrate dictionary paths in `language.rs`
- **US-0.4** — Add cross-platform `#[cfg]` guards for file permission operations

### Dependencies

- None. This is a greenfield module that other epics consume.

### Validation Gate

- `cargo test --manifest-path hc_core/Cargo.toml` passes all existing tests.
- `scripts/e2e-smoke.sh` passes end-to-end gate.
- Path resolution produces identical results to manual `XDG_*` construction on Linux.
- `platform::data_dir()` returns the correct XDG data directory with existing env vars.

### Estimated File Impact

| File | Action |
| --- | --- |
| `hc_core/Cargo.toml` | Modify (add `dirs` dependency) |
| `hc_core/src/platform.rs` | Create |
| `hc_core/src/lib.rs` | Modify (declare `mod platform`) |
| `hc_core/src/han_nom.rs` | Modify (migrate path construction) |
| `hc_core/src/language.rs` | Modify (migrate path construction) |

---

## Epic 1: Fix UX Behavioral Bugs

**Epic ID:** E1
**Dependencies:** None (can run in parallel with E0)

### Summary

Fix five behavioral bugs affecting Vietnamese composition: Telex double-tap
toggle-off incorrectly swallowing input, VNI diacritic toggle consuming digits,
synchronous PhraseHistory load causing OS freeze, Fcitx5 menu ghosting the input
panel, and candidate layout issues. Add targeted regression tests.

### User Stories

- **US-1.1** — Fix Telex double-tap toggle-off
- **US-1.2** — Fix VNI diacritic toggle functions
- **US-1.3** — Defer PhraseHistory load to first Han Nom keystroke
- **US-1.4** — Fix Fcitx5 menu ghosting
- **US-1.5** — Fix candidate layout
- **US-1.6** — Add state drift check in `applySurroundingTextPreedit`
- **US-1.7** — Add regression tests for double-tap toggle-off and VNI toggle literal emission

### Dependencies

- None. Pure behavioral fixes in existing code.

### Validation Gate

- New regression tests pass.
- `aaa` → `aa` (not `âa`).
- `a66` → `a6` (digit emitted, not consumed).
- `ddd` → `dd`.
- `o77` → `o7`.
- Menu switch does not ghost the input panel.
- PhraseHistory does not load on session init (only on first Han Nom keystroke).

### Estimated File Impact

| File | Action |
| --- | --- |
| `hc_core/src/compose.rs` | Modify (Telex double-tap logic) |
| `hc_core/src/transform.rs` | Modify (diacritic toggle logic) |
| `hc_core/src/han_nom.rs` | Modify (defer phrase history load) |
| `hc_core/src/session.rs` | Modify (deferred load guard) |
| `hc_core/src/tests.rs` | Modify (add regression tests) |
| `linux_fcitx5/hcime.cpp` | Modify (menu ghosting, candidate layout) |

---

## Epic 2: Split Tests for Regression Safety

**Epic ID:** E2
**Dependencies:** Epic 0, Epic 1 (should be based on fixed code)

### Summary

Split the 3,561-line monolithic `tests.rs` into per-concern test modules under
`hc_core/tests/`. Make `test_helpers.rs` accessible to all split modules via
`pub(crate)` visibility. Add integration tests for the Han Nom composition path.

### User Stories

- **US-2.1** — Make `test_helpers.rs` accessible to split modules
- **US-2.2** — Extract composition tests into `tests/composition_tests.rs`
- **US-2.3** — Extract translation/Han Nom tests into `tests/translation_tests.rs`
- **US-2.4** — Extract session integration tests into `tests/session_tests.rs`
- **US-2.5** — Extract FFI boundary tests into `tests/ffi_tests.rs`
- **US-2.6** — Add Han Nom composition-path integration tests

### Dependencies

- Epic 0 (platform paths may affect test behavior).
- Epic 1 (tests should validate fixed behavior).

### Validation Gate

- `cargo test --manifest-path hc_core/Cargo.toml` passes all tests with same count + new additions.
- `cargo test --test composition_tests` runs only composition tests.
- `cargo test --test translation_tests` runs only translation/Han Nom tests.
- `cargo test --test session_tests` runs only session integration tests.
- `cargo test --test ffi_tests` runs only FFI boundary tests.

### Estimated File Impact

| File | Action |
| --- | --- |
| `hc_core/src/tests.rs` | Remove (split into modules) |
| `hc_core/src/test_helpers.rs` | Modify (add `pub(crate)` visibility) |
| `hc_core/src/tests/composition_tests.rs` | Create |
| `hc_core/src/tests/translation_tests.rs` | Create |
| `hc_core/src/tests/session_tests.rs` | Create |
| `hc_core/src/tests/ffi_tests.rs` | Create |

---

## Epic 3: Extract CompositionEngine

**Epic ID:** E3
**Dependencies:** Epic 2 (tests must be in place first)

### Summary

Extract the ~22 pure-Vietnamese composition fields from `Session` into a
standalone `CompositionEngine` struct in `hc_core/src/composition.rs`. Session
delegates to `self.composition.method()` for all composition operations.
Zero behavioral change — every existing test must pass identically.

### User Stories

- **US-3.1** — Define `CompositionEngine` struct
- **US-3.2** — Move composition methods to `CompositionEngine`
- **US-3.3** — Update `Session` to hold and delegate to `CompositionEngine`
- **US-3.4** — Update Han Nom handlers to use `self.composition.*`
- **US-3.5** — Update `lib.rs` FFI functions for new field paths

### Dependencies

- Epic 2 (split tests must be in place to catch regressions).

### Validation Gate

- All existing tests pass with identical results.
- `cargo test --test composition_tests` passes.
- Pure Vietnamese typing unaffected.
- Han Nom typing unaffected.

### Estimated File Impact

| File | Action |
| --- | --- |
| `hc_core/src/composition.rs` | Create |
| `hc_core/src/session.rs` | Modify (extract fields, delegate calls) |
| `hc_core/src/lib.rs` | Modify (declare `mod composition`, update FFI paths) |
| `hc_core/src/han_nom.rs` | Modify (update `self.composition.*` paths) |
| `hc_core/tests/composition_tests.rs` | Modify (update imports if needed) |

---

## Epic 4: Decouple Dictionary from Binary

**Epic ID:** E4
**Dependencies:** Epic 0 (platform paths needed)

### Summary

Add runtime file loading for Han Nom dictionaries using `memmap2` for zero-copy
access. Extend `platform::dictionary_paths()` to search file paths before falling
back to `include_bytes!` embedded data. Update CMake to install `.bin` files
alongside the shared library.

### User Stories

- **US-4.1** — Add `memmap2` dependency to `Cargo.toml`
- **US-4.2** — Extend `get_global_dict()` with file-first loading
- **US-4.3** — Add `memmap2`-based zero-copy loading
- **US-4.4** — Extend `get_global_phrase_dict()` with same pattern
- **US-4.5** — Update CMake to install `.bin` files

### Dependencies

- Epic 0 (`platform::dictionary_paths()` must exist).

### Validation Gate

- `scripts/e2e-smoke.sh` passes (dict regen check).
- Dictionary loads from file when `HC_IME_NOM_DICT` environment variable is set.
- Falls back to embedded data when no file is found.
- `memmap2` zero-copy load works on Linux.

### Estimated File Impact

| File | Action |
| --- | --- |
| `hc_core/Cargo.toml` | Modify (add `memmap2`) |
| `hc_core/src/han_nom.rs` | Modify (file-first dict loading) |
| `hc_core/src/platform.rs` | Modify (add `dictionary_paths()`) |
| `linux_fcitx5/CMakeLists.txt` | Modify (install `.bin` files) |

---

## Epic 5: TranslationEngine + Translator Trait

**Epic ID:** E5
**Dependencies:** Epic 3 (CompositionEngine extracted), Epic 4 (dict loading)

### Summary

Define a `Translator` trait with `lookup`, `lookup_phrase`, `select`, and
`record_selection` methods. Implement `HanNomTranslator` as a concrete
implementation. Make `Session` hold `Option<Box<dyn Translator>>` so that
pure Vietnamese mode allocates zero Han Nom state.

### User Stories

- **US-5.1** — Define `Translator` trait
- **US-5.2** — Create `hc_core/src/translation.rs` module
- **US-5.3** — Implement `HanNomTranslator` struct
- **US-5.4** — Move Han Nom fields from Session into `HanNomTranslator`
- **US-5.5** — Update Session to hold `Option<Box<dyn Translator>>`
- **US-5.6** — Add FFI pointer lifetime contract doc comment
- **US-5.7** — Update all Han Nom FFI functions

### Dependencies

- Epic 3 (CompositionEngine extracted; Han Nom handlers use `self.composition.*`).
- Epic 4 (dictionary loading refactored; `HanNomTranslator` can use it).

### Validation Gate

- All existing tests pass.
- Han Nom candidate lookup works.
- Phrase prediction works.
- Pure Vietnamese mode allocates zero Han Nom heap allocation (verified by test).

### Estimated File Impact

| File | Action |
| --- | --- |
| `hc_core/src/translation.rs` | Create |
| `hc_core/src/session.rs` | Modify (replace inline fields with `Option<Box<dyn Translator>>`) |
| `hc_core/src/han_nom.rs` | Modify (move logic to `translation.rs`) |
| `hc_core/src/lib.rs` | Modify (declare `mod translation`, update FFI) |
| `hc_core/tests/translation_tests.rs` | Modify (update for new module) |

---

## Epic 6: Simplify FFI Surface

**Epic ID:** E6
**Dependencies:** Epic 5 (TranslationEngine in place)

### Summary

Add `HC_KeyRequestV2` with a `translation_target` field and a unified
`hc_session_handle_key_v4()` FFI function that routes internally based on
`translation_target`. Migrate `hcime.cpp` to the unified entry point. Mark
v1 and v2 Han Nom FFI functions `#[deprecated]` using an additive protocol:
add new, migrate consumer, deprecate old.

### User Stories

- **US-6.1** — Add `HC_KeyRequestV2` struct
- **US-6.2** — Add `HC_KeyResultV2` unified result type
- **US-6.3** — Add `hc_session_handle_key_v4()` FFI function
- **US-6.4** — Migrate `hcime.cpp` to unified v4
- **US-6.5** — Mark v1 and v2 Han Nom FFI `#[deprecated]`
- **US-6.6** — Update `e2e-smoke.sh` ABI checks
- **US-6.7** — Verify v3 remains working

### Dependencies

- Epic 5 (`Translator` trait and `HanNomTranslator` must be in place for v4 routing).

### Validation Gate

- `scripts/e2e-smoke.sh` ABI checks pass.
- `hcime.cpp` uses the new unified function.
- v1/v2 marked deprecated but still callable.
- v3 wraps v2 internally and passes through the translator.

### Estimated File Impact

| File | Action |
| --- | --- |
| `hc_core/src/lib.rs` | Modify (add v4 FFI, deprecation annotations) |
| `hc_core/src/ffi_types.rs` | Modify (add `HC_KeyRequestV2`, `HC_KeyResultV2`) |
| `linux_fcitx5/hcime.cpp` | Modify (migrate to v4) |
| `scripts/e2e-smoke.sh` | Modify (update ABI export checks) |

---

## Epic 7: Thin C++ Frontend

**Epic ID:** E7
**Dependencies:** Epic 6 (new FFI surface)

### Summary

Extract components from the 1,256-line `hcime.cpp` god-class into focused classes:
key handler, candidate adapter, and status menu. Eliminate `if (mode >= 3 && mode <= 5)`
magic numbers by replacing them with `translationTarget == TranslationTarget::HanNom`
checks. Reduce `HcImeEngine` to under 200 lines.

### User Stories

- **US-7.1** — Extract `HcImeKeyHandler` class
- **US-7.2** — Extract `HcImeCandidateAdapter`
- **US-7.3** — Extract `HcImeStatusMenu`
- **US-7.4** — Replace magic numbers with translation target checks
- **US-7.5** — Reduce `HcImeEngine` to <200 lines

### Dependencies

- Epic 6 (unified v4 FFI function must be available for key handler extraction).

### Validation Gate

- `scripts/e2e-smoke.sh` passes.
- Candidate navigation works.
- Menu toggles work.
- Build succeeds with extracted classes.

### Estimated File Impact

| File | Action |
| --- | --- |
| `linux_fcitx5/hcime.cpp` | Modify (extract classes out) |
| `linux_fcitx5/hcime_key_handler.cpp` | Create |
| `linux_fcitx5/hcime_candidate_adapter.cpp` | Create |
| `linux_fcitx5/hcime_status_menu.cpp` | Create |
| `linux_fcitx5/CMakeLists.txt` | Modify (add new source files) |

---

## Epic 8: Memory Deduplication

**Epic ID:** E8
**Dependencies:** Epic 5 (TranslationEngine extracted)

### Summary

Share read-only configuration data globally via `OnceLock<Arc<HashMap>>`:
macros and phrase history path. Implement lazy PhraseHistory loading per session.
Eliminate per-session duplication of read-only config data. Add a memory
regression test verifying pure Vietnamese sessions have zero Han Nom heap
allocation.

### User Stories

- **US-8.1** — Move macros to global `OnceLock<Arc<HashMap<String, String>>>`
- **US-8.2** — Move `phrase_history_path` to global `OnceLock<PathBuf>`
- **US-8.3** — Implement lazy PhraseHistory loading
- **US-8.4** — Update macro mutation FFI functions
- **US-8.5** — Add memory regression test

### Dependencies

- Epic 5 (`Option<Box<dyn Translator>>` makes the zero-Han-Nom-state assertion possible).

### Validation Gate

- `scripts/e2e-smoke.sh` passes.
- Macros loaded once and shared across sessions.
- Phrase history not loaded for pure Vietnamese sessions.
- Multi-window scenario does not duplicate the macro `HashMap`.

### Estimated File Impact

| File | Action |
| --- | --- |
| `hc_core/src/session.rs` | Modify (use global statics) |
| `hc_core/src/macros.rs` | Modify (move to global `OnceLock`) |
| `hc_core/src/han_nom.rs` | Modify (lazy phrase history, global path) |
| `hc_core/src/lib.rs` | Modify (update FFI for macro mutations) |
| `hc_core/tests/session_tests.rs` | Modify (add memory regression test) |

---

## Cross-Platform Requirements

- All `platform::*()` calls must resolve correctly on Linux (primary target).
- macOS and Windows paths are handled by the `dirs` crate automatically.
- Unix permission operations use `#[cfg(unix)]` guards.
- File loading in Epic 4 must degrade gracefully on Windows (fall back to
  embedded dict if no file is found).
- Path separator and permission operations must not panic on any platform.

## Epic Dependency Graph

```
E0 ──────┬──── E4 ────┐
          │            │
E1 ── E2 ── E3 ─────── E5 ──┬── E6 ── E7
                              │
                              └── E8
```

Epics E0 and E1 can proceed in parallel. E2 builds on both. E3 requires E2.
E4 requires E0. E5 requires E3 and E4. E6 requires E5. E7 requires E6.
E8 requires E5 and can run in parallel with E6/E7.
