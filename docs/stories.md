# HC_IME Refactoring — User Story Specifications

This document defines the user story specifications for the HC_IME refactoring
project. Each story maps to an epic defined in [`epics.md`](epics.md).

---

## Epic 0 Stories

---

### US-0.1: Add `dirs` dependency and `platform.rs` module

**As a** core developer,
**I want** a centralized `platform.rs` module backed by the `dirs` crate for cross-platform path resolution,
**so that** all modules use a single, tested source for platform directories instead of manual `XDG_*` construction.

**Acceptance Criteria:**
- `dirs` crate added to `hc_core/Cargo.toml` dependencies.
- `hc_core/src/platform.rs` created as a public module.
- Exports `data_dir()`, `config_dir()`, `state_dir()`, `dictionary_paths()`.
- `dictionary_paths()` returns a `Vec<PathBuf>` of candidate search directories.
- Module declared in `hc_core/src/lib.rs`.

**Atomic Tasks:**
1. Add `dirs` to `hc_core/Cargo.toml` under `[dependencies]`.
2. Create `hc_core/src/platform.rs`.
3. Implement `pub fn data_dir() -> Option<PathBuf>` using `dirs::data_dir()`.
4. Implement `pub fn config_dir() -> Option<PathBuf>` using `dirs::config_dir()`.
5. Implement `pub fn state_dir() -> Option<PathBuf>` using `dirs::state_dir()`.
6. Implement `pub fn dictionary_paths() -> Vec<PathBuf>` with priority-ordered candidate directories.
7. Add `pub mod platform;` to `hc_core/src/lib.rs`.
8. Write doc comments on all public functions.

**Files Touched:**
- `hc_core/Cargo.toml`
- `hc_core/src/platform.rs` (create)
- `hc_core/src/lib.rs`

**Estimated Effort:** XS

---

### US-0.2: Migrate `default_history_path()` in han_nom.rs

**As a** core developer,
**I want** `default_history_path()` to use `platform::state_dir()` instead of manually constructing `XDG_STATE_HOME`,
**so that** the history path is resolved consistently across all platforms.

**Acceptance Criteria:**
- `default_history_path()` calls `platform::state_dir()`.
- Fallback behavior matches the old manual construction.
- Path returned is identical to the current behavior on Linux with standard XDG variables.

**Atomic Tasks:**
1. Locate `default_history_path()` in `hc_core/src/han_nom.rs`.
2. Replace manual `XDG_STATE_HOME` / `HOME` logic with `platform::state_dir()`.
3. Append `"hcime"` subdirectory and filename as before.
4. Verify path output matches old behavior on Linux.

**Files Touched:**
- `hc_core/src/lib.rs`

**Estimated Effort:** M

---

### US-0.3: Migrate dictionary paths in language.rs

**As a** core developer,
**I want** dictionary loading in `language.rs` to use `platform::dictionary_paths()` for file search,
**so that** dictionary file resolution is consistent and portable.

**Acceptance Criteria:**
- Dictionary file searches use `platform::dictionary_paths()`.
- Existing `XDG_*` and `HOME` environment variable construction is removed.
- Paths function identically on Linux.

**Atomic Tasks:**
1. Locate dictionary path construction in `hc_core/src/language.rs`.
2. Replace manual path logic with `platform::dictionary_paths()` and `platform::data_dir()`.
3. Ensure fallback chain matches current behavior.
4. Run `cargo test` to confirm no regressions.

**Files Touched:**
- `hc_core/src/language.rs`

**Estimated Effort:** XS

---

### US-0.4: Add cross-platform `#[cfg]` guards for file permissions

**As a** core developer,
**I want** file permission operations guarded with `#[cfg(unix)]` attributes,
**so that** the code compiles and runs without panics on macOS, Windows, and other platforms.

**Acceptance Criteria:**
- All `chmod`, `set_permissions`, or Unix-specific permission calls are wrapped in `#[cfg(unix)]`.
- Non-Unix platforms use no-op or `#[cfg(not(unix))]` alternatives.
- Code compiles on Linux (primary), with no regressions.

**Atomic Tasks:**
1. Audit `hc_core/src/` for file permission operations.
2. Add `#[cfg(unix)]` to all Unix-specific permission calls.
3. Add `#[cfg(not(unix))]` alternatives (no-op or `eprintln!` warning).
4. Verify compilation and tests on Linux.

**Files Touched:**
- `hc_core/src/han_nom.rs`
- `hc_core/src/language.rs` (if applicable)

**Estimated Effort:** S

---

## Epic 1 Stories

---

### US-1.1: Fix Telex double-tap toggle-off

**As a** Telex user,
**I want** typing `aaa` to produce `aa` (not `âa`),
**so that** toggling a diacritic off does not consume the second keystroke.

**Acceptance Criteria:**
- When a double-tap would strip a circumflex (already applied), the second keystroke is emitted as a literal character.
- `aaa` → `aa` (double-tap `aa` toggles `â`, third `a` is literal).
- Other double-tap toggle-off sequences work correctly (e.g., `ow` → `ơ`, `oww` → `o` — already works).
- No regression in other double-tap transforms (`dd` → `đ`, `aa` → `â`, etc.).

**Atomic Tasks:**
1. Locate `apply_double_tap` (or equivalent) in `hc_core/src/compose.rs`.
2. Add circumflex-strip logic: when double-tap would strip a circumflex, return `false` and emit the literal keystroke.
3. Verify tone mark stripping (e.g., `as` → `á`, `ass` → `a`) is not affected.
4. Add explicit test case: `aaa` → `aa`.

**Files Touched:**
- `hc_core/src/transform.rs`
- `hc_core/src/tests.rs`

**Estimated Effort:** S

---

### US-1.2: Fix VNI diacritic toggle functions

**As a** VNI user,
**I want** typing `a66` to produce `a6` (not `â6` or `â`),
**so that** toggling a diacritic off returns `false` and the digit is emitted as a literal.

**Acceptance Criteria:**
- Diacritic toggle functions for circumflex (`6`), horn (`7`), breve (`8`), and d-stroke (`9`) return `false` when stripping a diacritic.
- Tone toggle functions (digits `1`–`5`) are NOT changed (they have different toggle semantics).
- `a66` → `a6` (first `6` adds circumflex `â`, second `6` strips it and emits literal `6`).
- `o77` → `o7`.
- `ddd` → `dd` (VNI `9` for d-stroke).

