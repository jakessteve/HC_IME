#!/usr/bin/env bash
# HC_IME guided installer for Debian, Ubuntu, and their derivatives.
#
# apt-only by design: it refuses to run where /etc/os-release is not in the
# Debian family. On Fedora, Arch, or anything else, follow the "Build and
# Install" section of README.md (scripts/install.sh prints the exact commands).
#
# It takes a blank machine — no Rust, no Fcitx5, no CJK fonts — to a working,
# default-on HC_IME: installs build + runtime dependencies, builds the Rust core
# and the Fcitx5 addon, installs them system-wide, and (unless --no-config) wires
# HC_IME into the running Fcitx5 session.
#
# Run it either way:
#   ./scripts/install-debian.sh       - as your normal user; sudo is called only
#                                       for apt-get and `cmake --install`.
#   sudo ./scripts/install-debian.sh  - one password prompt up front; the build
#                                       and every ~/.config change is dropped
#                                       back down to $SUDO_USER.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$ROOT/build-install}"
BACKUP_STAMP="$(date +%Y%m%d-%H%M%S)"

# Pango font description for the Hán Nôm candidate window; the trailing number is
# the point size. HanaMinA/HanaMinB come from fonts-hanazono and are what
# actually cover CJK Extension B on a stock Debian/Ubuntu box.
CANDIDATE_FONT='Hanom PV,HAN NOM B,HAN NOM A,Noto Sans CJK SC,HanaMinA,HanaMinB,Jigmo,Jigmo2,Jigmo3 17'
# Former installer value recognized only so removable_detect can find and
# restore/uninstall installs made before the candidate size changed to 17pt.
LEGACY_CANDIDATE_FONT_28='Hanom PV,HAN NOM B,HAN NOM A,Noto Sans CJK SC,HanaMinA,HanaMinB,Jigmo,Jigmo2,Jigmo3 28'

# Minimum Rust the core builds with, and the Fcitx5 core the addon declares a
# runtime dependency on (linux_fcitx5/fcitx/hcime.conf: 0=core:5.1.19).
MIN_RUST_MAJOR=1
MIN_RUST_MINOR=70
MIN_FCITX5_VERSION="5.1.19"

ASSUME_YES=0
SKIP_TESTS=0
WITH_FONTS=1
DO_CONFIG=1
DO_UNINSTALL=0
DO_UPDATE=0
DO_STATUS=0
FORCE=0
FCITX5_STOPPED=0

if [[ -t 1 ]]; then
    C_RESET=$'\033[0m'; C_BOLD=$'\033[1m'; C_RED=$'\033[31m'
    C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'; C_BLUE=$'\033[34m'
else
    C_RESET=""; C_BOLD=""; C_RED=""; C_GREEN=""; C_YELLOW=""; C_BLUE=""
fi

step()  { printf '\n%s==> %s%s\n' "$C_BOLD$C_BLUE" "$*" "$C_RESET"; }
info()  { printf '    %s\n' "$*"; }
ok()    { printf '    %s✓%s %s\n' "$C_GREEN" "$C_RESET" "$*"; }
warn()  { printf '    %s!%s %s\n' "$C_YELLOW" "$C_RESET" "$*" >&2; }
die()   { printf '\n%serror:%s %s\n' "$C_RED$C_BOLD" "$C_RESET" "$*" >&2; exit 1; }

usage() {
    cat <<'EOF'
HC_IME installer for Debian, Ubuntu, and derivatives (apt only)

Usage: scripts/install-debian.sh [options]
       sudo scripts/install-debian.sh [options]

Run with no options it shows what is already on the machine and walks through
the missing pieces one step at a time, asking before each one. Nothing is
installed that you have not seen a command for first. Re-running is safe: it
only offers what is still missing.

Both forms work. Under sudo the build and every change under your home
directory are performed as $SUDO_USER, not as root.

Options:
      --status      Show what is installed and what is missing, change nothing.
      --uninstall   Pick apart an existing install, component by component.
                    Never removes Fcitx5 or a package it did not install.
      --update      Rebuild and reinstall after a code change: no apt, no fonts,
                    no changes under ~/.config.
      --force       With --update, reinstall even when the build is unchanged.
  -h, --help        Show this help.

Automation (skips the guided run; for CI and scripts):
  -y, --yes         Answer yes to everything and run the whole install in order.
      --skip-tests  Skip `cargo test` before building the addon.
      --no-fonts    Do not install the Hán Nôm CJK fonts.
      --no-config   Install only; do not touch your Fcitx5 configuration.

Environment:
  BUILD_DIR         Build directory (default: <repo>/build-install).
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -y|--yes)     ASSUME_YES=1 ;;
        --skip-tests) SKIP_TESTS=1 ;;
        --no-fonts)   WITH_FONTS=0 ;;
        --no-config)  DO_CONFIG=0 ;;
        --update)     DO_UPDATE=1 ;;
        --status)     DO_STATUS=1 ;;
        --force)      FORCE=1 ;;
        --uninstall)  DO_UNINSTALL=1 ;;
        -h|--help)    usage; exit 0 ;;
        *)            usage >&2; die "unknown option: $1" ;;
    esac
    shift
done

if (( DO_UPDATE && DO_UNINSTALL )); then
    usage >&2; die "--update and --uninstall do the opposite of each other; pick one."
fi

# ------------------------------------------------------- privilege plumbing --
#
# Two entry points have to work: run as a normal user (sudo per root step) and
# run under sudo (drop back to $SUDO_USER for everything user-scoped). Both are
# expressed through run_root/as_user so the rest of the script never branches.
#
#   run_root - needs to write outside $HOME (apt, cmake --install, rm)
#   as_user  - anything touching $HOME, the Rust toolchain, the session bus,
#              or the user's Fcitx5 process

USER_ENV=()

if [[ $EUID -eq 0 ]]; then
    RUNNING_AS_ROOT=1
    [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]] || die \
        "run this with sudo from your normal account (sudo $0), not as root directly.
The build and the Fcitx5 configuration have to belong to a real desktop user."

    TARGET_USER="$SUDO_USER"
    TARGET_UID="$(id -u "$TARGET_USER")"
    TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
    [[ -d "$TARGET_HOME" ]] || die "cannot resolve the home directory of $TARGET_USER."

    # sudo's env_reset drops XDG_*, so these would otherwise fall back to root's.
    unset XDG_CONFIG_HOME XDG_DATA_HOME

    # runuser keeps the caller's environment, which here is root's. Rebuild the
    # parts the user's session actually needs.
    USER_ENV=(
        HOME="$TARGET_HOME"
        USER="$TARGET_USER"
        LOGNAME="$TARGET_USER"
        XDG_RUNTIME_DIR="/run/user/$TARGET_UID"
        DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$TARGET_UID/bus"
        PATH="$TARGET_HOME/.cargo/bin:$PATH"
    )
    # sudo keeps DISPLAY/XAUTHORITY but not WAYLAND_DISPLAY; pass through only
    # what is actually set so nothing is clobbered with an empty value.
    for _var in DISPLAY XAUTHORITY WAYLAND_DISPLAY XDG_SESSION_TYPE; do
        [[ -n "${!_var:-}" ]] && USER_ENV+=("$_var=${!_var}")
    done
    unset _var

    run_root() { "$@"; }
    as_user()  { runuser -u "$TARGET_USER" -- env "${USER_ENV[@]}" "$@"; }
else
    RUNNING_AS_ROOT=0
    TARGET_USER="$(id -un)"
    TARGET_UID="$EUID"
    TARGET_HOME="$HOME"

    run_root() { sudo "$@"; }
    as_user()  { "$@"; }
fi

# The session type decides how the input-method environment is wired. Under
# sudo, XDG_SESSION_TYPE is usually gone, so fall back to probing the user's
# runtime directory for a Wayland socket.
if [[ -n "${XDG_SESSION_TYPE:-}" ]]; then
    SESSION_TYPE="$XDG_SESSION_TYPE"
elif [[ -e "/run/user/$TARGET_UID/wayland-0" ]]; then
    SESSION_TYPE="wayland"
else
    SESSION_TYPE="x11"
fi

FCITX_CONFIG_DIR="${XDG_CONFIG_HOME:-$TARGET_HOME/.config}/fcitx5"
PROFILE_PATH="$FCITX_CONFIG_DIR/profile"
CLASSICUI_PATH="$FCITX_CONFIG_DIR/conf/classicui.conf"
ENV_DIR="${XDG_CONFIG_HOME:-$TARGET_HOME/.config}/environment.d"
ENV_FILE="$ENV_DIR/90-hcime-fcitx5.conf"
MANIFEST_STORE="${XDG_DATA_HOME:-$TARGET_HOME/.local/share}/hcime/install_manifest.txt"
RECEIPT_STORE="${XDG_DATA_HOME:-$TARGET_HOME/.local/share}/hcime/receipt.ini"

# Prefix for the copyable commands printed to the user. Someone already inside
# `sudo ./install.sh` should not be told to type sudo a second time.
SUDO_HINT=""
(( RUNNING_AS_ROOT )) || SUDO_HINT="sudo "

