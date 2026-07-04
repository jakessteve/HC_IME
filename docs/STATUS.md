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
- External dictionary lookups reload when `HC_IME_VI_DICT` or `HC_IME_EN_DICT`
  changes, so config updates do not stay pinned to the first loaded file.
- The smoke script verifies Rust tests, addon build/install, metadata, shared
  library resolution, and FFI exports.
- The latest validated e2e sweep passed after the dictionary-cache fix.

## Cherry-Picked Features (from VMK + VKey analysis)

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
- Tracks previous preedit text and computes diff for incremental updates
- Configurable via `OutputMode` setting (Preedit/SurroundingText)
- Falls back to standard preedit when surrounding text is unavailable

## FFI Surface

The `HC_KeyRequest` struct now includes:
- `quick_consonants: u8` — enable quick consonant expansion
- `english_protection: u8` — 0=Off, 1=Soft, 2=Hard
- `macro_in_english: u8` — allow macro expansion in English mode
- `esc_restore_raw: u8` — ESC returns raw keystrokes

New status flag: `HC_STATUS_ESC_RESTORED_RAW = 4`

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