**Atomic Tasks:**
1. Audit VNI diacritic toggle functions in `hc_core/src/vni.rs` (or `compose.rs`).
2. For circumflex (`6`), horn (`7`), breve (`8`), d-stroke (`9`): return `false` when stripping.
3. Do NOT modify tone toggles (`1`–`5`).
4. Add test cases: `a66` → `a6`, `o77` → `o7`, `ddd` → `dd`.

**Files Touched:**
- `hc_core/src/transform.rs`
- `hc_core/src/tests.rs`

**Estimated Effort:** S

---

### US-1.3: Defer PhraseHistory load to first Han Nom keystroke

**As a** user opening a new input context,
**I want** PhraseHistory to load lazily on the first Han Nom keystroke instead of synchronously during session init,
**so that** the OS does not experience a brief freeze when the IME is first activated.

**Acceptance Criteria:**
- `PhraseHistory` is NOT loaded during `hc_session_create()` or `hc_session_init()`.
- A `phrase_history_loaded: bool` guard is checked before any phrase lookup.
- On the first Han Nom keystroke, `PhraseHistory` loads once and the guard is set.
- Pure Vietnamese sessions never trigger `PhraseHistory` load.

**Atomic Tasks:**
1. Add `phrase_history_loaded: bool` field to the appropriate struct (Session or Han Nom state).
2. Remove synchronous PhraseHistory load from session initialization.
3. Add lazy-load check at the top of phrase lookup / Han Nom key handler.
4. Ensure load is idempotent (guard prevents double-load).
5. Verify pure Vietnamese keystrokes never trigger the load.

**Files Touched:**
- `hc_core/src/session.rs`
- `hc_core/src/han_nom.rs`

**Estimated Effort:** S

---

### US-1.4: Fix Fcitx5 menu ghosting

**As a** desktop user toggling the HC_IME status menu,
**I want** the input panel to not ghost (flicker or disappear) when the menu is activated,
**so that** candidate display is stable during mode switches.

**Acceptance Criteria:**
- `onMenuActivated` does not trigger `updateUserInterface(InputPanel)`.
- Or: `updateUserInterface` call is guarded to only run when the panel has meaningful changes.
- Menu open/close does not cause the candidate window to disappear.

**Atomic Tasks:**
1. Locate `onMenuActivated` in `linux_fcitx5/hcime.cpp`.
2. Audit `updateUserInterface(InputPanel)` calls within or triggered by the method.
3. Guard or remove the call that causes panel ghosting.
4. Verify manually: open status menu, observe candidate window stability.

**Files Touched:**
- `linux_fcitx5/hcime.cpp`

**Estimated Effort:** S

---

### US-1.5: Fix candidate layout

**As a** Fcitx5 user,
**I want** the candidate list to respect Fcitx5's native layout settings,
**so that** candidates are not forced into horizontal layout with hardcoded page size and labels.

**Acceptance Criteria:**
- `setLayoutHint(Horizontal)` is removed.
- `setPageSize(5)` is removed (Fcitx5 controls page size).
- Hardcoded candidate labels are removed (Fcitx5 provides native labels).
- Candidates display using Fcitx5's default layout.

**Atomic Tasks:**
1. Locate `setLayoutHint(Horizontal)` in `linux_fcitx5/hcime.cpp`.
2. Remove the call.
3. Locate `setPageSize(5)` call(s).
4. Remove the call(s).
5. Locate hardcoded label assignment.
6. Remove hardcoded labels, relying on Fcitx5 defaults.
7. Verify candidate display in Fcitx5.

**Files Touched:**
- `linux_fcitx5/hcime.cpp`

**Estimated Effort:** XS

---

### US-1.6: Add state drift check in `applySurroundingTextPreedit`

**As a** user in surrounding-text output mode,
**I want** a state drift check before `deleteSurroundingText` is called,
**so that** the IME does not corrupt text when the application modifies surrounding text behind the IME.

**Acceptance Criteria:**
- Before `deleteSurroundingText`, the function verifies that the current surrounding text ends with the previously inserted preedit.
- On mismatch, the function clears tracked state and commits the new preedit directly.
- No `deleteSurroundingText` call when state is drifted.

**Atomic Tasks:**
1. Locate `applySurroundingTextPreedit` in `linux_fcitx5/hcime.cpp`.
2. Add state validation: compare current surrounding text suffix against tracked preedit.
3. On match: proceed with existing diff logic.
4. On mismatch: clear tracked state, call `commitString(newPreedit)` directly.
5. Log the mismatch for debugging.

**Files Touched:**
- `linux_fcitx5/hcime.cpp`

**Estimated Effort:** S

---

### US-1.7: Add regression tests for double-tap toggle-off and VNI toggle literal emission

**As a** developer,
**I want** regression tests that lock in the double-tap toggle-off and VNI toggle fixes,
**so that** future changes cannot reintroduce these bugs.

**Acceptance Criteria:**
- Test: `aaa` with Telex mode → preedit `aa`.
- Test: `a66` with VNI mode → preedit `a6`.
- Test: `o77` with VNI mode → preedit `o7`.
- Test: `ddd` with VNI mode → preedit `dd`.
- All tests pass as regression suite.

**Atomic Tasks:**
1. Add test case: Telex `aaa` → `aa` in `tests/tests.rs` or appropriate test file.
2. Add test case: VNI `a66` → `a6`.
3. Add test case: VNI `o77` → `o7`.
4. Add test case: VNI `ddd` → `dd`.
5. Run `cargo test` to confirm tests pass.

**Files Touched:**
- `hc_core/tests/tests.rs`

**Estimated Effort:** XS

---

## Epic 2 Stories

---

### US-2.1: Make `test_helpers.rs` accessible to split modules

**As a** developer,
**I want** `test_helpers.rs` to export its utilities with `pub(crate)` visibility,
**so that** split test modules under `tests/` can use shared test helpers.

**Acceptance Criteria:**
- All helper functions and types in `test_helpers.rs` are marked `pub(crate)`.
- Split test modules can `use hc_core::test_helpers::*`.
- No test compilation errors from visibility mismatches.

**Atomic Tasks:**
1. Audit `hc_core/src/test_helpers.rs` for all public API items.
2. Change `pub` to `pub(crate)` or `pub` (depending on whether tests are integration or unit).
3. If tests under `tests/` are integration tests, ensure helpers are accessible via `hc_core::test_helpers`.
4. Verify compilation.

**Files Touched:**
- `hc_core/src/test_helpers.rs`

**Estimated Effort:** XS

---

### US-2.2: Extract composition tests into `tests/composition_tests.rs`

**As a** developer,
**I want** composition-specific tests in their own module,
**so that** composition test failures are isolated and test files are manageable.

