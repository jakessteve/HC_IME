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

## Gaps Found In HC_IME

- No native config UI in `fcitx5-configtool`.
- No user-visible toggles for spell checking, auto-restore, or legacy tone
  placement.
- Telex/VNI/VIQR were installable as separate input methods, but not
  configurable.
- Dictionary paths were implicit rather than visible/auditable.
- No one-command E2E gate verifying config metadata, input methods, staged
  install, shared-library resolution, and FFI exports.
- No macro editor, custom keymap editor, legacy charset conversion, status-area
  quick actions, or per-application mode UI yet.

## Upgrade Implemented

- Added native Fcitx5 configuration for:
  - Input method selection
  - Legacy tone placement
  - Spell-check/dictionary validation
  - Auto-restore raw keystrokes for invalid Vietnamese sequences
  - Preedit underline
  - Vietnamese dictionary path
  - English dictionary path
- Marked the addon and Telex/VNI/VIQR input methods as configurable.
- Added a Bamboo-like HC_IME status-area menu with mode switching and runtime
  toggles.
- Passed these config toggles into the Rust core through the FFI request.
- Added tests for the config-controlled engine behavior.
- Extended the E2E smoke path to verify Linux install readiness.

## Remaining Larger Parity Work

- Macro expansion and macro editor.
- Custom keymap editor.
- Legacy charset output modes beyond Unicode.
- Per-application mode/exclusion behavior.
