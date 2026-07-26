# HC_IME Status

This document is the local source of truth for the current repo snapshot.
It reflects the current worktree rooted at `de2b55b` and records only behavior
backed by the repository's tests or end-to-end smoke gate.

## Current Shape (Post-Refactoring, 2026-07-26)

- Rust core in `hc_core/` implements Telex, VNI, and VIQR composition through a standalone `CompositionEngine` in `composition.rs` (~22 fields, zero Han Nom knowledge).
- Han Nom translation is a pluggable `Translator` trait with `HanNomTranslator` in `translation.rs`. `Session` holds `Option<Box<dyn Translator>>` — pure Vietnamese mode allocates zero Han Nom state.
- `platform.rs` provides cross-platform path resolution via the `dirs` crate (`data_dir`, `config_dir`, `state_dir`, `dictionary_paths`).
- Dictionary loading uses `memmap2` for zero-copy file-based loading, falling back to `include_bytes!` embedded data.
- FFI is unified through `hc_session_handle_key_v4()` with `HC_KeyRequestV2`/`HC_KeyResultV2` types. v1/v2 Han Nom handlers are `#[deprecated]`.
- `linux_fcitx5/` provides the Fcitx5 addon, refactored into `HcImeKeyHandler`, `HcImeCandidateAdapter`, `HcImeStatusMenu` components.
- Macros are shared globally via `OnceLock<Arc<RwLock<HashMap>>>`; phrase history path is a global `OnceLock<PathBuf>`; PhraseHistory is lazy-loaded per session.
- Tests are split into `tests/composition_tests.rs`, `tests/translation_tests.rs`, `tests/session_tests.rs`, `tests/ffi_tests.rs`.
- `scripts/e2e-smoke.sh` is the repo's local end-to-end validation gate.

## Validated Behaviors

- Preedit composition and commit handling are routed through the Rust session engine.
- Tone placement, diacritic transforms, reconversion, and raw-keystroke replay are implemented in the Rust core.
- The addon exposes one configurable HC_IME entry with Telex, VNI, VIQR, and Hán Nôm Telex/VNI/VIQR modes plus behavior toggles.
- Native Fcitx5 config controls input mode, legacy tone placement, spell check, auto-restore, underline behavior, and dictionary paths.
- Fcitx5 exposes native configuration and status-area actions for HC_IME; configure them through `fcitx5-configtool`.
- External dictionary lookups reload when `HC_IME_VI_DICT` or `HC_IME_EN_DICT` changes, so config updates do not stay pinned to the first loaded file.
- The addon can switch between preedit and surrounding-text output using the native Fcitx5 capability checks, and the surrounding-text path uses a UTF-8-safe diff.
- The smoke script verifies Rust tests, addon build/install, metadata, shared library resolution, and FFI exports.
- VNI mode includes specialized handling for English words containing Telex trigger characters (s, f, r, x, j, w, z).
- Hán Nôm core engine and Fcitx5 addon are implemented across all seven planned feature areas (T1.0–T7.3).
- CJK IME UX Alignment: V3 returns up to 256 ranked candidates; paging is delegated to Fcitx5 `CommonCandidateList`.
- Interactive Candidate Navigation: Arrow keys and Tab/Shift+Tab move highlight cursor. Hán Nôm VNI digits always remain tone/shape triggers.
- Hán Nôm phrase prediction: 11,153 validated two-word `(reading, glyphs)` pairs from bundled `HNPH` phrase dictionary.
- Hán Nôm ABI: V3 is additive. V1/V2 structures remain exported but deprecated. V3 borrowed text lasts through next Hán Nôm call.
- Local Hán Nôm learning ranks both single glyphs and phrases, bounded to 2,048 entries with atomic 0600 writes.
- Hán Nôm multi-source data pipeline: `scripts/build_nom_dict.rs` produces 7,079 final readings and 11,153 final phrase pairs.
- Per-app output strategy: `SurroundingTextApps` and `PreeditApps` override global output mode.
- Surrounding-text re-sync guard detects application-side text mutations and recovers cleanly.

## UX Bug Fixes (2026-07-26)

- Telex double-tap toggle-off: `aaa` → `aa` (was `âa`). Circumflex is stripped and literal 'a' emitted.
- VNI diacritic toggle: `a66` → `a6` (was `a`). Digit emitted literally after stripping circumflex.
- OS freeze fix: PhraseHistory lazy-loaded on first Han Nom keystroke, not at session creation.
- Menu ghosting fix: Removed unconditional `updateUserInterface(InputPanel)` from `onMenuActivated()`.
- Candidate layout fix: Removed forced `Horizontal` layout and hardcoded `setPageSize(5)`.

## Architecture Decisions