**Acceptance Criteria:**
- All composition tests (Telex, VNI, VIQR transforms, reconversion, raw replay, tone placement, spell check) moved to `tests/composition_tests.rs`.
- Tests compile and pass independently: `cargo test --test composition_tests`.
- No test logic changed — pure file reorganization.

**Atomic Tasks:**
1. Identify composition test functions in `tests/tests.rs` (lines covering composition logic).
2. Create `hc_core/tests/composition_tests.rs`.
3. Copy composition tests verbatim.
4. Add appropriate imports and module preamble.
5. Remove composition tests from `tests/tests.rs`.
6. Verify `cargo test --test composition_tests` passes.

**Files Touched:**
- `hc_core/src/tests.rs`
- `hc_core/src/tests/composition_tests.rs` (create)

**Estimated Effort:** M

---

### US-2.3: Extract translation/Han Nom tests into `tests/translation_tests.rs`

**As a** developer,
**I want** Han Nom and translation tests in their own module,
**so that** translation-related test failures are isolated.

**Acceptance Criteria:**
- All Han Nom tests (candidate lookup, phrase prediction, reading normalization, local learning) moved to `tests/translation_tests.rs`.
- Tests compile and pass independently: `cargo test --test translation_tests`.
- No test logic changed.

**Atomic Tasks:**
1. Identify Han Nom / translation test functions in `tests/tests.rs`.
2. Create `hc_core/tests/translation_tests.rs`.
3. Copy tests verbatim with appropriate imports.
4. Remove tests from `tests/tests.rs`.
5. Verify `cargo test --test translation_tests` passes.

**Files Touched:**
- `hc_core/src/tests.rs`
- `hc_core/src/tests/translation_tests.rs` (create)

**Estimated Effort:** M

---

### US-2.4: Extract session integration tests into `tests/session_tests.rs`

**As a** developer,
**I want** session lifecycle and integration tests in their own module,
**so that** session-related test failures are isolated.

**Acceptance Criteria:**
- All session tests (create, init, reset, mode switching, configuration propagation) moved to `tests/session_tests.rs`.
- Tests compile and pass independently: `cargo test --test session_tests`.
- No test logic changed.

**Atomic Tasks:**
1. Identify session integration test functions in `tests/tests.rs`.
2. Create `hc_core/tests/session_tests.rs`.
3. Copy tests verbatim with appropriate imports.
4. Remove tests from `tests/tests.rs`.
5. Verify `cargo test --test session_tests` passes.

**Files Touched:**
- `hc_core/src/tests.rs`
- `hc_core/src/tests/session_tests.rs` (create)

**Estimated Effort:** M

---

### US-2.5: Extract FFI boundary tests into `tests/ffi_tests.rs`

**As a** developer,
**I want** FFI boundary tests in their own module,
**so that** FFI contract violations are isolated and easy to diagnose.

**Acceptance Criteria:**
- All FFI tests (C ABI function calls, struct layouts, pointer lifetimes, borrowed output) moved to `tests/ffi_tests.rs`.
- Tests compile and pass independently: `cargo test --test ffi_tests`.
- No test logic changed.

**Atomic Tasks:**
1. Identify FFI boundary test functions in `tests/tests.rs`.
2. Create `hc_core/tests/ffi_tests.rs`.
3. Copy tests verbatim with appropriate imports.
4. Remove tests from `tests/tests.rs`.
5. Verify `cargo test --test ffi_tests` passes.

**Files Touched:**
- `hc_core/src/tests.rs`
- `hc_core/src/tests/ffi_tests.rs` (create)

**Estimated Effort:** M

---

### US-2.6: Add Han Nom composition-path integration tests

**As a** developer,
**I want** integration tests that verify the Hán Nôm reading composition uses Vietnamese composition identically to pure Vietnamese mode,
**so that** regressions in the shared composition path are caught before they reach end-to-end.

**Acceptance Criteria:**
- Test: Hán Nôm Telex reading composition produces the same preedit as pure Telex for the same keystrokes.
- Test: Hán Nôm VNI reading composition produces the same preedit as pure VNI.
- Test: Tone placement in Hán Nôm mode matches pure Vietnamese tone placement.
- Tests run as part of `cargo test --test translation_tests`.

**Atomic Tasks:**
1. Design test: type Vietnamese reading keystrokes in HanNomTelex mode, assert preedit matches pure Telex mode preedit.
2. Design test: type Vietnamese reading keystrokes in HanNomVni mode, assert preedit matches pure Vni mode preedit.
3. Design test: verify tone placement consistency between modes.
4. Add tests to `hc_core/tests/translation_tests.rs`.
5. Run `cargo test --test translation_tests` to confirm.

**Files Touched:**
- `hc_core/tests/translation_tests.rs`

**Estimated Effort:** S

---

## Epic 3 Stories

---

### US-3.1: Define `CompositionEngine` struct

**As a** core developer,
**I want** a `CompositionEngine` struct that holds all pure-Vietnamese composition fields currently in `Session`,
**so that** composition state is cleanly separated from session and translation state.

**Acceptance Criteria:**
- `CompositionEngine` struct defined in `hc_core/src/composition.rs`.
- Fields extracted from `Session`: `raw_buffer`, `commit_buffer`, `preedit`, `spell_check_status`, `undo_stack`, composition mode flags, reconversion state, and other pure-Vietnamese fields.
- Struct derives `Default` or has a `new()` constructor.

**Atomic Tasks:**
1. Create `hc_core/src/composition.rs`.
2. Audit `Session` struct in `hc_core/src/session.rs` for pure-Vietnamese composition fields.
3. Define `CompositionEngine` struct with extracted fields.
4. Implement `Default` or `new()` for `CompositionEngine`.
5. Add `pub mod composition;` to `hc_core/src/lib.rs`.

**Files Touched:**
- `hc_core/src/composition.rs` (create)
- `hc_core/src/lib.rs`

**Estimated Effort:** S

---

### US-3.2: Move composition methods to `CompositionEngine`

**As a** core developer,
**I want** composition methods moved from `Session` to `CompositionEngine`,
**so that** the composition module owns its behavior entirely.

**Acceptance Criteria:**
- Methods moved: `render_from_raw`, `apply_quick_consonants`, `apply_end_quick_consonants`, `update_spell_check_status`, `emit_preedit`, `commit_current`, `save_state_for_undo`, `undo`, `try_boundary_trigger`, `try_esc_restore_raw`, `resolve_commit_text`.
- Each method takes `&mut self` and any additional parameters needed.
- Methods compile and are callable.

