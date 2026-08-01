# HC_IME — Setup, Install, and Check

How to get HC_IME running, and how to find out what state a machine is in.
For removal, see [UNINSTALL.md](UNINSTALL.md). For the Arch/CachyOS work that is
still outstanding, see [ARCH_CACHYOS_PLAN.md](ARCH_CACHYOS_PLAN.md).

## Distribution support

| Distribution | Guided installer | Manual build |
| --- | --- | --- |
| Debian, Ubuntu, derivatives | Yes | Yes |
| Arch, CachyOS, derivatives | Not yet — see [the plan](ARCH_CACHYOS_PLAN.md) | Yes |
| Anything else | No | Yes |

The Rust core, the C++ addon, and the CMake install rules carry no
distribution-specific assumptions: the install directory comes from
`FCITX_INSTALL_ADDONDIR`, which Fcitx5Core resolves per system. Only the
installer's package step is apt-specific today.

## What an install consists of

Eight pieces. The guided installer, the status screen, and the uninstaller are
all views over this same list, so they can never disagree.

| # | Component | What it is |
| --- | --- | --- |
| 1 | Build + Fcitx5 packages | Compiler, CMake, Ninja, Fcitx5 and its development files |
| 2 | Hán Nôm fonts | CJK Extension B coverage for rare glyphs |
| 3 | Rust toolchain | `cargo` / `rustc` 1.70 or newer |
| 4 | Build the addon | `cargo test`, then `cmake --build` |
| 5 | Install the addon | `cmake --install`, needs root |
| 6 | Register hcime | Adds `hcime` to your Fcitx5 input-method group |
| 7 | Candidate window font | ClassicUI font — **applies to every input method, not just hcime** |
| 8 | Input-method environment | `~/.config/environment.d/90-hcime-fcitx5.conf` |

Items 1–5 are system-wide; 6–8 are per-user.

## Guided install (Debian, Ubuntu)

```bash
./scripts/install.sh
```

It shows what is already in place, then walks through what is missing:

```
  #  Component                      Status
  ─────────────────────────────────────────────────────────────────
  1  Build + Fcitx5 packages (apt)  ✓ all present
  2  Hán Nôm fonts                  ✗ missing: fonts-hanazono
  3  Rust toolchain                 ✓ rustc 1.97.1
  4  Build the addon and run tests  ✗ not built yet
  5  Install the addon system-wide  ✗ not installed
                                   ↳ needs root
  6  Register hcime with Fcitx5     ✗ not in the group
  7  Candidate window font          ✗ a different font is set
                                   ↳ applies to EVERY input method, not just hcime
  8  Input-method environment       ✓ environment already points at fcitx

  a) install what is missing  (2 4 5 6 7)
  c) choose components yourself
  q) quit
```

Status marks:

| Mark | Meaning |
| --- | --- |
| `✓` | in place, nothing to do |
| `✗` | not there yet |
| `↻` | there, but out of date — usually an installed addon older than your build |
| `!` | there, with something worth reading first |

Each step prints the command it is about to run, then waits: `Enter` to run,
`s` to skip, `q` to stop. On failure it offers `r` to retry, `s` to skip, `q` to
stop — one failed step does not abandon the run, and Fcitx5 is restarted if a
failed step had stopped it.

Re-running is safe and is how you continue later, or install one piece you
skipped: it re-detects everything and only offers what is still missing.

Root is only needed for apt and `cmake --install`. Both forms work:

```bash
./scripts/install.sh          # sudo is called only for the steps that need it
sudo ./scripts/install.sh     # build and ~/.config changes drop back to $SUDO_USER
```

For CI, `-y` skips the guided run and installs everything in order. See
`./scripts/install.sh --help`.

## Manual build (Arch, CachyOS, and everything else)

Dependencies, by distribution:

```bash
# Debian / Ubuntu
sudo apt-get install build-essential cmake ninja-build extra-cmake-modules \
    pkg-config gettext libfcitx5core-dev fcitx5-modules-dev fcitx5 \
    fcitx5-config-qt fonts-noto-cjk fonts-noto-cjk-extra fonts-hanazono

# Arch / CachyOS — fcitx5 ships its own headers, there is no -dev package
sudo pacman -S base-devel cmake ninja extra-cmake-modules pkgconf gettext \
    fcitx5 fcitx5-configtool fcitx5-gtk fcitx5-qt rust noto-fonts-cjk
paru -S ttf-hanazono        # AUR: Extension B coverage, optional
```

> The Debian list is verified on a running machine. The Arch list is written
> from the package names Arch uses and has not yet been confirmed on a CachyOS
> box — if a name is wrong there, correct it here and in
> [ARCH_CACHYOS_PLAN.md](ARCH_CACHYOS_PLAN.md).

Then build and install:

```bash
cargo test --manifest-path hc_core/Cargo.toml
cmake -S . -B build -G Ninja -DFCITX_INSTALL_USE_FCITX_SYS_PATHS=ON
cmake --build build
sudo cmake --install build
fcitx5 -r
```

Add `hcime` to your input-method group with `fcitx5-configtool`, or over D-Bus
(see "Configuring without the GUI" below).

A machine installed this way is still recognised by the tooling: `--status`,
`update.sh`, and `uninstall.sh` locate the addon through Fcitx5 itself rather
than through any record the scripts keep, and `update.sh` offers to adopt it.

