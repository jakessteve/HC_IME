#!/usr/bin/env bash
# HC_IME uninstall entry point.
#
# Walks through what HC_IME put on this machine and lets you take off only the
# parts you want: the addon, its Fcitx5 profile entry, the candidate-window font,
# the environment drop-in, the Hán Nôm font packages.
#
# It never removes Fcitx5, and never removes an apt package it did not install
# itself — a machine that already had Fcitx5 keeps it. What it may remove is
# recorded during the install, so a package that was here first is left alone.
#
# Works run either way; arguments are passed straight through:
#   ./scripts/uninstall.sh
#   sudo ./scripts/uninstall.sh
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

${C_BOLD}HC_IME has no automated uninstaller for this system.${C_RESET}
Detected: ${PRETTY_NAME:-${ID:-unknown}}

Remove it by hand. The installed files are listed in the manifest kept at:

    ~/.local/share/hcime/install_manifest.txt

    sudo xargs -d '\n' -a ~/.local/share/hcime/install_manifest.txt rm -f --

Then, if you want the configuration gone too:

  - remove the "hcime" entry from ~/.config/fcitx5/profile
  - restore ~/.config/fcitx5/conf/classicui.conf from its .hcime-backup-* copy
  - delete ~/.config/environment.d/90-hcime-fcitx5.conf
  - restart Fcitx5:  fcitx5 -r

Leave Fcitx5 itself installed unless you are sure nothing else uses it.
EOF
}

if is_debian_family; then
    exec "$HERE/install-debian.sh" --uninstall "$@"
else
    manual_steps
    exit 1
fi