**Atomic Tasks:**
1. For each method listed, copy the implementation from `Session` to `impl CompositionEngine`.
2. Adjust method signatures to use `&mut self` where `self.session_field` was used.
3. Remove the methods from `impl Session` (or empty them to delegate).
4. Verify compilation.

**Files Touched:**
- `hc_core/src/composition.rs`
- `hc_core/src/session.rs`

**Estimated Effort:** L

---

### US-3.3: Update `Session` to hold and delegate to `CompositionEngine`

**As a** core developer,
**I want** `Session` to hold `composition: CompositionEngine` and delegate calls,
**so that** the refactoring is zero-behavioral-change.

**Acceptance Criteria:**
- `Session` has `composition: CompositionEngine` field.
- All removed composition fields are no longer directly on `Session`.
- All call sites of composition methods use `self.composition.method()`.
- All existing tests pass identically.

**Atomic Tasks:**
1. Add `composition: CompositionEngine` field to `Session`.
2. Remove extracted fields from `Session` struct.
3. Update `Session::new()` or `Default` to initialize `composition`.
4. Update all internal `Session` call sites from `self.method()` to `self.composition.method()`.
5. Update all field accesses from `self.field` to `self.composition.field`.
6. Run full test suite.

**Files Touched:**
- `hc_core/src/session.rs`

**Estimated Effort:** L

---

### US-3.4: Update Han Nom handlers to use `self.composition.*`

**As a** core developer,
**I want** Han Nom handler functions (`handle_han_nom_key`, `handle_han_nom_key_v2`, `handle_han_nom_key_v3`) to use `self.composition.render_from_raw()` and other composition methods,
**so that** Han Nom composition reuses the extracted engine.

**Acceptance Criteria:**
- `handle_han_nom_key` uses `self.composition.render_from_raw()` for reading composition.
- `handle_han_nom_key_v2` uses `self.composition.*` methods.
- `handle_han_nom_key_v3` uses `self.composition.*` methods.
- No behavioral change; all Han Nom tests pass.

**Atomic Tasks:**
1. Update `handle_han_nom_key` to call `self.composition.render_from_raw()`.
2. Update `handle_han_nom_key_v2`.
3. Update `handle_han_nom_key_v3`.
4. Update any other Han Nom handler that directly manipulated composition fields.
5. Run Han Nom tests.

**Files Touched:**
- `hc_core/src/lib.rs`

**Estimated Effort:** M

---

### US-3.5: Update `lib.rs` FFI functions for new field paths

**As a** core developer,
**I want** FFI functions in `lib.rs` that access Session composition fields to use `session.composition.*`,
**so that** the C ABI functions compile and work with the extracted `CompositionEngine`.

**Acceptance Criteria:**
- All FFI functions that access composition fields use `session.composition.field`.
- FFI tests pass.
- No ABI breakage (struct layouts unchanged).

**Atomic Tasks:**
1. Audit `hc_core/src/lib.rs` for all `session.field` accesses that were moved to `CompositionEngine`.
2. Update to `session.composition.field`.
3. Run FFI tests: `cargo test --test ffi_tests`.

**Files Touched:**
- `hc_core/src/lib.rs`

**Estimated Effort:** M

---

## Epic 4 Stories

---

### US-4.1: Add `memmap2` dependency to Cargo.toml

**As a** core developer,
**I want** the `memmap2` crate added to `hc_core/Cargo.toml`,
**so that** zero-copy file-backed dictionary loading is available.

**Acceptance Criteria:**
- `memmap2` added to `[dependencies]` in `hc_core/Cargo.toml`.
- Crate version is the latest stable release.
- `cargo build` succeeds with the new dependency.

**Atomic Tasks:**
1. Add `memmap2 = "0.9"` (or latest) to `hc_core/Cargo.toml`.
2. Run `cargo build` to fetch and compile.

**Files Touched:**
- `hc_core/Cargo.toml`

**Estimated Effort:** XS

---

### US-4.2: Extend `get_global_dict()` with file-first loading

**As a** core developer,
**I want** `get_global_dict()` to search `platform::dictionary_paths()` for a `.bin` file before falling back to embedded data,
**so that** users can supply updated dictionaries without recompiling.

**Acceptance Criteria:**
- `get_global_dict()` calls `platform::dictionary_paths()`.
- Searches for `han_nom_dict.bin` in candidate directories.
- Falls back to `EMBEDDED_DICT_DATA` if no file is found or readable.
- `HC_IME_NOM_DICT` environment variable overrides the search path.
- Logs which source was used (file path or embedded).

**Atomic Tasks:**
1. Modify `get_global_dict()` in `hc_core/src/han_nom.rs`.
2. Add `platform::dictionary_paths()` call.
3. Check `HC_IME_NOM_DICT` env var first.
4. Iterate candidate paths for `han_nom_dict.bin`.
5. If found, use file loading (prep for US-4.3).
6. If not found, use existing `EMBEDDED_DICT_DATA` logic.
7. Add log/print indicating source.

**Files Touched:**
- `hc_core/src/han_nom.rs`

**Estimated Effort:** S

---

### US-4.3: Add `memmap2`-based zero-copy loading

**As a** core developer,
**I want** file-based dictionary loading to use `memmap2` for zero-copy access,
**so that** dictionary file loading has minimal memory overhead.

**Acceptance Criteria:**
- `memmap2::Mmap` is used to memory-map the `.bin` file.
- Dictionary data is accessed via the mmap slice, not a `Vec<u8>` copy.
- Mmap handle is stored (or leaked) for the lifetime of the dictionary.
- Binary format validation (magic bytes, version) runs against the mmap'd data.

**Atomic Tasks:**
1. In the file-found path from US-4.2, use `memmap2::Mmap::map(&file)`.
2. Validate binary header (magic, version) from mmap'd bytes.
3. Store or leak the `Mmap` handle as needed for `'static` lifetime.
4. Parse dictionary index from mmap'd bytes.
5. Fall back to embedded dict if mmap or validation fails.

**Files Touched:**
- `hc_core/src/han_nom.rs`

**Estimated Effort:** M

---

### US-4.4: Extend `get_global_phrase_dict()` with same pattern

**As a** core developer,
**I want** the phrase dictionary loader (`get_global_phrase_dict()`) to use the same file-first-then-embedded pattern as the character dictionary,
**so that** users can supply updated phrase dictionaries without recompiling.

**Acceptance Criteria:**
- `get_global_phrase_dict()` searches `platform::dictionary_paths()` for `han_nom_phrase_dict.bin`.
- `HC_IME_NOM_PHRASE_DICT` environment variable overrides the search path.
- Falls back to embedded phrase dict data.
- Uses `memmap2` for file-based loading.

