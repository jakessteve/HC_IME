# Vietnamese IME Research Gaps

This note records the comparison that drove the current HC_IME upgrade.

## Similar Projects Reviewed

- UniKey: small trusted Vietnamese IME core; supports Telex, VNI, VIQR and many
  output encodings.
- IBus Bamboo: supports many Vietnamese encodings, many input methods,
  spell-checking, auto-restore, free tone marking, macro support, emoji, and
  application typing-mode switching.
- Fcitx5 Bamboo: exposes input-method selection, charset selection,
  spell-check toggles, macro toggles, and status-area actions through Fcitx5.
- Fcitx5 UniKey: exposes broad Fcitx5 configuration options for input method,
  charset, spell checking, macro expansion, and display behavior.
- VMK (thanhpy2009): Fcitx5 addon wrapping Bamboo engine with non-preedit
  backspace-replay via uinput daemon. Key features: VMK1/VMK1HC/VMK2/VMK-Pre
  modes, mouse-click reset, browser-specific fixes, single-file installer.
- VKey (phatMT97): Windows TSF-based Vietnamese IME in C++20. Key features:
  Combined Telex+VNI mode, quick consonant expansion, 3-tier English
  protection, per-app mode with smart switch, feature pipeline with gates,
  engine rule plugins, output strategy per app type, RCU config reload,
  ESC restore raw, macro up to 20K chars.

## Gaps Found In HC_IME (Original)

- No native config UI in `fcitx5-configtool`.
- No user-visible toggles for spell checking, auto-restore, or legacy tone
  placement.
- Telex/VNI/VIQR were not selectable from the HC_IME entry itself.
- Dictionary paths were implicit rather than visible/auditable.
- No one-command E2E gate verifying config metadata, input methods, staged
  install, shared-library resolution, and FFI exports.
- No macro editor, custom keymap editor, legacy charset conversion, status-area
  quick actions, or per-application mode UI yet.

## Upgrade Implemented (Phase 1)

- Added native Fcitx5 configuration for:
  - Input mode selection: Telex, VNI, or VIQR
  - Legacy tone placement
  - Spell-check/dictionary validation
  - Auto-restore raw keystrokes for invalid Vietnamese sequences
  - Preedit underline
  - Vietnamese dictionary path
  - English dictionary path
- Marked the addon configurable and kept one `HC_IME` input method with a
  selectable input mode.
- Added a Bamboo-like HC_IME status-area menu with runtime toggles.
- Passed these config toggles into the Rust core through the FFI request.
- Added tests for the config-controlled engine behavior.
- Extended the E2E smoke path to verify Linux install readiness.

## Upgrade Implemented (Phase 2 — Cherry-Pick from VMK + VKey)

### From VKey:
- **Quick Consonant Expansion**: cc→ch, gg→gi, nn→ng, uu→ư (mid-word);
  f→ph, j→gi, w→qu (start-of-word); g→ng, h→nh, k→ch (end-of-word)
- **3-Tier English Protection**: Off/Soft/Hard levels for rejecting
  impossible Vietnamese patterns
- **Enhanced Macro Expansion**: macro_in_english toggle, newline support
- **ESC Restore Raw**: ESC returns original keystrokes instead of clearing
- **Smart Switch**: Per-app Vietnamese/English mode memory

### From VMK:
- **Per-App Exclusion**: Force English or Vietnamese mode per application
- **Non-Preedit Surrounding-Text Mode**: Alternative output using
  deleteSurroundingText API (no root required)

### Architecture Changes:
- Extended `HC_KeyRequest` with 4 new feature flags
- Added `HC_STATUS_ESC_RESTORED_RAW` status flag
- Added `quick_consonants.rs` module
- Added `EnglishProtectionLevel` enum and 3-tier protection functions
- Added per-app config sections (ExcludedApps, ForcedVnApps, SmartSwitch)
- Added output mode config (Preedit/SurroundingText)
- Status area expanded from 4 to 7 behavior toggles

## Remaining Larger Parity Work

- Custom keymap editor
- Legacy charset output modes beyond Unicode
- Full uinput-based non-preedit mode (requires root daemon, like VMK)
- Cross-process smart switch persistence (currently per-session only)
- Combined Telex+VNI mode (intentionally excluded — user preference)
- Macro editor UI (currently file-based only)
