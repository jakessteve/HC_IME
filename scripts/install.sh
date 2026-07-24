#!/usr/bin/env bash
# HC_IME installer entry point.
#
# Detects the distribution and dispatches to the right installer. On Debian,
# Ubuntu, and derivatives it hands off to install-debian.sh (apt, fully
# automated). On everything else it prints the exact manual dependency +
# build steps and points at README.md.
#
# Works run either way; arguments are passed straight through:
#   ./scripts/install.sh [options]
#   sudo ./scripts/install.sh [options]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

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

${C_BOLD}HC_IME has no automated installer for this system.${C_RESET}
Detected: ${PRETTY_NAME:-${ID:-unknown}}

Install the equivalents by hand, then follow the "Manual build and install"
section of README.md:

EOF
    case "$ID $ID_LIKE" in
        *fedora*|*rhel*)
            cat >&2 <<'EOF'
  Fedora / RHEL:
    sudo dnf install cmake ninja-build gcc-c++ extra-cmake-modules \
        fcitx5-devel fcitx5-configtool rust cargo \
        google-noto-sans-cjk-fonts
EOF
            ;;
        *arch*)
            cat >&2 <<'EOF'
  Arch:
    sudo pacman -S cmake ninja base-devel extra-cmake-modules fcitx5 \
        fcitx5-configtool rust noto-fonts-cjk
EOF
            ;;
        *suse*)
            cat >&2 <<'EOF'
  openSUSE:
    sudo zypper install cmake ninja gcc-c++ extra-cmake-modules \
        fcitx5-devel fcitx5-configtool rust cargo \
        noto-sans-cjk-fonts
EOF
            ;;
        *)
            cat >&2 <<'EOF'
  Generic: install these (names vary by distribution):
    - build toolchain:  a C++20 compiler, cmake, ninja, extra-cmake-modules,
                        pkg-config, gettext
    - Fcitx5 dev + tool: Fcitx5 core/config/utils development files,
                        fcitx5-configtool
    - Rust:             rustc + cargo (1.70+), or rustup
    - CJK fonts:        Noto CJK plus a CJK Extension B font (e.g. Hanazono)
EOF
            ;;
    esac

    cat >&2 <<'EOF'

Then, from the repository root:
    cargo test --manifest-path hc_core/Cargo.toml
    cmake -S . -B build -G Ninja -DFCITX_INSTALL_USE_FCITX_SYS_PATHS=ON
    cmake --build build
    sudo cmake --install build
    fcitx5 -r
EOF
}

if is_debian_family; then
    exec "$HERE/install-debian.sh" "$@"
else
    manual_steps
    exit 1
fi