**Atomic Tasks:**
1. Apply the same file-first pattern from US-4.2 to `get_global_phrase_dict()`.
2. Apply the same `memmap2` loading from US-4.3.
3. Fall back to `EMBEDDED_PHRASE_DICT_DATA`.
4. Test with and without file present.

**Files Touched:**
- `hc_core/src/han_nom.rs`

**Estimated Effort:** S

---

### US-4.5: Update CMake to install `.bin` files

**As a** packager,
**I want** the CMake build to install dictionary `.bin` files to the Fcitx5 pkgdatadir,
**so that** installed binaries can find their dictionaries at the expected runtime paths.

**Acceptance Criteria:**
- `linux_fcitx5/CMakeLists.txt` installs `hc_core/data/*.bin` to `${FCITX_INSTALL_PKGDATADIR}/hcime/data`.
- `platform::dictionary_paths()` includes this install path.
- `scripts/e2e-smoke.sh` verifies the files are installed.

**Atomic Tasks:**
1. Add `install(FILES ... DESTINATION ...)` to `linux_fcitx5/CMakeLists.txt`.
2. Reference `hc_core/data/han_nom_dict.bin` and `hc_core/data/han_nom_phrase_dict.bin`.
3. Ensure install path is `${FCITX_INSTALL_PKGDATADIR}/hcime/data`.
4. Verify with `cmake --install` that files land in the correct location.

**Files Touched:**
- `linux_fcitx5/CMakeLists.txt`

**Estimated Effort:** XS

---

## Epic 5 Stories

---

### US-5.1: Define `Translator` trait

**As a** core developer,
**I want** a `Translator` trait defining the interface for any translation backend,
**so that** Han Nom and future translation engines can be swapped without modifying Session.

**Acceptance Criteria:**
- Trait defined in `hc_core/src/translation.rs`.
- Required methods: `lookup(&self, reading: &str) -> Vec<Candidate>`, `lookup_phrase(...)`, `select(...)`, `record_selection(...)`.
- Optional: `reset()`, `load_config(...)`.
- Trait is object-safe (no generic methods that prevent `dyn Translator`).

**Atomic Tasks:**
1. Define `Translator` trait in `hc_core/src/translation.rs`.
2. Define `Candidate` type (or reuse existing).
3. Add doc comments for each method.
4. Ensure trait is `dyn`-compatible.

**Files Touched:**
- `hc_core/src/translation.rs` (create)

**Estimated Effort:** S

---

### US-5.2: Create `hc_core/src/translation.rs` module

**As a** core developer,
**I want** a dedicated `translation.rs` module that houses the `Translator` trait and `HanNomTranslator` implementation,
**so that** translation logic is cleanly separated from session management.

**Acceptance Criteria:**
- Module declared in `hc_core/src/lib.rs`.
- Module contains trait (US-5.1) and implementation (US-5.3).
- Module compiles.

**Atomic Tasks:**
1. Create `hc_core/src/translation.rs`.
2. Add `pub mod translation;` to `hc_core/src/lib.rs`.
3. Add necessary `use` imports.

**Files Touched:**
- `hc_core/src/translation.rs` (create)
- `hc_core/src/lib.rs`

**Estimated Effort:** XS

---

### US-5.3: Implement `HanNomTranslator` struct

**As a** core developer,
**I want** a `HanNomTranslator` struct implementing the `Translator` trait,
**so that** Han Nom functionality is encapsulated behind the trait interface.

**Acceptance Criteria:**
- `HanNomTranslator` implements `Translator`.
- `lookup()` delegates to existing Han Nom candidate lookup.
- `lookup_phrase()` delegates to existing phrase prediction.
- `select()` commits a candidate and records selection.
- `record_selection()` updates local learning data.

**Atomic Tasks:**
1. Define `HanNomTranslator` struct in `hc_core/src/translation.rs`.
2. Implement `Translator` for `HanNomTranslator`.
3. Wire `lookup()` to existing lookup logic.
4. Wire `lookup_phrase()` to existing phrase prediction.
5. Wire `select()` and `record_selection()`.
6. Compile and verify trait impl.

**Files Touched:**
- `hc_core/src/translation.rs`

**Estimated Effort:** L

---

### US-5.4: Move Han Nom fields from Session into `HanNomTranslator`

**As a** core developer,
**I want** Han Nom-specific fields removed from `Session` and placed into `HanNomTranslator`,
**so that** Session only delegates to the translator without owning translation state.

**Acceptance Criteria:**
- Han Nom fields (reading buffer, candidate state, phrase state, local learning, phrase history reference) moved to `HanNomTranslator`.
- `HanNomTranslator` owns its state.
- No Han Nom fields remain directly on `Session`.

**Atomic Tasks:**
1. Audit `Session` struct for Han Nom fields.
2. Move fields to `HanNomTranslator`.
3. Update field accesses in `translation.rs`.
4. Remove fields from `Session` struct.

**Files Touched:**
- `hc_core/src/session.rs`
- `hc_core/src/translation.rs`

**Estimated Effort:** L

---

### US-5.5: Update Session to hold `Option<Box<dyn Translator>>`

**As a** core developer,
**I want** `Session` to hold `translator: Option<Box<dyn Translator>>` instead of inline Han Nom fields,
**so that** pure Vietnamese mode allocates zero Han Nom state.

**Acceptance Criteria:**
- `Session` has `translator: Option<Box<dyn Translator>>`.
- When input mode is pure Vietnamese, `translator` is `None`.
- When input mode is Han Nom, `translator` is `Some(HanNomTranslator::new(...))`.
- All Han Nom FFI functions delegate through `session.translator`.
- Pure Vietnamese sessions have no Han Nom heap allocation.

**Atomic Tasks:**
1. Add `translator: Option<Box<dyn Translator>>` to `Session`.
2. Update session creation/init to set `translator` based on input mode.
3. Update mode-switch logic to create/destroy translator as needed.
4. Gate all Han Nom code paths with `if let Some(t) = &self.translator`.
5. Verify pure Vietnamese mode does not allocate translator.

**Files Touched:**
- `hc_core/src/session.rs`
- `hc_core/src/lib.rs`

**Estimated Effort:** L

---

### US-5.6: Add FFI pointer lifetime contract doc comment

**As a** developer integrating with the C ABI,
**I want** a clear doc comment documenting that FFI-returned pointers are valid only until the next call on the same session,
**so that** the C++ frontend can safely use borrowed data without use-after-free.

**Acceptance Criteria:**
- Each FFI function returning borrowed data has a `/// # Safety` or `/// # Lifetime` doc comment.
- Comments specify: "Pointer is valid until the next FFI call on the same session or session destruction."
- All relevant functions documented consistently.