## Checking a machine

```bash
./scripts/install.sh --status
```

Read-only, needs no root, and works on any distribution — the package row is the
only one that needs apt. This is the fastest way to answer "what is missing
here?" and the first thing to run when reporting a problem.

Lower-level checks that do not involve the scripts at all:

```bash
# Where Fcitx5 loads addons from on this machine, and is HC_IME there?
ls "$(pkg-config --variable=libdir Fcitx5Core)/fcitx5/libhcime.so"

# Has the running Fcitx5 actually loaded it?
gdbus call --session --dest org.fcitx.Fcitx5 --object-path /controller \
  --method org.fcitx.Fcitx.Controller1.AvailableInputMethods | grep hcime

# Is the Fcitx5 core new enough for this addon (hcime.conf declares 5.1.19)?
pkg-config --modversion Fcitx5Core

# Current input method, and the group contents
fcitx5-remote -n
gdbus call --session --dest org.fcitx.Fcitx5 --object-path /controller \
  --method org.fcitx.Fcitx.Controller1.InputMethodGroupInfo "Default"
```

An addon present on disk but absent from `AvailableInputMethods` almost always
means the running Fcitx5 is older than the version the addon declares.

## Updating after a code change

```bash
./scripts/update.sh
```

Rebuilds incrementally, reinstalls, restarts Fcitx5. Never touches apt, fonts,
or anything under `~/.config`. If the rebuild produced nothing new it says so
and leaves Fcitx5 alone. On a machine that was built by hand it offers to adopt
the existing installation instead of refusing.

## Configuring without the GUI

`fcitx5-configtool` can hang on some Wayland desktops — it blocks with no CPU
use, waiting on a call that never returns. Everything it does is available over
D-Bus, and answers in well under a second:

```bash
FC="gdbus call --session --dest org.fcitx.Fcitx5 --object-path /controller \
    --method org.fcitx.Fcitx.Controller1"

$FC.GetConfig "fcitx://config/addon/hcime"

# Partial writes merge: keys you do not mention keep their values.
$FC.SetConfig "fcitx://config/addon/hcime" "<{'Input': <{'InputMethod': <'VNI'>}>}>"
$FC.SetConfig "fcitx://config/addon/hcime" "<{'Output': <{'OutputMode': <'SurroundingText'>}>}>"
$FC.SetConfig "fcitx://config/addon/classicui" "<{'Font': <'Hanom PV,Noto Sans CJK SC 17'>}>"
```

Accepted values:

| Key | Values |
| --- | --- |
| `Input/InputMethod` | `Telex` `VNI` `VIQR` `HanNomTelex` `HanNomVni` `HanNomViqr` |
| `Output/OutputMode` | `Preedit` `SurroundingText` |
| `Behavior/EnglishProtection` | `Off` `Soft` `Hard` |
| other `Behavior/*` keys | `True` / `False` |

Changes apply immediately, with no restart.

> **Known issue.** HC_IME writes its settings to `~/.config/conf/hcime.conf`
> instead of `~/.config/fcitx5/conf/hcime.conf`. It reads back from the same
> place, so nothing is lost, but the file sits outside the Fcitx5 config tree.
> A fix with migration is planned; see [ARCH_CACHYOS_PLAN.md](ARCH_CACHYOS_PLAN.md).

## Files an install touches

| Path | Written by |
| --- | --- |
| `$(pkg-config --variable=libdir Fcitx5Core)/fcitx5/libhcime.so`, `libhc_core.so` | `cmake --install` |
| `$(pkg-config --variable=prefix Fcitx5Core)/share/fcitx5/addon/hcime.conf` | `cmake --install` |
| `.../share/fcitx5/inputmethod/hcime.conf` | `cmake --install` |
| `~/.config/fcitx5/profile` | component 6 (backed up first) |
| `~/.config/fcitx5/conf/classicui.conf` | component 7 (backed up first) |
| `~/.config/environment.d/90-hcime-fcitx5.conf` | component 8 |
| `~/.config/conf/hcime.conf` | the addon itself — see the known issue above |
| `~/.local/share/hcime/install_manifest.txt` | record of installed files |
| `~/.local/share/hcime/receipt.ini` | record of what the installer added |

Backups are written next to the original as `.hcime-backup-<timestamp>` and are
never removed automatically.

## Troubleshooting

**Nothing types Vietnamese.** Check the framework first: on Ubuntu the default
is IBus, and HC_IME is an Fcitx5 addon.

```bash
im-config -n fcitx5     # then log out and back in; revert with: im-config -n ibus
```

**The addon builds but never loads.** Compare `pkg-config --modversion
Fcitx5Core` against the `5.1.19` that `hcime.conf` declares. Fcitx5 refuses an
addon whose declared dependency is newer than the running core. Ubuntu 24.04 can
be too old; Ubuntu 26.04 and Arch/CachyOS are current.

**Applications do not pick it up.** Log out and back in — component 8 writes a
systemd user environment file, which is only read at session start.

**Validation gate.** `scripts/e2e-smoke.sh` runs formatting, the installer
script checks, dictionary regeneration, Rust tests, Clippy, the addon build, the
Fcitx5 bridge probe, and a staged install verified against the CMake manifest.
