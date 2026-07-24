#!/usr/bin/env bash
# HC_IME update entry point.
#
# For a machine that already ran scripts/install.sh once: rebuilds the Rust core
# and the Fcitx5 addon from the current source, reinstalls them over the previous
# installation, and restarts Fcitx5. It does not touch apt packages, fonts, or
# anything under ~/.config — those survive a code change, so re-doing them is
# both slow and a way to overwrite settings you have since adjusted.
#
# On Debian, Ubuntu, and derivatives it hands off to install-debian.sh --update.
# On everything else it prints the equivalent manual commands.
#
# Works run either way; arguments are passed straight through:
#   ./scripts/update.sh [options]
#   sudo ./scripts/update.sh [options]
#
# Useful options:
#   --skip-tests   Skip `cargo test` before rebuilding.
#   --force        Reinstall even when the rebuild produced no change.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$ROOT/build-install}"

if [[ -t 1 ]]; then
    C_RESET=$'\033[0m'; C_BOLD=$'\033[1m'
else
    C_RESET=""; C_BOLD=""
fi

ID=""; ID_LIKE=""; PRETTY_NAME=""
if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
fi

is_debian_family() {
    [[ "$ID" == "debian" || "$ID" == "ubuntu" || "$ID_LIKE" == *debian* ]]
}

manual_steps() {
    cat >&2 <<EOF

${C_BOLD}HC_IME has no automated updater for this system.${C_RESET}
Detected: ${PRETTY_NAME:-${ID:-unknown}}

Dependencies and your Fcitx5 configuration are already in place from the first
install, so a code change only needs a rebuild and a reinstall. From the
repository root:

    cargo test --manifest-path hc_core/Cargo.toml
    cmake --build $BUILD_DIR
    sudo cmake --install $BUILD_DIR
    fcitx5 -r

The build directory is reused, so only what changed is recompiled. Stop Fcitx5
(or accept the restart \`fcitx5 -r\` does) before installing: the install
overwrites libhcime.so and libhc_core.so in place, and a running daemon has
them mapped.

If $BUILD_DIR does not exist yet, configure it once:

    cmake -S . -B $BUILD_DIR -G Ninja \\
        -DCMAKE_BUILD_TYPE=Release -DFCITX_INSTALL_USE_FCITX_SYS_PATHS=ON
EOF
}

if is_debian_family; then
    exec "$HERE/install-debian.sh" --update "$@"
else
    manual_steps
    exit 1
fi
