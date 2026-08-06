#!/usr/bin/env bash
# Builds the HC_IME macOS InputMethodKit frontend into HCIME.app.
#
#   ./build.sh            build only
#   ./build.sh --install  build, then install into ~/Library/Input Methods
#
# This is a test frontend for the Rust core. It does not replace the Fcitx5
# addon, which remains the shipping Linux path.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HERE="$ROOT/macos_imk"
BUILD="$HERE/build"
APP="$BUILD/HCIME.app"
INSTALL_DIR="$HOME/Library/Input Methods"

INSTALL=0
[[ "${1:-}" == "--install" ]] && INSTALL=1

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: this frontend only builds on macOS" >&2
    exit 1
fi

echo "==> Building Rust core (release)"
cargo build --release --manifest-path "$ROOT/hc_core/Cargo.toml"

DYLIB="$ROOT/hc_core/target/release/libhc_core.dylib"
[[ -f "$DYLIB" ]] || { echo "error: $DYLIB not found" >&2; exit 1; }

echo "==> Assembling bundle"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp "$HERE/Resources/Info.plist" "$APP/Contents/Info.plist"
cp "$HERE/Resources/HCIME.tiff" "$APP/Contents/Resources/HCIME.tiff"
cp "$DYLIB" "$APP/Contents/Frameworks/libhc_core.dylib"

# The Rust cdylib is built with an absolute install name. Rewrite it to an
# @rpath reference so the bundled copy is the one that loads.
install_name_tool -id "@rpath/libhc_core.dylib" "$APP/Contents/Frameworks/libhc_core.dylib"

echo "==> Compiling Swift"
swiftc \
    -O \
    -target arm64-apple-macos12.0 \
    -sdk "$(xcrun --show-sdk-path)" \
    -import-objc-header "$HERE/Sources/Bridging.h" \
    -framework Cocoa \
    -framework InputMethodKit \
    -L "$ROOT/hc_core/target/release" \
    -lhc_core \
    -Xlinker -rpath -Xlinker "@executable_path/../Frameworks" \
    -o "$APP/Contents/MacOS/HCIME" \
    "$HERE/Sources/HCCore.swift" \
    "$HERE/Sources/HanNomFont.swift" \
    "$HERE/Sources/HCIMEApplication.swift" \
    "$HERE/Sources/HCIMEInputController.swift" \
    "$HERE/Sources/main.swift"

# Point the executable at the bundled dylib rather than the cargo target dir.
install_name_tool -change \
    "$(otool -D "$DYLIB" | tail -1)" \
    "@rpath/libhc_core.dylib" \
    "$APP/Contents/MacOS/HCIME" 2>/dev/null || true

# Sign inside-out: --deep is deprecated and can leave nested code mis-sealed.
echo "==> Signing (ad-hoc)"
codesign --force --sign - "$APP/Contents/Frameworks/libhc_core.dylib"
codesign --force --sign - "$APP"
codesign --verify --strict "$APP"

echo "==> Verifying linkage"
otool -L "$APP/Contents/MacOS/HCIME" | grep -q "@rpath/libhc_core.dylib" \
    || { echo "error: executable does not reference the bundled core" >&2; exit 1; }

# Drive the core through the same bridging header and wrapper the controller
# uses. A green bundle with a broken bridge is worse than a build failure.
echo "==> Running FFI probe"
PROBE_BIN="$BUILD/hc-probe"
swiftc \
    -O \
    -target arm64-apple-macos12.0 \
    -sdk "$(xcrun --show-sdk-path)" \
    -import-objc-header "$HERE/Sources/Bridging.h" \
    -framework Cocoa \
    -framework InputMethodKit \
    -L "$ROOT/hc_core/target/release" \
    -lhc_core \
    -Xlinker -rpath -Xlinker "$ROOT/hc_core/target/release" \
    -o "$PROBE_BIN" \
    "$HERE/Sources/HCCore.swift" \
    "$HERE/Sources/HanNomFont.swift" \
    "$HERE/Tools/main.swift"
"$PROBE_BIN"

echo "Built $APP"

if [[ $INSTALL -eq 1 ]]; then
    echo "==> Installing to $INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"
    # Stop a previously running copy so the new binary is picked up.
    pkill -x HCIME 2>/dev/null || true
    rm -rf "$INSTALL_DIR/HCIME.app"
    cp -R "$APP" "$INSTALL_DIR/HCIME.app"

    # Registering avoids a logout; if it fails, the login scan still picks it up.
    "$INSTALL_DIR/HCIME.app/Contents/MacOS/HCIME" --register || true

    echo
    echo "Installed. To activate:"
    echo "  1. System Settings > Keyboard > Text Input > Input Sources > Edit… > +"
    echo "  2. Choose Vietnamese, then an HC_IME mode (Telex / VNI / VIQR / Hán Nôm)"
    echo "  3. Switch to it from the input menu (or Ctrl+Space) and type in any app"
    echo
    echo "If HC_IME does not appear in the list, log out and back in — macOS"
    echo "rescans ~/Library/Input Methods at login."
fi
