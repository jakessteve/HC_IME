#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-"$ROOT/build-e2e"}"
DESTDIR_PATH="${DESTDIR_PATH:-/tmp/hcime-e2e-install}"

cd "$ROOT"

rustfmt --check hc_core/src/*.rs
BUILDER_TMP="$(mktemp -d)"
rustc scripts/build_nom_dict.rs -O -o "$BUILDER_TMP/build_nom_dict"
"$BUILDER_TMP/build_nom_dict" --output-dir "$BUILDER_TMP/data" >/dev/null
cmp "$BUILDER_TMP/data/han_nom_dict.bin" hc_core/data/han_nom_dict.bin
cmp "$BUILDER_TMP/data/han_nom_phrase_dict.bin" hc_core/data/han_nom_phrase_dict.bin
cargo test --manifest-path hc_core/Cargo.toml

if command -v cargo-clippy >/dev/null 2>&1; then
  cargo-clippy clippy --manifest-path hc_core/Cargo.toml -- -D warnings
elif cargo clippy --version >/dev/null 2>&1; then
  cargo clippy --manifest-path hc_core/Cargo.toml -- -D warnings
else
  echo "warning: clippy is not installed; skipping lint gate" >&2
fi

cmake -S . -B "$BUILD_DIR" -G Ninja -DFCITX_INSTALL_USE_FCITX_SYS_PATHS=ON
cmake --build "$BUILD_DIR"

BRIDGE_PROBE="$BUILD_DIR/hcime-bridge-probe"
HC_CORE_RELEASE="$BUILD_DIR/linux_fcitx5/cargo-target/release"
"${CXX:-c++}" -std=c++20 linux_fcitx5/tests/bridge_probe.cpp \
  -o "$BRIDGE_PROBE" \
  -Ilinux_fcitx5/include \
  $(pkg-config --cflags --libs Fcitx5Core Fcitx5Config Fcitx5Utils) \
  -L"$HC_CORE_RELEASE" -lhc_core \
  -Wl,-rpath,"$HC_CORE_RELEASE"
"$BRIDGE_PROBE"

rm -rf "$DESTDIR_PATH"
DESTDIR="$DESTDIR_PATH" cmake --install "$BUILD_DIR"

ADDON="$DESTDIR_PATH/usr/lib/fcitx5/libhcime.so"
CORE="$DESTDIR_PATH/usr/lib/fcitx5/libhc_core.so"

test -f "$ADDON"
test -f "$CORE"
test -f "$DESTDIR_PATH/usr/share/fcitx5/addon/hcime.conf"
test -f "$DESTDIR_PATH/usr/share/fcitx5/inputmethod/hcime.conf"
test ! -f "$DESTDIR_PATH/usr/share/fcitx5/inputmethod/hcime-telex.conf"
test ! -f "$DESTDIR_PATH/usr/share/fcitx5/inputmethod/hcime-vni.conf"
test ! -f "$DESTDIR_PATH/usr/share/fcitx5/inputmethod/hcime-viqr.conf"
grep -q '^Configurable=True$' "$DESTDIR_PATH/usr/share/fcitx5/addon/hcime.conf"
grep -q '^Configurable=True$' "$DESTDIR_PATH/usr/share/fcitx5/inputmethod/hcime.conf"

LD_LIBRARY_PATH="$DESTDIR_PATH/usr/lib/fcitx5" ldd "$ADDON" | grep -q "$CORE"
readelf -d "$ADDON" | grep -q 'RUNPATH.*\$ORIGIN'
nm -D "$CORE" | grep -q 'hc_session_handle_key'
nm -D "$CORE" | grep -q 'hc_session_handle_key_hannom_v2'
nm -D "$CORE" | grep -q 'hc_session_handle_key_hannom_v3'
nm -D "$CORE" | grep -q 'hc_session_select_hannom_candidate_v3'
nm -D "$CORE" | grep -q 'hc_compose_with_request'
nm -D "$CORE" | grep -q 'hc_rehydrate_apply'
grep -a -q 'HC_IME' "$ADDON"
grep -a -q 'Toggle Vietnamese word validation' "$ADDON"
grep -a -q 'Toggle raw-keystroke restore' "$ADDON"
grep -a -q 'Toggle preedit underline' "$ADDON"
grep -a -q 'hcime-toggle-spell-check' "$ADDON"
grep -a -q 'hcime-toggle-preedit-underline' "$ADDON"
grep -a -q 'hcime-mode-telex' "$ADDON"
grep -a -q 'hcime-mode-vni' "$ADDON"
grep -a -q 'hcime-mode-viqr' "$ADDON"

echo "HC_IME e2e smoke passed: $DESTDIR_PATH"
