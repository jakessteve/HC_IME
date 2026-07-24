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
CANDIDATE_FONT='Hanom PV,HAN NOM B,HAN NOM A,Noto Sans CJK SC,HanaMinA,HanaMinB,Jigmo,Jigmo2,Jigmo3 28'

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

Both forms work. Under sudo the build and every change under your home
directory are performed as $SUDO_USER, not as root.

Options:
  -y, --yes         Do not prompt for confirmation.
      --skip-tests  Skip `cargo test` before building the addon.
      --no-fonts    Do not install the Hán Nôm CJK fonts.
      --no-config   Install only; do not touch your Fcitx5 configuration.
      --uninstall   Remove a previous installation made by this script.
  -h, --help        Show this help.

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
        --uninstall)  DO_UNINSTALL=1 ;;
        -h|--help)    usage; exit 0 ;;
        *)            usage >&2; die "unknown option: $1" ;;
    esac
    shift
done

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

# Prefix for the copyable commands printed to the user. Someone already inside
# `sudo ./install.sh` should not be told to type sudo a second time.
SUDO_HINT=""
(( RUNNING_AS_ROOT )) || SUDO_HINT="sudo "

confirm() {
    local prompt="$1"
    (( ASSUME_YES )) && return 0
    if [[ ! -t 0 ]]; then
        die "$prompt (no terminal to ask on; re-run with --yes)"
    fi
    local reply
    read -r -p "    $prompt [Y/n] " reply
    [[ -z "$reply" || "$reply" =~ ^[Yy] ]]
}

backup_file() {
    local path="$1"
    [[ -f "$path" ]] || return 0
    as_user cp -p "$path" "$path.hcime-backup-$BACKUP_STAMP"
    info "backed up $path -> $(basename "$path").hcime-backup-$BACKUP_STAMP"
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

install_packages() {
    step "Installing build and runtime dependencies"

    local font_pkgs=(fonts-noto-cjk fonts-noto-cjk-extra fonts-hanazono)
    local required=("${REQUIRED_PKGS[@]}")
    local optional=("${OPTIONAL_PKGS[@]}")
    (( WITH_FONTS )) && optional+=("${font_pkgs[@]}")

    # First pass (no root needed): what is already installed?
    local pkg missing_required=() missing_optional=()
    for pkg in "${required[@]}"; do
        pkg_installed "$pkg" || missing_required+=("$pkg")
    done
    for pkg in "${optional[@]}"; do
        pkg_installed "$pkg" || missing_optional+=("$pkg")
    done

    if [[ ${#missing_required[@]} -eq 0 && ${#missing_optional[@]} -eq 0 ]]; then
        ok "all apt dependencies are already installed"
        return 0
    fi

    # We are going to touch apt, so refresh the lists first; availability
    # filtering below relies on an up-to-date cache.
    info "these packages are not installed yet:"
    printf '      %s\n' "${missing_required[@]}" "${missing_optional[@]}"
    if ! sudo_noninteractive && [[ ! -t 0 ]]; then
        die "cannot run apt without a password prompt here. Install the packages above, then re-run this script."
    fi
    confirm "Update apt and install them now?" \
        || die "dependencies are required. Install the packages above, then re-run this script."

    run_root apt-get update || warn "apt-get update reported problems; continuing with the current cache"

    # Availability filter: drop anything apt does not actually offer on this
    # release. Missing required packages are fatal; missing optional ones warn.
    local to_install=() unavailable_required=() unavailable_optional=()
    for pkg in "${missing_required[@]}"; do
        if pkg_available "$pkg"; then to_install+=("$pkg"); else unavailable_required+=("$pkg"); fi
    done
    for pkg in "${missing_optional[@]}"; do
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
    ok "apt dependencies installed"

    if (( WITH_FONTS )); then
        as_user fc-cache -f >/dev/null 2>&1 || true
        ok "font cache refreshed"
    fi
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

# ------------------------------------------------------------- uninstall --

uninstall() {
    step "Uninstalling HC_IME"
    local manifest=""
    if [[ -f "$MANIFEST_STORE" ]]; then
        manifest="$MANIFEST_STORE"
    elif [[ -f "$BUILD_DIR/install_manifest.txt" ]]; then
        manifest="$BUILD_DIR/install_manifest.txt"
    else
        die "no install manifest found at $MANIFEST_STORE or $BUILD_DIR/install_manifest.txt; nothing to uninstall."
    fi
    info "using manifest: $manifest"

    info "the following files will be removed:"
    sed 's/^/      /' "$manifest"
    info "this is done as root with:"
    copyable "${SUDO_HINT}xargs -d '\\n' -a $manifest rm -f --"
    confirm "Remove them?" || die "aborted."

    stop_fcitx5
    # -d '\n' so paths are split only on newlines; xargs otherwise treats spaces,
    # quotes and backslashes as separators.
    run_root xargs -d '\n' -a "$manifest" rm -f --
    as_user rm -f "$MANIFEST_STORE"
    ok "removed installed files"

    if [[ -f "$PROFILE_PATH" ]] && grep -q 'hcime' "$PROFILE_PATH"; then
        if confirm "Also remove hcime from your Fcitx5 profile?"; then
            backup_file "$PROFILE_PATH"
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
            ok "profile cleaned"
        fi
    fi

    if [[ -f "$ENV_FILE" ]]; then
        as_user rm -f "$ENV_FILE"
        ok "removed $ENV_FILE"
    fi
    start_fcitx5
    printf '\n%sHC_IME removed.%s Font and other backups were left in place.\n' "$C_BOLD" "$C_RESET"
}

# ------------------------------------------------------------------ main --

main() {
    check_environment

    if (( DO_UNINSTALL )); then
        uninstall
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
    check_fonts
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
