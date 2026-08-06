# HC_IME — macOS InputMethodKit frontend

A native macOS input method that drives the same `hc_core` Rust engine as the
Fcitx5 addon, so you can type Vietnamese and Hán Nôm into any macOS app.

This is a **test frontend**. Fcitx5 on Linux remains the shipping path; this
exists to exercise the engine on a second platform and to make the core's
behaviour observable outside a Linux VM.

## Two ways to run it

**Standalone tester — no install, no logout, no permissions:**

```bash
./macos_imk/run-tester.sh
```

Opens a window with a mode picker, behaviour toggles, a text area and a
candidate strip. Keystrokes go through the same `HCSession` wrapper and the
same key classification as the real input method, so what you see here is what
the installed input method does. This is the fast loop for testing engine
changes.

**System-wide input method — types into every app, needs a logout once:**

```bash
./macos_imk/build.sh --install
```

See *Activation* below. macOS only scans `~/Library/Input Methods` at login,
which is why the standalone tester exists.

## Build

```bash
./macos_imk/build.sh              # build + FFI probe, no install
./macos_imk/build.sh --install    # also install to ~/Library/Input Methods
./macos_imk/run-tester.sh         # build + launch the standalone tester
./macos_imk/run-tester.sh --selftest   # headless: assert what actually renders
```

`--selftest` drives the tester's text view without a window and prints the
resulting document. Use it when the window looks wrong: it separates an engine
fault from a rendering fault in one run.

Requires macOS on Apple silicon, Xcode Command Line Tools, and Rust. The build
compiles `hc_core` as a `cdylib`, bundles it under `Contents/Frameworks`,
ad-hoc signs the result, and runs an FFI probe before declaring success.

## Activation

After `--install`, macOS needs to pick the bundle up:

1. **Log out and back in.** macOS scans `~/Library/Input Methods` when a login
   session starts. `build.sh` calls `TISRegisterInputSource` to try to skip
   this, but for input-method bundles that call is unreliable — if HC_IME is
   not in the list below, the logout is required.
2. System Settings → Keyboard → Text Input → Input Sources → **Edit…** → **+**
3. Pick **Vietnamese**, then one of:
   - HC_IME Telex / VNI / VIQR
   - HC_IME Hán Nôm (Telex / VNI / VIQR)
4. Switch with the input menu or Ctrl+Space, then type.

To confirm registration:

```bash
./macos_imk/build.sh   # then check the input menu, or:
osascript -e 'tell application "System Events" to get name of every process'
```

## What works

| Area | Status |
| --- | --- |
| Telex / VNI / VIQR composition | Preedit, commit, tone placement, English restore |
| Marked text | Underlined preedit via `setMarkedText` |
| Delimiters | Space / punctuation commit the word, then reach the app |
| Hán Nôm | Candidate panel with keyboard navigation, digit selection (Telex/VIQR), phrase prediction |
| Learning | Local ranking persists to `~/Library/Application Support/hcime` |
| Settings | Per-toggle menu on the input-method menu item |
| Macros | `~/Library/Application Support/hcime/macros.txt`, `key=replacement` |

Hán Nôm VNI keeps digits as tone/shape triggers, matching the core and the
Fcitx5 addon, so digits 1–9 select a candidate in Telex and VIQR only. Every
mode navigates the panel with ↓/→/Tab (next), ↑/←/Shift+Tab (previous),
Page Down / Page Up, and Enter to take the highlighted candidate — the same
keys the Fcitx5 addon binds.

## Layout

```
macos_imk/
  Sources/Bridging.h                  includes the shared C ABI header
  Sources/HCCore.swift                Swift wrapper over hc_session_* / v4 FFI
  Sources/HCIMEInputController.swift  IMKInputController: key routing, preedit
  Sources/main.swift                  IMKServer bootstrap, --register
  Tester/main.swift                   standalone tester window
  Tools/main.swift                    FFI probe run by build.sh
  Resources/Info.plist                input modes, IMK wiring
  Resources/HCIME.tiff                template menu-bar icon
  build.sh                            input method: build / install
  run-tester.sh                       standalone tester: build / launch
```

`HCCore.swift` is shared by all three targets, so the tester and the probe
exercise the real bridge rather than a mock.

`Bridging.h` includes `linux_fcitx5/include/hcime/hc_core_ffi.h` rather than
copying it. The ABI has exactly one definition; if Rust changes, both frontends
break at compile time instead of one drifting silently.

## Notes and limits

- **arm64 only.** `build.sh` targets `arm64-apple-macos12.0`. Add
  `-target x86_64-apple-macos12.0` and `lipo` for a universal build.
- **Ad-hoc signed, not notarized.** `spctl` will report the bundle as rejected;
  that is expected and does not prevent an input method in
  `~/Library/Input Methods` from loading.
- **No surrounding-text mode.** The Fcitx5 addon can output via
  `deleteSurroundingText()`; this frontend uses marked text only.
- **Option is a command modifier**, as Alt is in the addon's
  `HasCommandModifier`. Option-typed characters go to the application rather
  than into the Vietnamese buffer, so Option+letter shortcuts and
  Option+Backspace (delete previous word) keep working.
- **Hán Nôm needs CJK Extension B fonts** (NomNaTong, HanaMinA/B) or candidates
  render as □.
- The core writes a startup line to stderr (`[HC_IME] Using embedded nom dict`),
  which lands in the system log.

## Verifying the bridge without installing

```bash
./macos_imk/build.sh   # runs the probe as part of the build
```

The probe drives `HCSession` — the same wrapper the controller uses — through
Telex, VNI, VIQR and a Hán Nôm candidate lookup. If it passes but typing
misbehaves, the fault is in the InputMethodKit glue, not the FFI.
