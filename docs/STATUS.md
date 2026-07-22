# HC_IME Status

This document is the local source of truth for the current repo snapshot.
It reflects the validated state of the codebase at the time of the latest
cleanup and documentation sync.

## Current Shape

- Rust core in `hc_core/` implements Telex, VNI, and VIQR composition.
- `linux_fcitx5/` provides the Fcitx5 addon, metadata, and install rules.
- `scripts/e2e-smoke.sh` is the repo's local end-to-end validation gate.
- `README.md` describes the architecture, build flow, native UI, and packaging.

## Validated Behaviors

- Preedit composition and commit handling are routed through the Rust session
  engine.
- Tone placement, diacritic transforms, reconversion, and raw-keystroke replay
  are implemented in the Rust core.
- The addon exposes a single configurable HC_IME entry with a selectable
  Telex/VNI/VIQR mode and behavior toggles.
- Native Fcitx5 config controls input mode, legacy tone placement, spell
  check, auto-restore, underline behavior, and dictionary paths.
- The status menu includes a `Settings` launcher that opens the native Fcitx5
  configuration tool for HC_IME.
- External dictionary lookups reload when `HC_IME_VI_DICT` or `HC_IME_EN_DICT`
  changes, so config updates do not stay pinned to the first loaded file.
- The addon can switch between preedit and surrounding-text output using the
  native Fcitx5 capability checks, and the surrounding-text path now replaces
  prior composition text with a UTF-8-safe diff instead of appending it.
- The smoke script verifies Rust tests, addon build/install, metadata, shared
  library resolution, and FFI exports.
- The latest validated e2e sweep passed after the dictionary-cache fix and the
  surrounding-text bridge cleanup.
- VNI mode includes specialized handling for English words containing Telex 
  trigger characters (s, f, r, x, j, w, z) to prevent cross-contamination and
  false diacritic application during composition.
- Hán Nôm core engine & Fcitx5 addon feature fully implemented across all 7 Epics (Tasks T1.0 - T7.3):
  - Data Pipeline (E1): Parsed Unihan, NomStandardization, cake_gao, pearapple123 into binary format v1 `hc_core/data/han_nom_dict.bin` (7,114 unique readings, 19,134 unique characters, 14,297 Extension B+ Nôm characters).
  - Composition Engine Refactor (E2): Extracted `TypingEngine` in `hc_core/src/compose.rs` supporting `Inline` & `Dictionary` composition modes.
  - Core Nom Module (E3): Implemented dual-phase engine (`Reading` & `Candidate` phases), exact & toneless lookups, and candidate pagination.
  - FFI Layer (E4): Added `hc_session_handle_key_hannom` and `hc_nom_dict_status` to Rust C ABI & `hc_core_ffi.h`.
  - Fcitx5 Addon Integration (E5): Extended `HcImeInputMode` enum (`HanNomTelex`, `HanNomVni`, `HanNomViqr`), wired `CommonCandidateList` UI, and updated status menu.
  - Validation & Tests (E6): Added 116 Rust unit tests (stress test, Ext B+ safety, error fallback) and bridge probe test suite.
  - Verification (E7): 100% test pass rate across `cargo test` and `./scripts/e2e-smoke.sh`.
- TypingEngine extracted into `hc_core/src/compose.rs` with `CompositionMode` support (Inline and Dictionary modes).
- Hán Nôm multi-source data pipeline is active: `scripts/build_nom_dict.rs` parses Unihan, NomStandardization, cake_gao, and pearapple123, producing a validated binary dictionary at `hc_core/data/han_nom_dict.bin` with 7,114 unique readings and 19,134 unique characters (14,297 Extension B+ Nôm characters).
- Per-app output strategy overrides the global output mode: apps listed in
  `SurroundingTextApps` always use surrounding-text output, and apps listed in
  `PreeditApps` always use client preedit, regardless of the global setting.
- The surrounding-text path includes a re-sync guard that detects when the
  application modifies surrounding text behind the IME and recovers cleanly
  by committing the new preedit directly instead of computing a stale diff.

## Cherry-Picked Features (from VMK + VKey + EVKey analysis)