**Atomic Tasks:**
1. Audit all FFI functions in `hc_core/src/lib.rs` that return pointers.
2. Add doc comments documenting pointer lifetime.
3. Ensure comment format is consistent.

**Files Touched:**
- `hc_core/src/lib.rs`

**Estimated Effort:** XS

---

### US-5.7: Update all Han Nom FFI functions

**As a** core developer,
**I want** all Han Nom FFI functions to delegate through `translator` instead of directly accessing Session fields,
**so that** the FFI layer is translation-engine-agnostic.

**Acceptance Criteria:**
- `hc_session_handle_key_hannom` (v1) delegates through `session.translator`.
- `hc_session_handle_key_hannom_v2` delegates through `session.translator`.
- `hc_session_handle_key_hannom_v3` delegates through `session.translator`.
- Candidate lookup FFI functions delegate through `session.translator`.
- Phrase lookup FFI functions delegate through `session.translator`.
- All Han Nom FFI tests pass.

**Atomic Tasks:**
1. Audit all Han Nom FFI functions in `lib.rs`.
2. Replace direct field access with `session.translator.as_ref().unwrap().method()`.
3. Handle `None` translator with appropriate error return.
4. Run FFI tests.

**Files Touched:**
- `hc_core/src/lib.rs`

**Estimated Effort:** M

---

## Epic 6 Stories

---

### US-6.1: Add `HC_KeyRequestV2` struct

**As a** core developer,
**I want** an additive `HC_KeyRequestV2` struct with a `translation_target` field,
**so that** the FFI can distinguish between pure Vietnamese and translation modes in the key request.

**Acceptance Criteria:**
- `HC_KeyRequestV2` contains all fields from `HC_KeyRequest` plus `translation_target: u8`.
- `HC_KeyRequest` is NOT removed or modified (additive protocol).
- Struct is `#[repr(C)]` and FFI-safe.

**Atomic Tasks:**
1. Define `HC_KeyRequestV2` in `hc_core/src/ffi_types.rs` (or `lib.rs`).
2. Include all `HC_KeyRequest` fields plus `translation_target: u8`.
3. Define `TranslationTarget` constants (e.g., `TRANSLATION_TARGET_VIETNAMESE = 0`, `TRANSLATION_TARGET_HAN_NOM = 1`).
4. Ensure `#[repr(C)]`.

**Files Touched:**
- `hc_core/src/types.rs` (or `hc_core/src/lib.rs`)

**Estimated Effort:** XS

---

### US-6.2: Add `HC_KeyResultV2` unified result type

**As a** core developer,
**I want** a unified `HC_KeyResultV2` type that can hold both pure Vietnamese preedit/commit data and optional candidate data,
**so that** a single FFI function can serve both modes.

**Acceptance Criteria:**
- `HC_KeyResultV2` contains preedit/commit fields plus optional candidate pointer/length.
- Struct is `#[repr(C)]` and FFI-safe.
- Does not replace `HC_KeyResult` (additive).

**Atomic Tasks:**
1. Define `HC_KeyResultV2` struct.
2. Include fields: preedit, commit_text, status flags, candidate_data pointer, candidate_count.
3. Document when candidate fields are valid.
4. Ensure `#[repr(C)]`.

**Files Touched:**
- `hc_core/src/types.rs` (or `hc_core/src/lib.rs`)

**Estimated Effort:** XS

---

### US-6.3: Add `hc_session_handle_key_v4()` FFI function

**As a** core developer,
**I want** a unified `hc_session_handle_key_v4()` FFI function that routes internally based on `translation_target`,
**so that** the C++ frontend calls a single entry point for all modes.

**Acceptance Criteria:**
- `hc_session_handle_key_v4(request: &HC_KeyRequestV2, result: &mut HC_KeyResultV2)`.
- Routes to pure Vietnamese handler when `translation_target == VIETNAMESE`.
- Routes to translator handler when `translation_target == HAN_NOM`.
- Function is exported in the C ABI.
- `e2e-smoke.sh` ABI checks include the new symbol.

**Atomic Tasks:**
1. Implement `hc_session_handle_key_v4()` in `hc_core/src/lib.rs`.
2. Gate routing on `request.translation_target`.
3. Add `#[no_mangle] pub extern "C"` signature.
4. Update `hc_core_ffi.h` with the new function declaration.
5. Add to `e2e-smoke.sh` ABI symbol checks.

**Files Touched:**
- `hc_core/src/lib.rs`
- `hc_core/hc_core_ffi.h`
- `linux_fcitx5/include/hcime/hc_core_ffi.h`
- `scripts/e2e-smoke.sh`

**Estimated Effort:** M

---

### US-6.4: Migrate `hcime.cpp` to unified v4

**As a** addon developer,
**I want** `hcime.cpp` to use `hc_session_handle_key_v4()` instead of `hc_session_handle_key_utf8` + `hc_session_handle_key_hannom_v3`,
**so that** the C++ frontend has a single key-handling code path.

**Acceptance Criteria:**
- `keyEvent()` calls `hc_session_handle_key_v4()` for all modes.
- `translation_target` is set based on `HcImeInputMode` in the `HC_KeyRequestV2`.
- Old separate-call logic is removed.
- All modes (Telex, VNI, VIQR, Han Nom Telex/VNI/VIQR) work identically.

**Atomic Tasks:**
1. Update `keyEvent()` to construct `HC_KeyRequestV2` with `translation_target`.
2. Replace `hc_session_handle_key_utf8` and `hc_session_handle_key_hannom_v3` calls with `hc_session_handle_key_v4`.
3. Update result handling to use `HC_KeyResultV2`.
4. Test all six input modes.

**Files Touched:**
- `linux_fcitx5/hcime.cpp`

**Estimated Effort:** L

---

### US-6.5: Mark v1 and v2 Han Nom FFI `#[deprecated]`

**As a** core developer,
**I want** the v1 and v2 Han Nom FFI functions marked `#[deprecated]`,
**so that** consumers are warned to migrate while the old symbols remain callable.

**Acceptance Criteria:**
- `hc_session_handle_key_hannom` (v1) marked `#[deprecated]`.
- `hc_session_handle_key_hannom_v2` marked `#[deprecated]`.
- `hc_session_handle_key_hannom_v3` NOT marked deprecated (v3 wraps v2).
- Deprecation messages reference the v4 replacement.

**Atomic Tasks:**
1. Add `#[deprecated = "Use hc_session_handle_key_v4 instead"]` to v1.
2. Add same to v2.
3. Do NOT deprecate v3 (still wraps v2 internally).
4. Verify compilation with deprecation warnings.