# Questions go to the terminal, not to stdin: the script has to keep working when
# it is piped in (curl ... | bash), where stdin is the script itself.
ask() {
    local prompt="$1" default="${2:-}" reply
    if ! interactive_available; then
        printf '%s\n' "$default"
        return 0
    fi
    if [[ -r /dev/tty ]]; then
        read -r -p "$prompt" reply </dev/tty
    else
        read -r -p "$prompt" reply
    fi
    printf '%s\n' "${reply:-$default}"
}

interactive_available() {
    [[ -r /dev/tty ]] || [[ -t 0 ]]
}

confirm() {
    local prompt="$1"
    (( ASSUME_YES )) && return 0
    if ! interactive_available; then
        die "$prompt (no terminal to ask on; re-run with --yes)"
    fi
    local reply
    reply="$(ask "    $prompt [Y/n] " "")"
    [[ -z "$reply" || "$reply" =~ ^[Yy] ]]
}

backup_file() {
    local path="$1"
    [[ -f "$path" ]] || return 0
    as_user cp -p "$path" "$path.hcime-backup-$BACKUP_STAMP"
    receipt_record backup "$path|$path.hcime-backup-$BACKUP_STAMP"
    info "backed up $path -> $(basename "$path").hcime-backup-$BACKUP_STAMP"
}

# ------------------------------------------------------------------ receipt --
#
# What this installer actually changed, as opposed to what it found already in
# place. Without it an uninstall cannot tell a package it installed itself from
# one the machine had all along, and would have to either leave everything
# behind or rip out somebody else's Fcitx5.
receipt_record() {
    local key="$1" value="$2"
    as_user mkdir -p "$(dirname "$RECEIPT_STORE")"
    receipt_has "$key" "$value" && return 0
    printf '%s=%s\n' "$key" "$value" | as_user tee -a "$RECEIPT_STORE" >/dev/null
}

receipt_values() {
    [[ -f "$RECEIPT_STORE" ]] || return 0
    sed -n "s/^$1=//p" "$RECEIPT_STORE" 2>/dev/null
}

receipt_has() {
    [[ -f "$RECEIPT_STORE" ]] || return 1
    receipt_values "$1" | grep -Fxq "$2"
}

receipt_forget() {
    [[ -f "$RECEIPT_STORE" ]] || return 0
    local key="$1" value="$2" tmp
    tmp="$(mktemp)"
    grep -Fxv "$key=$value" "$RECEIPT_STORE" > "$tmp" 2>/dev/null || true
    as_user cp "$tmp" "$RECEIPT_STORE"
    rm -f "$tmp"
}

# Print a command on its own indented line so the user can copy it verbatim.
copyable() {
    printf '\n      %s\n\n' "$*"
}

# Can we reach root without stopping to ask for a password? Used to decide
# whether to run apt automatically or just hand over the command to copy.
sudo_noninteractive() {
    (( RUNNING_AS_ROOT )) && return 0
    sudo -n true 2>/dev/null
}

# ---------------------------------------------------------------- preflight --

check_environment() {
    step "Checking the environment"

    if (( RUNNING_AS_ROOT )); then
        command -v runuser >/dev/null \
            || die "runuser not found (it ships in util-linux); needed to build and configure as $TARGET_USER."
        ok "running under sudo; building and configuring as $TARGET_USER ($TARGET_HOME)"
    else
        ok "running as $TARGET_USER; sudo will be used only where root is required"
    fi

    [[ -r /etc/os-release ]] || die "cannot read /etc/os-release; this installer targets Debian and Ubuntu."
    # shellcheck disable=SC1091
    . /etc/os-release

    local like="${ID_LIKE:-}"
    if [[ "${ID:-}" != "debian" && "${ID:-}" != "ubuntu" && "$like" != *debian* ]]; then
        die "this installer only supports Debian, Ubuntu, and derivatives (detected: ${PRETTY_NAME:-${ID:-unknown}}).
Run scripts/install.sh for the manual steps on other distributions, or follow README.md."
    fi
    ok "distribution: ${PRETTY_NAME:-$ID}"

    command -v apt-get >/dev/null || die "apt-get not found; this installer needs a Debian/Ubuntu system."
    (( RUNNING_AS_ROOT )) || command -v sudo >/dev/null \
        || die "sudo not found. Install it first (as root: apt-get install -y sudo), then re-run."
    if ! command -v python3 >/dev/null; then
        warn "python3 is required to edit the Fcitx5 config safely. Install it with:"
        copyable "${SUDO_HINT}apt-get install -y python3"
        die "install python3, then re-run this script."
    fi

    [[ -f "$ROOT/CMakeLists.txt" && -d "$ROOT/hc_core" ]] \
        || die "run this from the HC_IME repository (expected $ROOT/hc_core)."
    ok "repository: $ROOT"
}

# ------------------------------------------------------------- apt packages --
#
# Required packages must all be installable or the build cannot succeed. Optional
# packages (extra frontends, fonts) are best-effort: a name that does not exist
# on a given release only prints a warning instead of aborting the whole run.

REQUIRED_PKGS=(
    build-essential
    cmake
    ninja-build
    extra-cmake-modules
    pkg-config
    gettext
    libfcitx5core-dev
    fcitx5-modules-dev
    fcitx5
    fcitx5-config-qt
)
OPTIONAL_PKGS=(
    im-config
    fcitx5-frontend-gtk3
    fcitx5-frontend-gtk4
    fcitx5-frontend-qt5
    fcitx5-frontend-qt6
)

pkg_installed() {
    dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q '^install ok installed$'
}

# True when apt knows a real (non-virtual) candidate for the package.
pkg_available() {
    local cand
    cand="$(apt-cache policy "$1" 2>/dev/null | awk '/Candidate:/{print $2; exit}')"
    [[ -n "$cand" && "$cand" != "(none)" ]]
}

FONT_PKGS=(fonts-noto-cjk fonts-noto-cjk-extra fonts-hanazono)

# Packages from the given list that dpkg does not have installed.
missing_pkgs() {
    local pkg
    for pkg in "$@"; do
        pkg_installed "$pkg" || printf '%s\n' "$pkg"
    done
}

install_packages() {
    step "Installing build and runtime dependencies"
    apt_ensure "build and runtime" REQUIRED_PKGS OPTIONAL_PKGS
}

install_fonts() {
    step "Installing Hán Nôm fonts"
    apt_ensure "font" NO_REQUIRED_PKGS FONT_PKGS
    as_user fc-cache -f >/dev/null 2>&1 || true
    ok "font cache refreshed"
    check_fonts
}

NO_REQUIRED_PKGS=()