### Quick Consonant Expansion
- Mid-word: `cc`→`ch`, `gg`→`gi`, `nn`→`ng`, `uu`→`ư`
- Start-of-word: `f`→`ph`, `j`→`gi`, `w`→`qu` (only when followed by vowel)
- End-of-word (on boundary/commit): `g`→`ng`, `h`→`nh`, `k`→`ch`
- Configurable via `QuickConsonants` toggle in Behavior settings
- Lock mechanism prevents double-expansion after initial trigger

### 3-Tier English Protection
- **Off** (default): No English protection, standard language scoring
- **Soft**: Rejects ambiguous patterns like `y`+vowel at word start
- **Hard**: Rejects impossible Vietnamese start clusters (cl, cr, br, etc.)
- Configurable via `EnglishProtection` dropdown (Off/Soft/Hard)
- Integrated into spell-check status pipeline

### Enhanced Macro Expansion
- Macros expand on space/enter/boundary commit
- `MacroInEnglish` toggle allows expansion even when classified as English
- Macro file loaded from configurable path (supports `~` expansion)
- Format: `key=value` per line, comments with `#`
- Existing `hc_session_add_macro` / `hc_session_clear_macros` FFI preserved

### ESC Restore Raw
- When enabled, pressing ESC during composition returns the raw keystrokes
  instead of clearing the buffer
- New `HC_STATUS_ESC_RESTORED_RAW` status flag (value 4)
- Configurable via `EscRestoreRaw` toggle
- Useful for recovering original input when transforms go wrong

### Per-Application Exclusion + Smart Switch
- **ExcludedApps**: List of app names forced to English mode
- **ForcedVnApps**: List of app names forced to Vietnamese mode
- **SmartSwitch**: Remembers Vietnamese/English mode per app based on
  commit history (English fallback → English mode, normal commit → Vietnamese)
- Precedence: ExcludedApps > ForcedVnApps > SmartSwitch > Global
- App detection via Fcitx5 `InputContext::program()`

### Non-Preedit Surrounding-Text Mode
- Alternative output mode using Fcitx5 `deleteSurroundingText()` API
- No root/daemon required (unlike VMK's uinput approach)
- Tracks the previously inserted text and computes a UTF-8-aware delta for
  incremental updates
- Configurable via `OutputMode` setting (Preedit/SurroundingText)
- Falls back to standard preedit when surrounding text is unavailable

### Per-App Output Strategy (from EVKey analysis)
- `SurroundingTextApps`: List of app names forced to use surrounding-text output
- `PreeditApps`: List of app names forced to use client preedit output
- Per-app lists override the global `OutputMode` setting
- Uses the same case-insensitive substring matching as `ExcludedApps`
- Configured under the `PerApp` section in `hcime.conf`

### Surrounding-Text Re-Sync Guard
- Validates that the current surrounding text ends with the previously
  inserted preedit before computing a diff
- On mismatch (app-side edit, auto-correct, or focus change), clears the
  tracked state and commits the new preedit directly
- Prevents stale `deleteSurroundingText` calls that would corrupt text

## FFI Surface

The `HC_KeyRequest` struct now includes:
- `quick_consonants: u8` — enable quick consonant expansion
- `english_protection: u8` — 0=Off, 1=Soft, 2=Hard
- `macro_in_english: u8` — allow macro expansion in English mode
- `esc_restore_raw: u8` — ESC returns raw keystrokes

New status flag: `HC_STATUS_ESC_RESTORED_RAW = 4`

New borrowed-output ABI:
- `HC_Utf8KeyResult`
- `hc_session_handle_key_utf8()` returns borrowed UTF-8 bytes valid until the
  next key-result call on the same thread

## Remaining Gaps

- Custom keymap editor
- Legacy charset output modes beyond Unicode
- Full uinput-based non-preedit mode (requires root daemon, like VMK)
- Cross-process smart switch persistence (currently per-session only)

## Related Docs

- [README.md](../README.md)
- [IME_RESEARCH_GAPS.md](IME_RESEARCH_GAPS.md)
- [VMK_CHERRYPICK_PLAN.md](VMK_CHERRYPICK_PLAN.md)
- [COMBINED_CHERRYPICK_PLAN.md](COMBINED_CHERRYPICK_PLAN.md)
