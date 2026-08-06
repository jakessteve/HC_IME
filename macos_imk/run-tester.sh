#!/usr/bin/env bash
# Builds and launches the standalone HC_IME tester window.
#
#   ./run-tester.sh
#
# No input-method install, no logout, no permissions. Type in the window and
# the keystrokes go through the same hc_core engine and the same key
# classification the installed input method uses.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HERE="$ROOT/macos_imk"
BUILD="$HERE/build"
BIN="$BUILD/HCIMETester"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: the tester only builds on macOS" >&2
    exit 1
fi

echo "==> Building Rust core (release)"
cargo build --release --manifest-path "$ROOT/hc_core/Cargo.toml"

mkdir -p "$BUILD"

echo "==> Compiling tester"
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
    -o "$BIN" \
    "$HERE/Sources/HCCore.swift" \
    "$HERE/Sources/HanNomFont.swift" \
    "$HERE/Sources/HCIMEApplication.swift" \
    "$HERE/Sources/HCIMEInputController.swift" \
    "$HERE/Tester/ControllerHarness.swift" \
    "$HERE/Tester/main.swift"

# `./run-tester.sh --selftest` drives the view headlessly and prints what
# actually landed in the text storage — use it when the window looks wrong, to
# separate an engine fault from a rendering fault. It then drives the shipping
# HCIMEInputController against a mock IMKTextInput client, so a controller
# regression fails here and not only in an installed input method.
echo "==> Launching"
exec "$BIN" "$@"