- [ADR-001](adr/adr-001-separate-composition-translation.md): Separate Composition from Translation
- [ADR-002](adr/adr-002-platform-abstraction-layer.md): Platform Abstraction Layer
- [ADR-003](adr/adr-003-additive-ffi-migration-protocol.md): Additive FFI Migration Protocol
- [ADR-004](adr/adr-004-dictionary-loading-strategy.md): Dictionary Loading Strategy
- [ADR-005](adr/adr-005-shared-vs-per-session-state.md): Shared vs Per-Session State

## Cherry-Picked Features (from VMK + VKey + EVKey analysis)

### Quick Consonant Expansion
- Mid-word: `cc`→`ch`, `gg`→`gi`, `nn`→`ng`, `uu`→`ư`
- Start-of-word: `f`→`ph`, `j`→`gi`, `w`→`qu` (only when followed by vowel)
- End-of-word (on boundary/commit): `g`→`ng`, `h`→`nh`, `k`→`ch`
- Configurable via `QuickConsonants` toggle in Behavior settings

### 3-Tier English Protection
- **Off** (default), **Soft**, **Hard** levels
- Configurable via `EnglishProtection` dropdown.

### Enhanced Macro Expansion
- Macros expand on space/enter/boundary commit
- `MacroInEnglish` toggle allows expansion in English mode
- Format: `key=value` per line. Shared globally via `OnceLock<Arc<RwLock<HashMap>>>`.

### ESC Restore Raw
- When enabled, pressing ESC returns raw keystrokes instead of clearing buffer
- Configurable via `EscRestoreRaw` toggle

### Per-Application Exclusion + Smart Switch
- `ExcludedApps`, `ForcedVnApps`, `SmartSwitch` per-app mode memory

### Non-Preedit Surrounding-Text Mode
- Alternative output using Fcitx5 `deleteSurroundingText()` API
- UTF-8-aware delta computation for incremental updates

### Per-App Output Strategy
- `SurroundingTextApps` and `PreeditApps` override global `OutputMode`

### Surrounding-Text Re-Sync Guard
- Validates current surrounding text before computing diff; recovers on mismatch

## FFI Surface

### Unified v4 (current):
- `HC_KeyRequestV2`: `composition_method` + `translation_target` replacing magic-number `input_mode`
- `HC_KeyResultV2`: unified result with optional candidate data
- `hc_session_handle_key_v4()`: single entry point routing internally

### Retained for backward compat:
- `HC_KeyRequest`, `HC_KeyResult`, `HC_Utf8KeyResult`
- `hc_session_handle_key_utf8()`, `hc_session_handle_key()`
- `hc_session_handle_key_hannom_v3()` (active), v1/v2 (`#[deprecated]`)

### New status flag: `HC_STATUS_ESC_RESTORED_RAW = 4`

## New Dependencies (post-refactoring)

- `dirs = "5"` — cross-platform standard directory resolution (~10 KB)
- `memmap2 = "0.9"` — zero-copy memory-mapped file I/O (~15 KB)

## Remaining Gaps

- macOS IMK frontend (Rust core is cross-platform ready)
- Windows TSF frontend (Rust core is cross-platform ready)
- Custom keymap editor
- Legacy charset output modes beyond Unicode
- Full uinput-based non-preedit mode
- Cross-process smart switch persistence

## Latest Verification

- **2026-07-26**: Completed 9-epic architectural refactoring. `cargo test` passes 143 tests (0 failures, 7 ignored). `scripts/e2e-smoke.sh` passes full gate: rustfmt, dict regen, Cargo test, Clippy, cmake build, bridge probe, installation, metadata, linkage, ABI exports. Refactored from monolithic Session (39 fields) to CompositionEngine (22 fields) + pluggable Translator with unified v4 FFI. Fixed 5 UX bugs (Telex double-tap, VNI toggle, OS freeze, menu ghosting, candidate layout).
- 2026-07-23: Fixed Hán Nôm VNI digit-routing bug. Digits always reach core as composition triggers in HanNomVni mode.
- 2026-07-23: `scripts/e2e-smoke.sh` passed with 145 Rust tests for the candidate/prediction upgrade.

## Related Docs

- [README.md](../README.md)
- [adr/adr-001-separate-composition-translation.md](adr/adr-001-separate-composition-translation.md)
- [adr/adr-002-platform-abstraction-layer.md](adr/adr-002-platform-abstraction-layer.md)
- [adr/adr-003-additive-ffi-migration-protocol.md](adr/adr-003-additive-ffi-migration-protocol.md)
- [adr/adr-004-dictionary-loading-strategy.md](adr/adr-004-dictionary-loading-strategy.md)
- [adr/adr-005-shared-vs-per-session-state.md](adr/adr-005-shared-vs-per-session-state.md)
- [epics.md](epics.md)
- [stories.md](stories.md)
- [IME_RESEARCH_GAPS.md](IME_RESEARCH_GAPS.md)
- [VMK_CHERRYPICK_PLAN.md](VMK_CHERRYPICK_PLAN.md)
- [COMBINED_CHERRYPICK_PLAN.md](COMBINED_CHERRYPICK_PLAN.md)