# apt_ensure <label> <name of required array> <name of optional array>
#
# Required packages must all be installable or the step fails; optional ones are
# best-effort, since a name that does not exist on a given release should not
# abort the run. Everything it actually installs is written to the receipt.
apt_ensure() {
    local label="$1"
    local -n required_ref="$2"
    local -n optional_ref="$3"

    local missing_required=() missing_optional=()
    mapfile -t missing_required < <(missing_pkgs "${required_ref[@]+"${required_ref[@]}"}")
    mapfile -t missing_optional < <(missing_pkgs "${optional_ref[@]+"${optional_ref[@]}"}")

    if [[ ${#missing_required[@]} -eq 0 && ${#missing_optional[@]} -eq 0 ]]; then
        ok "all $label packages are already installed"
        return 0
    fi

    # We are going to touch apt, so refresh the lists first; availability
    # filtering below relies on an up-to-date cache.
    info "these packages are not installed yet:"
    printf '      %s\n' "${missing_required[@]+"${missing_required[@]}"}" \
                        "${missing_optional[@]+"${missing_optional[@]}"}"
    if ! sudo_noninteractive && ! interactive_available; then
        die "cannot run apt without a password prompt here. Install the packages above, then re-run this script."
    fi
    confirm "Update apt and install them now?" \
        || die "these packages are required. Install them, then re-run this step."

    run_root apt-get update || warn "apt-get update reported problems; continuing with the current cache"

    # Availability filter: drop anything apt does not actually offer on this
    # release. Missing required packages are fatal; missing optional ones warn.
    local pkg to_install=() unavailable_required=() unavailable_optional=()
    for pkg in "${missing_required[@]+"${missing_required[@]}"}"; do
        if pkg_available "$pkg"; then to_install+=("$pkg"); else unavailable_required+=("$pkg"); fi
    done
    for pkg in "${missing_optional[@]+"${missing_optional[@]}"}"; do
        if pkg_available "$pkg"; then to_install+=("$pkg"); else unavailable_optional+=("$pkg"); fi
    done

    if [[ ${#unavailable_optional[@]} -gt 0 ]]; then
        warn "skipping optional packages not available on this release: ${unavailable_optional[*]}"
    fi
    if [[ ${#unavailable_required[@]} -gt 0 ]]; then
        die "these required packages are not available from apt on this system: ${unavailable_required[*]}
Check your apt sources (universe/main enabled?), fix them, then re-run."
    fi

    if [[ ${#to_install[@]} -eq 0 ]]; then
        ok "nothing left to install after availability filtering"
        return 0
    fi

    local install_cmd="${SUDO_HINT}apt-get install -y ${to_install[*]}"
    info "installing:"
    copyable "$install_cmd"
    if ! run_root apt-get install -y "${to_install[@]}"; then
        die "the package install failed. Run this manually, fix any errors, then re-run:
$(copyable "$install_cmd")"
    fi

    # Only what this run put on the machine is recorded: an uninstall may remove
    # these, and must never touch a package that was already here.
    local pkg
    for pkg in "${to_install[@]}"; do
        pkg_installed "$pkg" && receipt_record package "$pkg"
    done
    ok "$label packages installed"
}

check_fonts() {
    (( WITH_FONTS )) || return 0
    command -v fc-list >/dev/null || return 0
    if as_user fc-list 2>/dev/null | grep -qiE 'hanamin|han ?nom|jigmo'; then
        ok "a CJK Extension B font is available for Hán Nôm candidates"
    else
        warn "no Extension B font detected; rare Hán Nôm glyphs may render as empty boxes."
    fi
}

# ------------------------------------------------------------------ toolchain --

ensure_rust() {
    step "Checking the Rust toolchain"

    # rustup installs into ~/.cargo/bin, which is often absent from a
    # non-interactive shell's PATH. Under sudo, as_user already prepends the
    # target user's ~/.cargo/bin; here the current shell needs the same.
    if (( ! RUNNING_AS_ROOT )) && ! command -v cargo >/dev/null \
       && [[ -r "$TARGET_HOME/.cargo/env" ]]; then
        # shellcheck disable=SC1091
        . "$TARGET_HOME/.cargo/env"
    fi

    # The toolchain that matters is the target user's, not root's.
    if ! as_user bash -c 'command -v cargo >/dev/null'; then
        warn "cargo not found for $TARGET_USER. Install the Rust toolchain with either:"
        copyable "${SUDO_HINT}apt-get install -y rustc cargo"
        info "or, for a newer toolchain from upstream (run this as $TARGET_USER, not as root):"
        copyable "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        if confirm "Install rustc + cargo from apt now?"; then
            run_root apt-get install -y rustc cargo \
                || die "Rust install failed. Run one of the commands above, then re-run this script."
        else
            die "Rust is required. Run one of the commands above, then re-run this script."
        fi
    fi

    local version major minor
    version="$(as_user rustc --version | awk '{print $2}')"
    major="${version%%.*}"
    minor="${version#*.}"; minor="${minor%%.*}"
    if (( major < MIN_RUST_MAJOR || (major == MIN_RUST_MAJOR && minor < MIN_RUST_MINOR) )); then
        die "Rust $version is too old; $MIN_RUST_MAJOR.$MIN_RUST_MINOR or newer is required."
    fi
    ok "rustc $version at $(as_user bash -c 'command -v rustc')"
}

check_fcitx5_version() {
    command -v pkg-config >/dev/null || return 0
    local have
    have="$(pkg-config --modversion Fcitx5Core 2>/dev/null || true)"
    if [[ -z "$have" ]]; then
        warn "could not determine the installed Fcitx5Core version; continuing."
        return 0
    fi
    if command -v dpkg >/dev/null && dpkg --compare-versions "$have" lt "$MIN_FCITX5_VERSION"; then
        warn "Fcitx5Core $have is older than $MIN_FCITX5_VERSION, which this addon targets"
        warn "(hcime.conf declares core:$MIN_FCITX5_VERSION). The build may fail, or Fcitx5"
        warn "may refuse to load the addon. If so, install a newer Fcitx5 (PPA or from source)."
    else
        ok "Fcitx5Core $have (>= $MIN_FCITX5_VERSION)"
    fi
}

# ----------------------------------------------------------------- build --

run_tests() {
    (( SKIP_TESTS )) && { info "skipping tests (--skip-tests)"; return 0; }
    step "Running the Rust core test suite"
    as_user cargo test --manifest-path "$ROOT/hc_core/Cargo.toml"
    ok "core tests passed"
}

build_addon() {
    step "Building the Fcitx5 addon"
    info "build directory: $BUILD_DIR"
    # Built as the user so the build tree and the cargo cache do not end up
    # root-owned inside the repository.
    as_user cmake -S "$ROOT" -B "$BUILD_DIR" -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DFCITX_INSTALL_USE_FCITX_SYS_PATHS=ON
    as_user cmake --build "$BUILD_DIR"
    ok "addon built"
}

INSTALLED_ADDON=""

install_addon() {
    step "Installing HC_IME system-wide"
    info "writing into /usr as root:"
    copyable "${SUDO_HINT}cmake --install $BUILD_DIR"
    run_root cmake --install "$BUILD_DIR"

    local manifest="$BUILD_DIR/install_manifest.txt"
    if [[ -f "$manifest" ]]; then
        INSTALLED_ADDON="$(grep -m1 'libhcime\.so$' "$manifest" || true)"
        info "installed files:"
        sed 's/^/      /' "$manifest"
        # Keep a copy outside the build tree: BUILD_DIR matches the /build-*/
        # entry in .gitignore and is exactly what people delete, which would
        # otherwise leave --uninstall with nothing to work from.
        as_user mkdir -p "$(dirname "$MANIFEST_STORE")"
        as_user cp "$manifest" "$MANIFEST_STORE"
        info "manifest saved to $MANIFEST_STORE"
    fi

    if [[ -n "$INSTALLED_ADDON" && -f "$INSTALLED_ADDON" ]]; then
        ok "addon installed at $INSTALLED_ADDON"
    else
        warn "could not confirm the addon path from the install manifest"
    fi
}

# --------------------------------------------------------- fcitx5 wiring --

# Scoped to the target user: as root, an unscoped pgrep would also match a
# second desktop user's Fcitx5.
fcitx5_running() { pgrep -x -u "$TARGET_USER" fcitx5 >/dev/null 2>&1; }

# True when the session still looks like ibus (Ubuntu's default) rather than
# fcitx5. HC_IME is a fcitx5 addon, so an ibus user has to switch the framework
# with im-config before it can do anything. The process check works under sudo
# too; the env-var check only when running as the user (root's env is not the
# user's). We do not switch automatically — only surface the step to run.
ibus_active() {
    pgrep -x -u "$TARGET_USER" ibus-daemon >/dev/null 2>&1 && return 0
    if (( ! RUNNING_AS_ROOT )); then
        [[ "${GTK_IM_MODULE:-}" == *ibus* || "${XMODIFIERS:-}" == *ibus* ]] && return 0
    fi
    return 1
}

stop_fcitx5() {
    # Fcitx5 rewrites ~/.config/fcitx5/profile when it exits, so it must be
    # stopped before the profile is edited or the edit is silently lost. It also
    # maps the addon .so, which cmake --install overwrites in place.
    fcitx5_running || return 0
    info "stopping Fcitx5 so it does not overwrite the profile"
    if as_user systemctl --user --quiet is-active 'app-org.fcitx.Fcitx5@autostart.service' 2>/dev/null; then
        as_user systemctl --user stop 'app-org.fcitx.Fcitx5@autostart.service' || true
    fi
    pkill -x -u "$TARGET_USER" fcitx5 2>/dev/null || true
    local waited=0
    while fcitx5_running && (( waited < 50 )); do
        sleep 0.1; waited=$(( waited + 1 ))
    done
    if fcitx5_running; then
        warn "Fcitx5 is still running; profile changes may not stick."
    else
        FCITX5_STOPPED=1
    fi
    return 0
}

start_fcitx5() {
    step "Starting Fcitx5"
    # Always as the user: an fcitx5 started by root would attach to root's
    # runtime dir and never reach the user's session.
    if as_user systemctl --user list-unit-files 'app-org.fcitx.Fcitx5@autostart.service' >/dev/null 2>&1 \
       && as_user systemctl --user start 'app-org.fcitx.Fcitx5@autostart.service' 2>/dev/null; then
        ok "started via the systemd user unit"
    elif command -v fcitx5 >/dev/null; then
        (as_user setsid fcitx5 -d >/dev/null 2>&1 &) || true
        ok "started fcitx5 in the background"
    fi
    FCITX5_STOPPED=0
    sleep 2
}

# If the script stopped Fcitx5 and then exits before restarting it — a cancelled
# sudo prompt, a failed cmake install, a config error — the user would be left
# with no input method. Bring it back on any such exit.
on_exit() {
    local rc=$?
    if (( rc != 0 && FCITX5_STOPPED )) && ! fcitx5_running; then
        warn "run did not finish; restarting Fcitx5 so you are not left without input"
        start_fcitx5 || true
    fi
}
trap on_exit EXIT

configure_profile() {
    step "Registering HC_IME as the default input method"
    # Everything under $TARGET_HOME is created as the user; a root-owned
    # ~/.config/fcitx5 would stop Fcitx5 writing its own profile later.
    as_user mkdir -p "$FCITX_CONFIG_DIR"
    backup_file "$PROFILE_PATH"

    as_user env PROFILE_PATH="$PROFILE_PATH" python3 - <<'PY'
import configparser, os

path = os.environ["PROFILE_PATH"]
IM = "hcime"

cfg = configparser.ConfigParser(interpolation=None, delimiters=("=",))
cfg.optionxform = str          # Fcitx5 keys are case-sensitive.
if os.path.exists(path):
    cfg.read(path, encoding="utf-8")

group = "Groups/0"
if not cfg.has_section(group):
    cfg[group] = {"Name": "Default", "Default Layout": "us", "DefaultIM": IM}

# Read the existing item list in index order.
prefix = f"{group}/Items/"
indices = sorted(
    int(s[len(prefix):]) for s in cfg.sections()
    if s.startswith(prefix) and s[len(prefix):].isdigit()
)
items = [(cfg[f"{prefix}{i}"].get("Name", ""), cfg[f"{prefix}{i}"].get("Layout", "")) for i in indices]

# Fcitx5 expects the first entry of a group to be a keyboard layout. A profile
# that has never been through fcitx5-configtool may not have one yet.
if not items or not items[0][0].startswith("keyboard-"):
    layout = cfg[group].get("Default Layout", "").strip() or "us"
    items.insert(0, (f"keyboard-{layout}", ""))
    print(f"    added the missing keyboard-{layout} layout entry")

if IM in [name for name, _ in items]:
    print(f"    {IM} is already in the input-method group")
else:
    # HC_IME goes directly after the layout, ahead of any other engine.
    items.insert(1, (IM, ""))
    print(f"    added {IM} after the keyboard layout")

for i in indices:
    cfg.remove_section(f"{prefix}{i}")
for i, (name, layout) in enumerate(items):
    cfg[f"{prefix}{i}"] = {"Name": name, "Layout": layout}

previous = cfg[group].get("DefaultIM", "")
cfg[group]["DefaultIM"] = IM
if previous and previous != IM:
    print(f"    DefaultIM changed from {previous} to {IM} ({previous} is still available in the group)")

if not cfg.has_section("GroupOrder"):
    cfg["GroupOrder"] = {"0": cfg[group].get("Name", "Default")}

with open(path, "w", encoding="utf-8") as fh:
    cfg.write(fh, space_around_delimiters=False)
PY
    ok "profile updated: $PROFILE_PATH"
}

configure_font() {
    step "Setting the candidate-window font"
    warn "this is a ClassicUI setting, so it applies to every Fcitx5 input method, not just HC_IME"
    as_user mkdir -p "$(dirname "$CLASSICUI_PATH")"
    backup_file "$CLASSICUI_PATH"

    as_user env CLASSICUI_PATH="$CLASSICUI_PATH" CANDIDATE_FONT="$CANDIDATE_FONT" python3 - <<'PY'
import os, re

path = os.environ["CLASSICUI_PATH"]
font = os.environ["CANDIDATE_FONT"]

# classicui.conf is a flat key=value file with no [section] header, so it cannot
# be read with configparser. Rewrite it line by line instead, which also
# preserves the comments Fcitx5 writes above each key.
#
# Fcitx5 quotes any value containing whitespace when it writes a config file
# (stringutils::escapeForValue), and the font description contains spaces.
escaped = font.replace("\\", "\\\\").replace('"', '\\"')
value = f'"{escaped}"'

lines = []
if os.path.exists(path):
    with open(path, encoding="utf-8") as fh:
        lines = fh.read().splitlines()

# Only touch a top-level Font key; anything after a [Section] header belongs to a
# subsection and is left alone.
in_section = False
replaced = False
for i, line in enumerate(lines):
    stripped = line.strip()
    if stripped.startswith("["):
        in_section = True
    if in_section or stripped.startswith("#"):
        continue
    if re.match(r"Font\s*=", stripped):
        lines[i] = f"Font={value}"
        replaced = True
        break

if replaced:
    print("    updated the existing Font entry")
else:
    insert_at = next((i for i, l in enumerate(lines) if l.strip().startswith("[")), len(lines))
    block = ["# Font", f"Font={value}"]
    if insert_at > 0 and lines[insert_at - 1].strip():
        block.append("")
    lines[insert_at:insert_at] = block
    print("    added a Font entry")

with open(path, "w", encoding="utf-8") as fh:
    fh.write("\n".join(lines).rstrip("\n") + "\n")
PY
    ok "candidate font set to: $CANDIDATE_FONT"
}

configure_environment() {
    step "Checking input-method environment variables"
    # Under sudo this shell holds root's environment, not the user's, so there is
    # nothing meaningful to inspect: write the drop-in unconditionally.
    local missing=1
    if (( ! RUNNING_AS_ROOT )); then
        missing=0
        [[ "${GTK_IM_MODULE:-}" == "fcitx" ]] || missing=1
        [[ "${QT_IM_MODULE:-}" == "fcitx" ]] || missing=1
        [[ "${XMODIFIERS:-}"   == "@im=fcitx" ]] || missing=1
    fi

    if (( ! missing )); then
        ok "GTK_IM_MODULE, QT_IM_MODULE and XMODIFIERS already point at fcitx"
        return 0
    fi

    # On a Wayland session, GTK4 and Qt6 reach Fcitx5 through the compositor's
    # text-input protocol. Forcing GTK_IM_MODULE there routes them down the older
    # module path instead, which is a downgrade, so only XMODIFIERS is set for
    # the XWayland clients that still need it.
    as_user mkdir -p "$ENV_DIR"
    if [[ "$SESSION_TYPE" == "wayland" ]]; then
        as_user tee "$ENV_FILE" >/dev/null <<'EOF'
# Written by HC_IME scripts/install-debian.sh
# Wayland session: GTK4/Qt6 use the text-input protocol, so only XWayland
# clients need an explicit input-method module.
XMODIFIERS=@im=fcitx
EOF
        ok "wrote $ENV_FILE (Wayland session)"
    else
        as_user tee "$ENV_FILE" >/dev/null <<'EOF'
# Written by HC_IME scripts/install-debian.sh
GTK_IM_MODULE=fcitx
QT_IM_MODULE=fcitx
XMODIFIERS=@im=fcitx
EOF
        ok "wrote $ENV_FILE (X11 session)"
    fi
    warn "log out and back in for these to take effect in every application"
}

verify_installation() {
    step "Verifying the installation"

    if [[ -n "$INSTALLED_ADDON" && -f "$INSTALLED_ADDON" ]]; then
        ok "addon present: $INSTALLED_ADDON"
    fi

    if ! fcitx5_running; then
        warn "Fcitx5 is not running; start it and re-check."
        return 0
    fi
    # Avoid `pgrep | head`: under `set -o pipefail` the SIGPIPE from head would
    # fail the pipeline.
    local pids
    pids="$(pgrep -x -u "$TARGET_USER" fcitx5 || true)"
    ok "Fcitx5 is running (pid ${pids%%$'\n'*})"

    if command -v gdbus >/dev/null; then
        local available
        # Must go over the user's session bus; root has no session bus here.
        available="$(as_user gdbus call --session --dest org.fcitx.Fcitx5 \
            --object-path /controller \
            --method org.fcitx.Fcitx.Controller1.AvailableInputMethods 2>/dev/null || true)"
        if [[ "$available" == *"hcime"* ]]; then
            ok "Fcitx5 reports hcime as an available input method"
        elif [[ -n "$available" ]]; then
            warn "Fcitx5 answered but did not list hcime. Run: fcitx5-configtool"
        else
            warn "could not query Fcitx5 over D-Bus; check manually with fcitx5-configtool"
        fi
    fi
}

print_summary() {
    cat <<EOF

$C_BOLD${C_GREEN}HC_IME installed.$C_RESET

Next steps:
  1. Switch input methods with $C_BOLD Ctrl+Space $C_RESET (the Fcitx5 default) and type
     Vietnamese to confirm the addon is live.
  2. Open $C_BOLD fcitx5-configtool $C_RESET to pick Telex / VNI / VIQR or a Hán Nôm mode
     and to adjust spell check, macros, and per-application rules.
  3. If nothing happens in an application, log out and back in so the input-method
     environment variables reach it.

Backups of anything this script changed are next to the originals with a
.hcime-backup-$BACKUP_STAMP suffix.

After a code change, do not run this installer again — rebuild and reinstall
only, leaving your configuration alone:  scripts/update.sh

To remove HC_IME again:  scripts/install-debian.sh --uninstall
EOF

    # Ubuntu ships ibus by default. HC_IME needs fcitx5 to be the active input
    # framework, so an ibus user has one more step the installer does not take
    # on their behalf: switching with im-config and logging back in.
    if ibus_active; then
        cat >&2 <<EOF

$C_BOLD${C_YELLOW}Heads up: your session still looks like ibus, not fcitx5.$C_RESET
HC_IME runs on fcitx5, so switch the input framework and re-login:

      im-config -n fcitx5

Then log out and back in. To switch back to ibus later: im-config -n ibus
EOF
    fi
}

# ---------------------------------------------------------------- update --
#
# The fast path for "I changed the code, put the new build in place". A full
# install is only needed once per machine: apt packages, fonts, and the Fcitx5
# configuration survive a rebuild, so an update touches none of them. It
# rebuilds incrementally, reinstalls, and restarts Fcitx5.

# The manifest of the last install. The copy under ~/.local/share outlives the
# build tree, which people delete; the one in BUILD_DIR is the fallback.
previous_install_manifest() {
    if [[ -f "$MANIFEST_STORE" ]]; then
        printf '%s\n' "$MANIFEST_STORE"
    elif [[ -f "$BUILD_DIR/install_manifest.txt" ]]; then
        printf '%s\n' "$BUILD_DIR/install_manifest.txt"
    fi
}

# The build-tree (or source-tree) file that ends up at the installed path $1,
# mirroring the install() rules in linux_fcitx5/CMakeLists.txt. Printing nothing
# means "unknown", which callers must treat as changed — a wrong guess can then
# only cause an unnecessary reinstall, never a skipped one.
built_counterpart() {
    local installed="$1" base parent candidate
    base="$(basename "$installed")"
    parent="$(basename "$(dirname "$installed")")"

    # fcitx/hcime-inputmethod.conf is installed as inputmethod/hcime.conf.
    if [[ "$parent" == "inputmethod" && "$base" == "hcime.conf" ]]; then
        [[ -f "$ROOT/linux_fcitx5/fcitx/hcime-inputmethod.conf" ]] \
            && printf '%s\n' "$ROOT/linux_fcitx5/fcitx/hcime-inputmethod.conf"
        return 0
    fi

    for candidate in \
        "$BUILD_DIR/linux_fcitx5/$base" \
        "$BUILD_DIR/linux_fcitx5/cargo-target/release/$base" \
        "$ROOT/linux_fcitx5/fcitx/$base"
    do
        [[ -f "$candidate" ]] && { printf '%s\n' "$candidate"; return 0; }
    done
    return 0
}

# True when every file from the last install still matches what was just built,
# i.e. reinstalling would copy nothing new. Lets an update leave a running
# Fcitx5 alone when the rebuild produced no change.
#
# Compared by size and modification time, not by content: `cmake --install`
# rewrites the RPATH of libhcime.so while copying it (BUILD_RPATH is the cargo
# target dir, INSTALL_RPATH is $ORIGIN), so the installed library is never
# byte-identical to the one in the build tree. It does keep the build tree's
# timestamp, truncated to whole seconds, which is what %Y reports.
install_is_current() {
    local manifest="$1" installed built
    [[ -s "$manifest" ]] || return 1
    # `|| [[ -n ... ]]` so a manifest without a trailing newline keeps its last
    # line; cmake writes one that way.
    while IFS= read -r installed || [[ -n "$installed" ]]; do
        [[ -n "$installed" ]] || continue
        [[ -f "$installed" ]] || return 1
        built="$(built_counterpart "$installed")"
        [[ -n "$built" ]] || return 1
        [[ "$(stat -c %s "$built")" == "$(stat -c %s "$installed")" ]] || return 1
        (( $(stat -c %Y "$installed") >= $(stat -c %Y "$built") )) || return 1
    done < "$manifest"
    return 0
}

update() {
    local manifest adopted=0
    manifest="$(previous_install_manifest)"
    if [[ -z "$manifest" ]]; then
        # A machine built and installed by hand has no manifest, but it does have
        # an addon. Refusing there would send someone who already runs HC_IME
        # through a full install; adopt it instead and start keeping records.
        local existing
        if existing="$(installed_addon_path)"; then
            adopted=1
            info "found an installation this script did not make: $existing"
            confirm "Rebuild and install over it?" || die "aborted."
        else
            die "no previous HC_IME installation found (looked for $MANIFEST_STORE, and for an addon under $(fcitx5_addon_dir 2>/dev/null || echo 'the Fcitx5 addon directory')).
An update only refreshes an existing install; set the machine up once with:
$(copyable "./scripts/install.sh")"
        fi
    fi

    cat <<EOF

${C_BOLD}HC_IME update${C_RESET}

Rebuilding for: ${C_BOLD}$TARGET_USER${C_RESET} ($TARGET_HOME)
Last install:   ${manifest:-none (adopting an installation made by hand)}

This rebuilds the Rust core and the Fcitx5 addon and reinstalls them over the
existing installation. apt packages, fonts, and your Fcitx5 configuration are
left exactly as they are.
EOF

    ensure_rust
    run_tests
    build_addon

    # An adopted install has no manifest to compare against, and reinstalling is
    # the whole point of adopting it, so it never takes the shortcut.
    if (( ! FORCE && ! adopted )) && install_is_current "$manifest"; then
        step "Nothing to reinstall"
        ok "the installed files already match this build"
        info "Fcitx5 was left running; reinstall anyway with:"
        copyable "scripts/update.sh --force"
        return 0
    fi

    stop_fcitx5
    install_addon
    start_fcitx5
    verify_installation

    if (( DO_CONFIG )) && [[ -f "$PROFILE_PATH" ]] && ! grep -q 'hcime' "$PROFILE_PATH"; then
        warn "hcime is not in $PROFILE_PATH; run ./scripts/install.sh to wire it back in"
    fi

    printf '\n%s%sHC_IME updated.%s The new build is installed and Fcitx5 has been restarted.\n' \
        "$C_BOLD" "$C_GREEN" "$C_RESET"
}

# ------------------------------------------------------------- uninstall --

# Removal is the same table as the install, read in the other direction: each
# piece says whether it is still present and what taking it off would touch.
# Anything the machine already had before HC_IME was installed is not on this
# list at all — the receipt is what tells the two apart.
REMOVABLES=(addon profile font env fonts)

removable_label() {
    case "$1" in
        addon)   echo "Installed addon (.so + .conf)" ;;
        profile) echo "hcime in the Fcitx5 profile" ;;
        font)    echo "Candidate window font" ;;
        env)     echo "Environment drop-in" ;;
        fonts)   echo "Hán Nôm font packages (apt)" ;;
    esac
}

# Packages this installer put on the machine, still installed today.
installed_by_us() {
    local pkg
    while IFS= read -r pkg; do
        [[ -n "$pkg" ]] || continue
        pkg_installed "$pkg" && printf '%s\n' "$pkg"
    done < <(receipt_values package)
}

# The backup this installer made for a config file, if it is still around.
backup_for() {
    local target="$1" line
    while IFS= read -r line; do
        [[ "${line%%|*}" == "$target" ]] || continue
        local backup="${line#*|}"
        [[ -f "$backup" ]] && printf '%s\n' "$backup"
    done < <(receipt_values backup) | tail -n 1
}

removable_detect() {
    case "$1" in
        addon)
            local manifest; manifest="$(previous_install_manifest)"
            if [[ -n "$manifest" ]]; then
                # cmake writes the last path without a trailing newline, so count
                # non-empty lines rather than newlines.
                echo "present|$(grep -c . "$manifest") file(s) from the manifest"
                return
            fi
            # No manifest: an installation made by hand still has to be
            # removable, but it is listed as what it is.
            local found=(); mapfile -t found < <(installed_files)
            if [[ ${#found[@]} -gt 0 ]]; then
                echo "present|${#found[@]} file(s) found on disk, not recorded by this script"
            else
                echo "absent|no installed addon found"
            fi
            ;;
        profile)
            if [[ -f "$PROFILE_PATH" ]] && grep -q 'hcime' "$PROFILE_PATH"; then
                echo "present|remove from the group, hand DefaultIM to another IM"
            else
                echo "absent|not in the profile"
            fi
            ;;
        font)
            local backup; backup="$(backup_for "$CLASSICUI_PATH")"
            if [[ -f "$CLASSICUI_PATH" ]] && (grep -Fq "$CANDIDATE_FONT" "$CLASSICUI_PATH" ||
                grep -Fq "$LEGACY_CANDIDATE_FONT_28" "$CLASSICUI_PATH"); then
                if [[ -n "$backup" ]]; then
                    echo "present|restore from $(basename "$backup")"
                else
                    echo "present|remove the Font line (no backup was taken)"
                fi
            else
                echo "absent|not a font set by HC_IME"
            fi
            ;;
        env)
            if [[ -f "$ENV_FILE" ]]; then
                echo "present|remove $(basename "$ENV_FILE")"
            else
                echo "absent|not present"
            fi
            ;;
        fonts)
            local pkgs=(); mapfile -t pkgs < <(installed_by_us)
            local fonts=() pkg
            for pkg in "${pkgs[@]+"${pkgs[@]}"}"; do
                [[ " ${FONT_PKGS[*]} " == *" $pkg "* ]] && fonts+=("$pkg")
            done
            if [[ ${#fonts[@]} -gt 0 ]]; then
                echo "present|${fonts[*]}"
            else
                echo "absent|no font package was installed by this installer"
            fi
            ;;
    esac
}

remove_addon() {
    local manifest; manifest="$(previous_install_manifest)"
    if [[ -n "$manifest" ]]; then
        info "these files will be removed:"
        sed 's/^/      /' "$manifest"
        copyable "${SUDO_HINT}xargs -d '\\n' -a $manifest rm -f --"
        stop_fcitx5
        # -d '\n' so paths are split only on newlines; xargs otherwise treats
        # spaces, quotes and backslashes as separators.
        run_root xargs -d '\n' -a "$manifest" rm -f --
        as_user rm -f "$MANIFEST_STORE"
        ok "removed the installed files"
        return 0
    fi

    # Nothing recorded: remove what is actually on disk, but say plainly that
    # these are files this script did not put there.
    local found=(); mapfile -t found < <(installed_files)
    [[ ${#found[@]} -gt 0 ]] || { warn "no installed addon found, skipping"; return 0; }
    warn "these files were not installed by this script; removing them anyway:"
    printf '      %s\n' "${found[@]}"
    copyable "${SUDO_HINT}rm -f ${found[*]}"
    confirm "Remove them?" || { info "kept"; return 0; }
    stop_fcitx5
    run_root rm -f -- "${found[@]}"
    ok "removed the installed files"
}

remove_font() {
    local backup; backup="$(backup_for "$CLASSICUI_PATH")"
    if [[ -n "$backup" ]]; then
        as_user cp -p "$backup" "$CLASSICUI_PATH"
        ok "restored $CLASSICUI_PATH from $(basename "$backup")"
        return 0
    fi
    # No backup: the installer added the Font line to a file that had none, so
    # taking that line back out is the closest thing to the original state.
    as_user env CLASSICUI_PATH="$CLASSICUI_PATH" python3 - <<'PY'
import os

path = os.environ["CLASSICUI_PATH"]
if os.path.exists(path):
    with open(path, encoding="utf-8") as fh:
        lines = fh.read().splitlines()
    kept, in_section = [], False
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("["):
            in_section = True
        if not in_section and stripped.startswith("Font="):
            continue
        kept.append(line)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(kept).rstrip("\n") + "\n")
    print("    removed the Font entry")
PY
    ok "dropped the font setting HC_IME added"
}

remove_font_packages() {
    local pkgs=() fonts=() pkg
    mapfile -t pkgs < <(installed_by_us)
    for pkg in "${pkgs[@]+"${pkgs[@]}"}"; do
        [[ " ${FONT_PKGS[*]} " == *" $pkg "* ]] && fonts+=("$pkg")
    done
    [[ ${#fonts[@]} -gt 0 ]] || { info "no font package to remove"; return 0; }
    copyable "${SUDO_HINT}apt-get remove -y ${fonts[*]}"
    run_root apt-get remove -y "${fonts[@]}" || { warn "apt reported an error removing the fonts"; return 1; }
    for pkg in "${fonts[@]}"; do receipt_forget package "$pkg"; done
    ok "removed the font packages"
}

removable_apply() {
    case "$1" in
        addon)   remove_addon ;;
        profile) remove_profile_entry ;;
        font)    remove_font ;;
        env)     as_user rm -f "$ENV_FILE"; ok "removed $ENV_FILE" ;;
        fonts)   remove_font_packages ;;
    esac
}

uninstall() {
    cat <<EOF

${C_BOLD}HC_IME${C_RESET} — uninstall
  ${PRETTY_NAME:-$ID} · $TARGET_USER ($TARGET_HOME)
EOF

    local -a states=() details=()
    local id result
    for id in "${REMOVABLES[@]}"; do
        result="$(removable_detect "$id")"
        states+=("${result%%|*}")
        details+=("${result#*|}")
    done

    printf '\n  %s#  Component                      What happens%s\n' "$C_BOLD" "$C_RESET"
    printf '  ─────────────────────────────────────────────────────────────────\n'
    local i present=()
    for i in "${!REMOVABLES[@]}"; do
        local mark
        if [[ "${states[$i]}" == "present" ]]; then
            mark="$(printf '%s✓%s' "$C_GREEN" "$C_RESET")"
            present+=("$i")
        else
            mark="$(printf '%s·%s' "$C_YELLOW" "$C_RESET")"
        fi
        printf '  %s%d%s  %s %s %s\n' "$C_BOLD" "$((i + 1))" "$C_RESET" \
            "$(pad_to "$(removable_label "${REMOVABLES[$i]}")" 30)" "$mark" "${details[$i]}"
    done

    # The line that answers "will this rip out my Fcitx5?".
    local kept=() pkg
    while IFS= read -r pkg; do
        [[ -n "$pkg" ]] || continue
        [[ " ${FONT_PKGS[*]} " == *" $pkg "* ]] || kept+=("$pkg")
    done < <(installed_by_us)
    printf '  %s─  %s %s%s\n' "$C_YELLOW" "$(pad_to "Fcitx5 and build packages" 30)" "NOT removed" "$C_RESET"
    if [[ ${#kept[@]} -gt 0 ]]; then
        printf '     %s(this installer did install: %s — remove by hand if you want)%s\n' \
            "$C_YELLOW" "${kept[*]}" "$C_RESET"
    fi
    printf '\n'

    local labels=() index
    for index in "${present[@]+"${present[@]}"}"; do labels+=("$((index + 1))"); done
    if [[ ${#present[@]} -eq 0 ]]; then
        ok "nothing of HC_IME is left on this machine"
        return 0
    fi

    printf '  %sa%s) remove addon + configuration  (%s)\n' "$C_BOLD" "$C_RESET" "${labels[*]}"
    printf '  %sc%s) choose components yourself\n' "$C_BOLD" "$C_RESET"
    printf '  %sq%s) quit\n\n' "$C_BOLD" "$C_RESET"

    local choice selection=()
    choice="$(ask "  Choose [a]: " "a")"
    case "$choice" in
        c|C)
            local raw; raw="$(ask "  Enter numbers separated by spaces: " "")"
            local token
            for token in $raw; do
                [[ "$token" =~ ^[0-9]+$ ]] || continue
                (( token >= 1 && token <= ${#REMOVABLES[@]} )) && selection+=("$((token - 1))")
            done
            ;;
        q|Q) info "quit"; return 0 ;;
        *)   selection=("${present[@]}") ;;
    esac

    [[ ${#selection[@]} -gt 0 ]] || { info "nothing selected, quitting"; return 0; }

    local position=0
    for index in "${selection[@]}"; do
        position=$((position + 1))
        id="${REMOVABLES[$index]}"
        printf '\n%s[%d/%d] %s%s\n' "$C_BOLD$C_BLUE" "$position" "${#selection[@]}" \
            "$(removable_label "$id")" "$C_RESET"
        local answer; answer="$(ask "    Enter to remove · s to skip · q to stop: " "")"
        case "$answer" in
            s|S) info "skipped"; continue ;;
            q|Q) info "stopped on request"; break ;;
        esac
        if ( ASSUME_YES=1; removable_apply "$id" ); then
            ok "done"
        else
            warn "failed, moving on"
        fi
        fcitx5_running || start_fcitx5
    done

    start_fcitx5
    printf '\n%sHC_IME removed.%s Backups are left in place.\n' "$C_BOLD" "$C_RESET"
}

remove_profile_entry() {
    [[ -f "$PROFILE_PATH" ]] || return 0
    backup_file "$PROFILE_PATH"
    stop_fcitx5
    as_user env PROFILE_PATH="$PROFILE_PATH" python3 - <<'PY'
import configparser, os

path = os.environ["PROFILE_PATH"]
IM = "hcime"

cfg = configparser.ConfigParser(interpolation=None, delimiters=("=",))
cfg.optionxform = str
cfg.read(path, encoding="utf-8")

for group in [s for s in cfg.sections() if s.startswith("Groups/") and s.count("/") == 1]:
    prefix = f"{group}/Items/"
    indices = sorted(
        int(s[len(prefix):]) for s in cfg.sections()
        if s.startswith(prefix) and s[len(prefix):].isdigit()
    )
    items = [(cfg[f"{prefix}{i}"].get("Name", ""), cfg[f"{prefix}{i}"].get("Layout", "")) for i in indices]
    kept = [it for it in items if it[0] != IM]
    for i in indices:
        cfg.remove_section(f"{prefix}{i}")
    for i, (name, layout) in enumerate(kept):
        cfg[f"{prefix}{i}"] = {"Name": name, "Layout": layout}
    if cfg[group].get("DefaultIM", "") == IM:
        fallback = next((n for n, _ in kept if not n.startswith("keyboard-")), "")
        cfg[group]["DefaultIM"] = fallback or (kept[0][0] if kept else "")
        print(f"    DefaultIM reset to '{cfg[group]['DefaultIM']}'")

with open(path, "w", encoding="utf-8") as fh:
    cfg.write(fh, space_around_delimiters=False)
PY
    ok "removed hcime from the profile"
}

# -------------------------------------------------------------- components --
#
# Every piece HC_IME puts on a machine, as something the user can look at and
# pick from. A component knows how to detect itself, how to apply itself, and
# (for the uninstaller) how to take itself back off. The guided run and the
# status screen are both just views over this table, which is why they can never
# disagree about what is installed.
#
# detect prints "<state>|<detail>", where state is:
#   ok      already in place, nothing to do
#   missing not there yet
#   stale   there, but out of date or not what this build produces
#   warn    there, with something worth reading before proceeding

COMPONENTS=(deps fonts rust build install profile font env)

component_label() {
    case "$1" in
        deps)    echo "Build + Fcitx5 packages (apt)" ;;
        fonts)   echo "Hán Nôm fonts" ;;
        rust)    echo "Rust toolchain" ;;
        build)   echo "Build the addon and run tests" ;;
        install) echo "Install the addon system-wide" ;;
        profile) echo "Register hcime with Fcitx5" ;;
        font)    echo "Candidate window font" ;;
        env)     echo "Input-method environment" ;;
    esac
}

component_note() {
    case "$1" in
        install) echo "needs root" ;;
        font)    echo "applies to EVERY input method, not just hcime" ;;
        env)     echo "takes effect after you log out and back in" ;;
        *)       echo "" ;;
    esac
}

detect_deps() {
    local missing=()
    mapfile -t missing < <(missing_pkgs "${REQUIRED_PKGS[@]}")
    if [[ ${#missing[@]} -eq 0 ]]; then
        local optional_missing=()
        mapfile -t optional_missing < <(missing_pkgs "${OPTIONAL_PKGS[@]}")
        if [[ ${#optional_missing[@]} -gt 0 ]]; then
            echo "warn|missing optional: ${optional_missing[*]}"
        else
            echo "ok|all present"
        fi
        return
    fi
    echo "missing|missing ${#missing[@]} package(s): ${missing[*]}"
}

detect_fonts() {
    local missing=()
    mapfile -t missing < <(missing_pkgs "${FONT_PKGS[@]}")
    if [[ ${#missing[@]} -eq 0 ]]; then
        echo "ok|all present"
    elif command -v fc-list >/dev/null && as_user fc-list 2>/dev/null | grep -qiE 'hanamin|han ?nom|jigmo'; then
        echo "warn|another Ext-B font found; missing: ${missing[*]}"
    else
        echo "missing|missing: ${missing[*]}"
    fi
}

detect_rust() {
    if ! as_user bash -c 'command -v cargo >/dev/null' 2>/dev/null; then
        echo "missing|no cargo found"
        return
    fi
    local version major minor
    version="$(as_user rustc --version 2>/dev/null | awk '{print $2}')"
    major="${version%%.*}"; minor="${version#*.}"; minor="${minor%%.*}"
    if (( major < MIN_RUST_MAJOR || (major == MIN_RUST_MAJOR && minor < MIN_RUST_MINOR) )); then
        echo "stale|rustc $version < $MIN_RUST_MAJOR.$MIN_RUST_MINOR"
    else
        echo "ok|rustc $version"
    fi
}

# Newest mtime among the sources the addon is built from.
newest_source_mtime() {
    local newest=0 file
    for file in "$ROOT"/hc_core/src/*.rs "$ROOT"/linux_fcitx5/src/*.cpp "$ROOT"/linux_fcitx5/include/hcime/*.h; do
        [[ -f "$file" ]] || continue
        local mtime; mtime="$(stat -c %Y "$file")"
        (( mtime > newest )) && newest="$mtime"
    done
    printf '%s\n' "$newest"
}

built_addon_path() { printf '%s\n' "$BUILD_DIR/linux_fcitx5/libhcime.so"; }

# Where Fcitx5 loads addons from, and where its addon metadata lives, both asked
# of Fcitx5 itself rather than assumed: /usr/lib/x86_64-linux-gnu/fcitx5 on
# Debian multiarch, /usr/lib/fcitx5 on Arch. Everything that looks for an
# existing installation goes through these, so a machine that was built and
# installed by hand — never through this script — is still recognised.
fcitx5_addon_dir() {
    local libdir
    libdir="$(pkg-config --variable=libdir Fcitx5Core 2>/dev/null)" || return 1
    [[ -n "$libdir" ]] || return 1
    printf '%s/fcitx5\n' "$libdir"
}

fcitx5_data_dir() {
    local prefix
    prefix="$(pkg-config --variable=prefix Fcitx5Core 2>/dev/null)" || return 1
    [[ -n "$prefix" ]] || return 1
    printf '%s/share/fcitx5\n' "$prefix"
}

installed_addon_path() {
    local dir
    dir="$(fcitx5_addon_dir)" || return 1
    [[ -f "$dir/libhcime.so" ]] || return 1
    printf '%s/libhcime.so\n' "$dir"
}

# Every file an installation owns, whether this script put it there or not.
installed_files() {
    local dir data
    if dir="$(fcitx5_addon_dir)"; then
        [[ -f "$dir/libhcime.so" ]]  && printf '%s\n' "$dir/libhcime.so"
        [[ -f "$dir/libhc_core.so" ]] && printf '%s\n' "$dir/libhc_core.so"
    fi
    if data="$(fcitx5_data_dir)"; then
        [[ -f "$data/addon/hcime.conf" ]]       && printf '%s\n' "$data/addon/hcime.conf"
        [[ -f "$data/inputmethod/hcime.conf" ]] && printf '%s\n' "$data/inputmethod/hcime.conf"
    fi
    return 0
}

# The strongest signal there is: the running Fcitx5 lists hcime as usable.
addon_loaded_in_fcitx5() {
    command -v gdbus >/dev/null || return 1
    fcitx5_running || return 1
    as_user timeout 5 gdbus call --session --dest org.fcitx.Fcitx5 \
        --object-path /controller \
        --method org.fcitx.Fcitx.Controller1.AvailableInputMethods 2>/dev/null \
        | grep -q "'hcime'"
}

# Compared by size and mtime, for the same reason install_is_current does:
# `cmake --install` rewrites the RPATH while copying, so the installed library is
# never byte-identical to the one in the build tree.
installed_matches_build() {
    local installed built
    installed="$(installed_addon_path)" || return 1
    built="$(built_addon_path)"
    [[ -f "$built" ]] || return 0
    [[ "$(stat -c %s "$built")" == "$(stat -c %s "$installed")" ]] || return 1
    (( $(stat -c %Y "$installed") >= $(stat -c %Y "$built") ))
}

detect_build() {
    local addon; addon="$(built_addon_path)"
    if [[ ! -f "$addon" ]]; then
        echo "missing|not built yet"
        return
    fi
    if (( $(newest_source_mtime) > $(stat -c %Y "$addon") )); then
        echo "stale|sources are newer than the build"
    else
        echo "ok|up to date with the sources"
    fi
}

detect_install() {
    local addon
    if ! addon="$(installed_addon_path)"; then
        echo "missing|not installed"
        return
    fi
    if ! installed_matches_build; then
        echo "stale|installed copy is older than the build"
        return
    fi
    # Installed and current. Two things are still worth saying out loud: whether
    # Fcitx5 actually took it, and whether this script knows about it at all.
    if fcitx5_running && ! addon_loaded_in_fcitx5; then
        echo "warn|installed at $(dirname "$addon") but Fcitx5 has not loaded it"
    elif [[ -z "$(previous_install_manifest)" ]]; then
        echo "warn|installed by hand at $(dirname "$addon"), not recorded by this script"
    else
        echo "ok|matches the build"
    fi
}

detect_profile() {
    [[ -f "$PROFILE_PATH" ]] || { echo "missing|no Fcitx5 profile yet"; return; }
    if grep -q 'hcime' "$PROFILE_PATH"; then
        if grep -q '^DefaultIM=hcime' "$PROFILE_PATH"; then
            echo "ok|present, set as default"
        else
            echo "warn|in the group but not the default"
        fi
    else
        echo "missing|not in the group"
    fi
}

detect_font() {
    [[ -f "$CLASSICUI_PATH" ]] || { echo "missing|not set"; return; }
    if grep -Fq "$CANDIDATE_FONT" "$CLASSICUI_PATH"; then
        echo "ok|Hán Nôm font is set"
    else
        echo "missing|a different font is set"
    fi
}

detect_env() {
    if [[ -f "$ENV_FILE" ]]; then
        echo "ok|$(basename "$ENV_FILE") present"
    elif (( ! RUNNING_AS_ROOT )) && [[ "${GTK_IM_MODULE:-}" == "fcitx" || "${XMODIFIERS:-}" == "@im=fcitx" ]]; then
        echo "ok|environment already points at fcitx"
    else
        echo "missing|not configured"
    fi
}

component_detect() {
    case "$1" in
        deps) detect_deps ;; fonts) detect_fonts ;; rust) detect_rust ;;
        build) detect_build ;; install) detect_install ;; profile) detect_profile ;;
        font) detect_font ;; env) detect_env ;;
    esac
}

# The commands a component runs, shown before it runs them so nothing happens
# that the user did not read first.
component_commands() {
    case "$1" in
        deps)    echo "${SUDO_HINT}apt-get install -y <missing packages>" ;;
        fonts)   echo "${SUDO_HINT}apt-get install -y ${FONT_PKGS[*]}; fc-cache -f" ;;
        rust)    echo "check cargo/rustc (>= $MIN_RUST_MAJOR.$MIN_RUST_MINOR)" ;;
        build)   echo "cargo test --manifest-path hc_core/Cargo.toml; cmake --build $BUILD_DIR" ;;
        install) echo "${SUDO_HINT}cmake --install $BUILD_DIR" ;;
        profile) echo "edit $PROFILE_PATH (backed up first)" ;;
        font)    echo "edit $CLASSICUI_PATH (backed up first)" ;;
        env)     echo "write $ENV_FILE" ;;
    esac
}

component_apply() {
    case "$1" in
        deps)    install_packages ;;
        fonts)   install_fonts ;;
        rust)    ensure_rust ;;
        build)   ensure_rust; run_tests; build_addon ;;
        install) stop_fcitx5; install_addon; start_fcitx5; verify_installation ;;
        profile) stop_fcitx5; configure_profile; start_fcitx5 ;;
        font)    configure_font ;;
        env)     configure_environment ;;
    esac
}

# ----------------------------------------------------------------- wizard --

# printf's %-30s pads to a byte count, and Vietnamese labels are multi-byte, so
# the columns would drift. Pad by character count instead.
pad_to() {
    local text="$1" width="$2" length=${#1}
    local fill=$(( width - length ))
    (( fill < 0 )) && fill=0
    printf '%s%*s' "$text" "$fill" ""
}

state_mark() {
    case "$1" in
        ok)      printf '%s✓%s' "$C_GREEN" "$C_RESET" ;;
        missing) printf '%s✗%s' "$C_RED" "$C_RESET" ;;
        stale)   printf '%s↻%s' "$C_YELLOW" "$C_RESET" ;;
        warn)    printf '%s!%s' "$C_YELLOW" "$C_RESET" ;;
    esac
}

# Fills DETECTED_STATE/DETECTED_DETAIL, indexed the same as COMPONENTS.
declare -a DETECTED_STATE=() DETECTED_DETAIL=()

scan_components() {
    DETECTED_STATE=(); DETECTED_DETAIL=()
    local id result
    for id in "${COMPONENTS[@]}"; do
        result="$(component_detect "$id")"
        DETECTED_STATE+=("${result%%|*}")
        DETECTED_DETAIL+=("${result#*|}")
    done
}

print_status_table() {
    printf '\n  %s#  Component                      Status%s\n' "$C_BOLD" "$C_RESET"
    printf '  ─────────────────────────────────────────────────────────────────\n'
    local i note
    for i in "${!COMPONENTS[@]}"; do
        printf '  %s%d%s  %s %s %s\n' \
            "$C_BOLD" "$((i + 1))" "$C_RESET" \
            "$(pad_to "$(component_label "${COMPONENTS[$i]}")" 30)" \
            "$(state_mark "${DETECTED_STATE[$i]}")" \
            "${DETECTED_DETAIL[$i]}"
        note="$(component_note "${COMPONENTS[$i]}")"
        [[ -n "$note" ]] && printf '     %s%s↳ %s%s\n' "$C_YELLOW" "$(pad_to "" 30)" "$note" "$C_RESET"
    done
    printf '\n'
}

# Indices (0-based) of everything not already in place.
pending_indices() {
    local i
    for i in "${!COMPONENTS[@]}"; do
        [[ "${DETECTED_STATE[$i]}" != "ok" ]] && printf '%s\n' "$i"
    done
    return 0
}

print_status() {
    cat <<EOF

${C_BOLD}HC_IME${C_RESET} — what is on this machine
  ${PRETTY_NAME:-$ID} · ${SESSION_TYPE} · $TARGET_USER
EOF
    scan_components
    print_status_table
    local pending=()
    mapfile -t pending < <(pending_indices)
    if [[ ${#pending[@]} -eq 0 ]]; then
        ok "everything is in place"
    else
        info "${#pending[@]} item(s) still to do; run ./scripts/install.sh to step through them"
    fi
}

# Runs one component, catching a failure instead of killing the run. The step
# functions abort with die(), so each one goes into a subshell: die() then ends
# only that subshell and the guided run stays in control.
run_component() {
    local id="$1"
    ( ASSUME_YES=1; component_apply "$id" )
}

guided_run() {
    local -a queue=("$@")
    local position=0 index id label
    for index in "${queue[@]}"; do
        position=$((position + 1))
        id="${COMPONENTS[$index]}"
        label="$(component_label "$id")"

        printf '\n%s[%d/%d] %s%s\n' "$C_BOLD$C_BLUE" "$position" "${#queue[@]}" "$label" "$C_RESET"
        printf '      %s\n' "$(component_commands "$id")"
        local note; note="$(component_note "$id")"
        [[ -n "$note" ]] && warn "$note"

        local answer
        answer="$(ask "    Enter to run · s to skip · q to stop: " "")"
        case "$answer" in
            s|S) info "skipped"; continue ;;
            q|Q) info "stopped on request"; return 0 ;;
        esac

        while true; do
            if run_component "$id"; then
                ok "$label: done"
                break
            fi
            warn "$label: failed"
            # The step runs in a subshell, so a failure after it stopped Fcitx5
            # leaves the user with no input method and this shell none the wiser.
            if ! fcitx5_running; then
                warn "Fcitx5 is stopped — restarting it so you are not left without input"
                start_fcitx5
            fi
            answer="$(ask "    r retry · s skip · q stop [r]: " "r")"
            case "$answer" in
                s|S) info "skipped"; break ;;
                q|Q) info "stopped on request"; return 1 ;;
                *)   ;;
            esac
        done
    done
    return 0
}

# Turns "1 3 5" into component indices, rejecting anything out of range.
parse_selection() {
    local raw="$1" token picked=()
    for token in $raw; do
        [[ "$token" =~ ^[0-9]+$ ]] || { warn "ignoring invalid value: $token" >&2; continue; }
        if (( token < 1 || token > ${#COMPONENTS[@]} )); then
            warn "number out of range: $token" >&2
            continue
        fi
        picked+=("$((token - 1))")
    done
    printf '%s\n' "${picked[@]+"${picked[@]}"}"
}

wizard() {
    cat <<EOF

${C_BOLD}HC_IME${C_RESET} — Vietnamese / Hán Nôm input method for Fcitx5
  ${PRETTY_NAME:-$ID} · ${SESSION_TYPE} · $TARGET_USER ($TARGET_HOME)
EOF
    scan_components
    print_status_table

    local pending=()
    mapfile -t pending < <(pending_indices)
    if [[ ${#pending[@]} -eq 0 ]]; then
        ok "everything is in place, nothing to install"
        info "to reinstall a specific piece, choose c below"
    fi

    local labels=() index
    for index in "${pending[@]+"${pending[@]}"}"; do labels+=("$((index + 1))"); done

    printf '  %sa%s) install what is missing  (%s)\n' "$C_BOLD" "$C_RESET" "${labels[*]:-none}"
    printf '  %sc%s) choose components yourself\n' "$C_BOLD" "$C_RESET"
    printf '  %sq%s) quit\n\n' "$C_BOLD" "$C_RESET"

    local choice selection=()
    choice="$(ask "  Choose [a]: " "a")"
    case "$choice" in
        c|C)
            local raw
            raw="$(ask "  Enter numbers separated by spaces: " "")"
            mapfile -t selection < <(parse_selection "$raw")
            ;;
        q|Q) info "quit"; return 0 ;;
        *)   selection=("${pending[@]+"${pending[@]}"}") ;;
    esac

    if [[ ${#selection[@]} -eq 0 ]]; then
        info "nothing selected, quitting"
        return 0
    fi

    guided_run "${selection[@]}" || true

    printf '\n%sResult%s\n' "$C_BOLD" "$C_RESET"
    scan_components
    print_status_table
    local still=()
    mapfile -t still < <(pending_indices)
    if [[ ${#still[@]} -eq 0 ]]; then
        print_summary
    else
        info "${#still[@]} item(s) left; run this script again to continue"
    fi
}

# ------------------------------------------------------------------ main --

main() {
    check_environment

    if (( DO_UNINSTALL )); then
        uninstall
        exit 0
    fi

    if (( DO_UPDATE )); then
        update
        exit 0
    fi

    if (( DO_STATUS )); then
        print_status
        exit 0
    fi

    # The guided run is the front door. Flags stay for automation: -y means
    # "do not ask me anything", which is the opposite of a wizard, and a run
    # with no terminal has nobody to ask.
    if (( ! ASSUME_YES )) && interactive_available; then
        wizard
        exit 0
    fi

    cat <<EOF

${C_BOLD}HC_IME installer${C_RESET}

Building and configuring for: ${C_BOLD}$TARGET_USER${C_RESET} ($TARGET_HOME)
Root steps run: $( (( RUNNING_AS_ROOT )) && echo "directly (already under sudo)" || echo "through sudo, one step at a time" )

This will:
  1. Install any missing apt build/runtime dependencies$( (( WITH_FONTS )) && echo " and Hán Nôm fonts" ).
  2. $( (( SKIP_TESTS )) && echo "Skip the Rust core tests." || echo "Run the Rust core test suite." )
  3. Build the Rust core and the Fcitx5 addon in $BUILD_DIR.
  4. Stop Fcitx5, then install system-wide as root.$( (( DO_CONFIG )) && cat <<'INNER'

  5. Add hcime to your Fcitx5 profile and make it the default input method.
  6. Set the ClassicUI candidate font so Hán Nôm glyphs render.
INNER
)
  $( (( DO_CONFIG )) && echo 7 || echo 5 ). Restart Fcitx5 and verify the addon loaded.

Existing configuration files are backed up before any change.
EOF

    confirm "Proceed?" || die "aborted."

    install_packages
    (( WITH_FONTS )) && install_fonts
    ensure_rust
    check_fcitx5_version
    run_tests
    build_addon

    # Always stop Fcitx5 before installing: cmake overwrites libhcime.so and
    # libhc_core.so in place, which can crash a daemon that has them mapped.
    stop_fcitx5
    install_addon

    if (( DO_CONFIG )); then
        configure_profile
        configure_font
        configure_environment
    else
        info "skipping Fcitx5 configuration (--no-config)"
        info "add hcime to your input methods with: fcitx5-configtool"
    fi

    start_fcitx5
    verify_installation
    print_summary
}

main "$@"