**Files Touched:**
- `hc_core/src/lib.rs`

**Estimated Effort:** XS

---

### US-6.6: Update `e2e-smoke.sh` ABI checks

**As a** maintainer,
**I want** the e2e smoke script to verify the new v4 FFI symbols and confirm v1/v2 are still exported,
**so that** the ABI contract is validated in every CI run.

**Acceptance Criteria:**
- ABI check includes `hc_session_handle_key_v4`.
- ABI check includes `hc_session_handle_key_hannom` (v1, deprecated).
- ABI check includes `hc_session_handle_key_hannom_v2` (deprecated).
- ABI check includes `hc_session_handle_key_hannom_v3`.

**Atomic Tasks:**
1. Add `hc_session_handle_key_v4` to the ABI symbol check list in `scripts/e2e-smoke.sh`.
2. Confirm v1, v2, v3 are still in the check list.
3. Run `scripts/e2e-smoke.sh` to verify.

**Files Touched:**
- `scripts/e2e-smoke.sh`

**Estimated Effort:** XS

---

### US-6.7: Verify v3 remains working

**As a** maintainer,
**I want** to verify that v3 (which wraps v2 internally) continues to function correctly with the `Translator` trait,
**so that** v3 consumers are not broken by the refactoring.

**Acceptance Criteria:**
- `hc_session_handle_key_hannom_v3` passes through `translator` and returns correct results.
- v3 candidate lookup and phrase prediction work identically to pre-refactoring.
- v3 tests pass.

**Atomic Tasks:**
1. Audit `hc_session_handle_key_hannom_v3` implementation.
2. Ensure v3 delegates through `session.translator` (from US-5.7).
3. Run v3-specific tests.
4. Run full test suite.

**Files Touched:**
- `hc_core/src/lib.rs`

**Estimated Effort:** S

---

## Epic 7 Stories

---

### US-7.1: Extract `HcImeKeyHandler` class

**As a** addon developer,
**I want** the key event handling logic (lines 459-702 of `hcime.cpp`) extracted into an `HcImeKeyHandler` class,
**so that** key handling is testable and `hcime.cpp` is thinner.

**Acceptance Criteria:**
- `HcImeKeyHandler` class in a new file.
- Contains `keyEvent()` logic from `hcime.cpp`.
- `HcImeEngine` creates and delegates to `HcImeKeyHandler`.
- Key handling behavior is identical.

**Atomic Tasks:**
1. Create `linux_fcitx5/hcime_key_handler.cpp` and header.
2. Move `keyEvent()` body logic into `HcImeKeyHandler::handle()`.
3. `HcImeEngine::keyEvent()` delegates to `handler.handle()`.
4. Update `CMakeLists.txt` with new source file.
5. Build and verify key handling.

**Files Touched:**
- `linux_fcitx5/hcime_key_handler.cpp` (create)
- `linux_fcitx5/hcime_key_handler.h` (create)
- `linux_fcitx5/hcime.cpp`
- `linux_fcitx5/CMakeLists.txt`

**Estimated Effort:** L

---

### US-7.2: Extract `HcImeCandidateAdapter`

**As a** addon developer,
**I want** the candidate UI update logic (lines 973-1021 of `hcime.cpp`) extracted into `HcImeCandidateAdapter`,
**so that** candidate display is self-contained and testable.

**Acceptance Criteria:**
- `HcImeCandidateAdapter` class in a new file.
- Contains `updateHanNomUi()` logic.
- `HcImeEngine` delegates to `HcImeCandidateAdapter::update()`.
- Candidate display behavior is identical.

**Atomic Tasks:**
1. Create `linux_fcitx5/hcime_candidate_adapter.cpp` and header.
2. Move `updateHanNomUi()` logic into `HcImeCandidateAdapter::update()`.
3. `HcImeEngine` delegates.
4. Update `CMakeLists.txt`.
5. Build and verify candidate display.

**Files Touched:**
- `linux_fcitx5/hcime_candidate_adapter.cpp` (create)
- `linux_fcitx5/hcime_candidate_adapter.h` (create)
- `linux_fcitx5/hcime.cpp`
- `linux_fcitx5/CMakeLists.txt`

**Estimated Effort:** M

---

### US-7.3: Extract `HcImeStatusMenu`

**As a** addon developer,
**I want** the status menu methods extracted into `HcImeStatusMenu`,
**so that** menu logic is self-contained.

**Acceptance Criteria:**
- `HcImeStatusMenu` class in a new file.
- Contains `buildStatusMenu`, `attachStatusMenu`, `onMenuActivated`, `refreshStatusMenu`.
- `HcImeEngine` delegates to `HcImeStatusMenu`.

**Atomic Tasks:**
1. Create `linux_fcitx5/hcime_status_menu.cpp` and header.
2. Move menu methods into `HcImeStatusMenu`.
3. `HcImeEngine` delegates.
4. Update `CMakeLists.txt`.
5. Build and verify menu operations.

**Files Touched:**
- `linux_fcitx5/hcime_status_menu.cpp` (create)
- `linux_fcitx5/hcime_status_menu.h` (create)
- `linux_fcitx5/hcime.cpp`
- `linux_fcitx5/CMakeLists.txt`

**Estimated Effort:** M

---

### US-7.4: Replace magic numbers with translation target checks

**As a** addon developer,
**I want** `if (mode >= 3 && mode <= 5)` checks replaced with `translationTarget == TranslationTarget::HanNom`,
**so that** the code is readable and immune to enum reordering.

**Acceptance Criteria:**
- All `if (mode >= 3 && mode <= 5)` replaced with `translationTarget == TRANSLATION_TARGET_HAN_NOM` (or equivalent constant).
- No magic numbers remain.
- Behavior is identical.

**Atomic Tasks:**
1. Grep for `mode >= 3` and `mode <= 5` patterns in `hcime.cpp` and new files.
2. Replace with `translation_target == TRANSLATION_TARGET_HAN_NOM`.
3. Ensure the constant matches the FFI `TranslationTarget` values.
4. Build and verify all mode-dependent behavior.

**Files Touched:**
- `linux_fcitx5/hcime.cpp`
- `linux_fcitx5/hcime_key_handler.cpp` (if created)

**Estimated Effort:** S

---

### US-7.5: Reduce `HcImeEngine` to <200 lines

**As a** addon developer,
**I want** the main `HcImeEngine` class to be under 200 lines (excluding includes),
**so that** it serves as a thin adapter between Fcitx5 and the extracted components.

