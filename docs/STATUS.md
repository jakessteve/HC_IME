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
- The smoke script verifies Rust tests, addon build/install, metadata, shared
  library resolution, and FFI exports.

## Remaining Gaps

- Macro editor and macro expansion.
- Custom keymap editor.
- Legacy charset output modes beyond Unicode.
- Per-application mode or exclusion behavior.

## Related Docs

- [README.md](../README.md)
- [IME_RESEARCH_GAPS.md](IME_RESEARCH_GAPS.md)