**Acceptance Criteria:**
- `hcime.cpp` main class is <200 lines (excluding includes and boilerplate).
- All extracted logic lives in component files.
- All functionality preserved.

**Atomic Tasks:**
1. Apply extractions from US-7.1, US-7.2, US-7.3.
2. Audit remaining code in `hcime.cpp` for further extraction opportunities.
3. Remove any dead code.
4. Count lines and confirm <200 (excluding includes).

**Files Touched:**
- `linux_fcitx5/hcime.cpp`

**Estimated Effort:** M

---

## Epic 8 Stories

---

### US-8.1: Move macros to global `OnceLock<Arc<HashMap<String, String>>>`

**As a** core developer,
**I want** macro definitions stored in a global `OnceLock<Arc<HashMap<String, String>>>` shared across sessions,
**so that** the macro HashMap is not duplicated per session.

**Acceptance Criteria:**
- Global `static MACROS: OnceLock<Arc<HashMap<String, String>>>`.
- Each session clones the `Arc` (cheap) instead of owning its own `HashMap`.
- Macro expansion reads from the shared `Arc`.
- `cargo test` passes.

**Atomic Tasks:**
1. Define `static MACROS: OnceLock<Arc<HashMap<String, String>>>` in `hc_core/src/macros.rs` or `session.rs`.
2. Initialize on first access (from macro file or empty).
3. Replace per-session `macros: HashMap<...>` with `macros: Arc<HashMap<...>>` cloned from global.
4. Update macro expansion to use `Arc`.
5. Run tests.

**Files Touched:**
- `hc_core/src/session.rs`

**Estimated Effort:** M

---

### US-8.2: Move `phrase_history_path` to global `OnceLock<PathBuf>`

**As a** core developer,
**I want** the phrase history file path stored in a global `OnceLock<PathBuf>`,
**so that** it is resolved once and shared across all sessions.

**Acceptance Criteria:**
- Global `static PHRASE_HISTORY_PATH: OnceLock<PathBuf>`.
- Path resolved once from config or `platform::state_dir()`.
- All sessions reference the global path.
- Path resolution is thread-safe.

**Atomic Tasks:**
1. Define `static PHRASE_HISTORY_PATH: OnceLock<PathBuf>`.
2. Initialize on first access.
3. Replace per-session `phrase_history_path` with reference to global.
4. Update all call sites.

**Files Touched:**
- `hc_core/src/han_nom.rs`
- `hc_core/src/session.rs`

**Estimated Effort:** S

---

### US-8.3: Implement lazy PhraseHistory loading

**As a** core developer,
**I want** `PhraseHistory` to load lazily on the first Han Nom keystroke per session,
**so that** pure Vietnamese sessions never incur the I/O cost.

**Acceptance Criteria:**
- `phrase_history_loaded: bool` guard on the translator (or session).
- `PhraseHistory` loads only on the first Han Nom keystroke.
- Pure Vietnamese sessions never trigger the load.
- Load is idempotent (guard prevents double-load).

**Atomic Tasks:**
1. Synchronize with US-1.3 if not already done.
2. Add `phrase_history_loaded: bool` to the translator's state.
3. Gate phrase history access behind the guard.
4. Verify pure Vietnamese sessions never load it.

**Files Touched:**
- `hc_core/src/translation.rs`
- `hc_core/src/session.rs`

**Estimated Effort:** S

---

### US-8.4: Update macro mutation FFI functions

**As a** core developer,
**I want** `hc_session_add_macro` and `hc_session_clear_macros` to operate on the global macro store,
**so that** macro changes are visible across all sessions.

**Acceptance Criteria:**
- `hc_session_add_macro` mutates the global `OnceLock<Arc<HashMap<...>>>`.
- `hc_session_clear_macros` clears the global store.
- Or: per-session override is supported with global as default.
- Thread safety maintained (use appropriate synchronization).

**Atomic Tasks:**
1. Decide design: global-only vs. per-session override with global default.
2. Update `hc_session_add_macro` to write to global.
3. Update `hc_session_clear_macros` to clear global.
4. Add synchronization if needed (`RwLock` or replace `OnceLock` value).
5. Run FFI tests.

**Files Touched:**
- `hc_core/src/lib.rs`
- `hc_core/src/session.rs`

**Estimated Effort:** M

---

### US-8.5: Add memory regression test

**As a** core developer,
**I want** a test that verifies a pure Vietnamese session has zero bytes of Han Nom heap allocation,
**so that** memory deduplication is enforced by CI.

**Acceptance Criteria:**
- Test creates a pure Vietnamese session.
- Asserts that no Han Nom translator is allocated (`translator.is_none()`).
- Asserts that no Han Nom dictionary is loaded.
- Asserts that phrase history is not loaded.
- Test runs as part of `cargo test --test session_tests`.

**Atomic Tasks:**
1. Write test: create session with Vietnamese mode.
2. Assert `session.translator.is_none()`.
3. Assert no dictionary load occurred.
4. Assert phrase history path is `None` or not loaded.
5. Add to session_tests.

**Files Touched:**
- `hc_core/tests/session_tests.rs`

**Estimated Effort:** XS

---

## Effort Summary

| Size | Count | Stories |
| --- | --- | --- |
| XS | 13 | US-0.1, US-0.2, US-0.3, US-1.5, US-1.7, US-2.1, US-4.1, US-4.5, US-5.2, US-5.6, US-6.1, US-6.2, US-8.5 |
| S | 13 | US-0.4, US-1.1, US-1.2, US-1.3, US-1.4, US-1.6, US-2.6, US-3.1, US-4.2, US-4.4, US-5.1, US-8.2, US-8.3 |
| M | 13 | US-2.2, US-2.3, US-2.4, US-2.5, US-3.4, US-3.5, US-4.3, US-5.7, US-6.3, US-7.2, US-7.3, US-7.5, US-8.4 |
| L | 7 | US-3.2, US-3.3, US-5.3, US-5.4, US-5.5, US-6.4, US-7.1 |
| XL | 0 | — |
| **Total** | **46** | |

## Cross-Platform Requirements

All stories that touch platform paths or file permissions must respect:

- **Linux (primary):** `platform::*()` resolves correctly via `dirs` + XDG variables.
- **macOS:** `dirs` crate maps to macOS standard directories automatically.
- **Windows:** `dirs` crate maps to Windows standard directories. File-based dictionary loading (Epic 4) falls back to embedded dict if no `.bin` file is found. Permission operations use `#[cfg(unix)]` guards and are no-ops on Windows.
- **Compilation:** All `#[cfg]`-gated code must compile on all platforms without errors.
