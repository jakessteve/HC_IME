# Arch / CachyOS support — plan

Working document for making the tooling work on Arch-family systems, not just
Debian. Written against a real situation: one CachyOS machine runs HC_IME today,
built and installed by hand, having never run any script in `scripts/`.

Status keys used below: **done** · **next** · **planned** · **needs a CachyOS
machine**.

## Context

| | Debian/Ubuntu box | CachyOS box |
| --- | --- | --- |
| How it was installed | `scripts/install.sh` | manual `cmake --install` |
| Manifest / receipt | present | none |
| Package manager | apt | pacman (+ AUR via `paru`, shipped by CachyOS) |
| Addon directory | `/usr/lib/x86_64-linux-gnu/fcitx5` | `/usr/lib/fcitx5` |
| `/etc/os-release` | `ID=ubuntu` | `ID=cachyos`, `ID_LIKE=arch` |

Distribution detection must match `ID_LIKE`, exactly as the Debian branch
already does for its derivatives. Known Arch-family IDs worth accepting:
`arch`, `cachyos`, `endeavouros`, `manjaro`, `garuda`, `artix`.

## What is already portable — verified

Checked on the Debian machine, and portable by construction:

- **Core and build.** Neither `hc_core/`, `linux_fcitx5/src/hcime.cpp`, nor the
  CMake files contain a distribution-specific path. The install directory comes
  from `FCITX_INSTALL_ADDONDIR`, resolved by Fcitx5Core per system.
- **Config location.** `StandardPathsType::PkgConfig` is XDG, not distro
  specific. The pending config-path fix needs no Arch-specific branch.
- **Fcitx5 API.** The addon already uses the `StandardPaths` API introduced in
  recent Fcitx5. Since the CachyOS build runs, that API is present there; the
  planned fix adds no new version floor.
- **Detection.** `pkg-config --variable=libdir Fcitx5Core` yields
  `/usr/lib/x86_64-linux-gnu` on Debian and `/usr/lib` on Arch, so
  `$libdir/fcitx5` is the addon directory everywhere. `prefix` gives
  `/usr/share/fcitx5` for the metadata.

## Done

**1. Detection independent of our own bookkeeping.**
`--status`, `update.sh`, and `uninstall.sh` used to decide "is it installed?"
from `~/.local/share/hcime/install_manifest.txt`, which a hand-built machine
never has — the CachyOS box was invisible to every one of them. They now locate
the addon through Fcitx5 (`pkg-config` libdir, plus a D-Bus check that the
running Fcitx5 actually loaded it). The manifest is now only the record of what
the script installed, used by the uninstaller.

Consequences, verified by hiding the manifests on the Debian box to simulate
CachyOS:

- `--status` reports the real state, with two new honest states: *installed by
  hand, not recorded by this script*, and *installed but Fcitx5 has not loaded
  it* (which is the symptom of a too-old Fcitx5 core).
- `update.sh` offers to adopt the installation instead of refusing, and starts
  keeping records from then on.
- `uninstall.sh` lists the files it found on disk, marks them as not its own,
  and confirms twice before removing them.

**2. The smoke gate no longer hardcodes a distribution layout.**
`scripts/e2e-smoke.sh` asserted paths under `/usr/lib/fcitx5` — the Arch layout,
which meant the gate passed on Arch and failed on Debian multiarch. It now reads
every path from the CMake install manifest. The full gate passes on Debian.

## Next

**3. Split the wizard core from apt.**

Everything except two of the eight components is package-manager agnostic. The
proposed layout:

```
scripts/lib/hcime-wizard.sh   # registry, detect, menu, guided run, receipt, uninstall
scripts/lib/pm-apt.sh         # Debian, Ubuntu
scripts/lib/pm-pacman.sh      # Arch, CachyOS
scripts/lib/pm-manual.sh      # anything else: print commands, install nothing
```

A backend implements four functions: `pm_installed`, `pm_available`,
`pm_install`, `pm_remove`. Do this split *before* adding pacman, so the Debian
behaviour can be shown unchanged first.

Rename `install-debian.sh` to `install-linux.sh`, keeping the old name as a
symlink.

**4. The pacman backend.** *(needs a CachyOS machine to accept)*

Package differences are structural, not just naming:

| Role | Debian / Ubuntu | Arch / CachyOS |
| --- | --- | --- |
| Compiler + make | `build-essential` | `base-devel` |
| Build tools | `cmake ninja-build extra-cmake-modules pkg-config gettext` | `cmake ninja extra-cmake-modules pkgconf gettext` |
| Fcitx5 headers | `libfcitx5core-dev`, `fcitx5-modules-dev` | none — shipped inside `fcitx5` |
| Fcitx5 runtime | `fcitx5`, `fcitx5-config-qt` | `fcitx5`, `fcitx5-configtool` |
| Frontends | `fcitx5-frontend-gtk3/gtk4/qt5/qt6` | `fcitx5-gtk`, `fcitx5-qt` |
| Rust | `rustc`, `cargo` | `rust` (or rustup) |
| CJK fonts | `fonts-noto-cjk`, `fonts-noto-cjk-extra` | `noto-fonts-cjk` |
| Extension B font | `fonts-hanazono` | `ttf-hanazono` — **AUR only** |

`pacman -S ttf-hanazono` fails: it is not in the official repositories. The
backend should detect `paru` or `yay` and use it for AUR-only names, and
otherwise print the command instead of failing the step — the same best-effort
treatment optional packages already get on apt.

`im-config` does not exist on Arch; the environment component writes
`~/.config/environment.d/`, which is systemd and works on both.

**5. Adopt-an-existing-install polish.** The adoption path is implemented but
has only been exercised through a simulation on Debian. Run it for real on the
CachyOS box once the pacman backend lands.

**6. Config path fix.** Independent of distribution, but it will migrate the
CachyOS machine's settings too. See the summary below.

**7. Uninstall gains an "HC_IME settings" component**, covering
`~/.config/fcitx5/conf/hcime.conf`, the legacy `~/.config/conf/hcime.conf`, and
an empty `~/.config/conf/` directory.

## The config path fix, in brief

`linux_fcitx5/src/hcime.cpp` passes `StandardPathsType::Config` (`~/.config`)
with the relative path `conf/hcime.conf`, so settings land in
`~/.config/conf/hcime.conf` instead of `~/.config/fcitx5/conf/hcime.conf`. It
reads back from the same wrong place, so nothing is lost today — but the file
sits outside the Fcitx5 config tree, and `~/.config/conf/` risks colliding with
other software. This affects the CachyOS machine identically.

The fix is `StandardPathsType::PkgConfig`, plus migration, because changing the
constant alone would silently reset every existing user to defaults:

```
new_exists    = locate(PkgConfig, "conf/hcime.conf") is non-empty
legacy_exists = locate(Config,    "conf/hcime.conf") is non-empty

if !new_exists && legacy_exists:  read legacy, from_legacy = true
else:                             read new
migrateLegacyConfig(raw)          # the existing flat-key migration composes here
config_.load(raw, true)
if from_legacy || hadLegacyKeys:  save to PkgConfig
if from_legacy:                   rename the legacy file to hcime.conf.moved-to-fcitx5
```

`readAsIni` returns `void`, so existence has to be tested with
`StandardPaths::global().locate(...)` — that is why the sketch above uses it.
`safeSaveAsIni` returns a `bool` that both current call sites ignore; the fix
should log on failure rather than lose settings quietly.

Cases to cover: both files exist (new wins, legacy renamed); neither (defaults);
legacy file still using flat keys (both migrations in one pass, one save); write
failure; downgrade to an older build (falls back to defaults, data recoverable
from the renamed file); `instance_ == nullptr`, as in the bridge probe, where
all file I/O stays skipped.

**Why no test caught this.** `linux_fcitx5/tests/bridge_probe.cpp` builds the
engine with `HcImeEngine(nullptr)`, so `save()` and `reloadConfig()` never touch
a file. The fix needs an integration test that runs a real Fcitx5:

```bash
dbus-run-session -- env XDG_CONFIG_HOME=$tmp \
  FCITX_ADDON_DIRS=$BUILD_DIR/linux_fcitx5:/usr/lib/<...>/fcitx5 \
  fcitx5 --disable=all --enable=hcime,dbus,dbusfrontend -D
gdbus call ... SetConfig "fcitx://config/addon/hcime" "<{'Behavior': <{'QuickConsonants': <'True'>}>}>"
test -f "$tmp/fcitx5/conf/hcime.conf"     # new location
test ! -f "$tmp/conf/hcime.conf"          # not the old one
```

Every mechanism in that snippet — private bus, addon-directory override, D-Bus
`SetConfig` round trip — was exercised by hand on the Debian box and works.
Migration is checked by seeding `$tmp/conf/hcime.conf` first and asserting the
value survives at the new path.

## Verification checklist for the CachyOS machine

Read-only, no root, safe to run at any point:

```bash
./scripts/install.sh --status                    # component table vs reality
pkg-config --modversion Fcitx5Core               # API floor, expect >= 5.1.19
pkg-config --variable=libdir Fcitx5Core          # expect /usr/lib
ls ~/.config/conf/hcime.conf ~/.config/fcitx5/conf/hcime.conf   # which path is in use
gdbus call --session --dest org.fcitx.Fcitx5 --object-path /controller \
  --method org.fcitx.Fcitx.Controller1.AvailableInputMethods | grep hcime
pacman -Q fcitx5 fcitx5-configtool fcitx5-gtk fcitx5-qt rust noto-fonts-cjk
```

The last line is what confirms or corrects the package table in step 4 — it is
written from knowledge of Arch, not from a machine, and is the one part of this
document that has not been verified anywhere.

## Order of work

1. ~~Detection independent of the manifest~~ — done
2. ~~Smoke gate reads the CMake manifest~~ — done
3. Split the wizard core, keep Debian behaviour identical — next
4. `pm-pacman.sh` and `pm-manual.sh`
5. Adoption tested for real on CachyOS
6. Config path fix, migration, integration test
7. Uninstall settings component, README and INSTALL.md updates

Steps 3 and 6 can be verified entirely on the Debian machine. Step 4 cannot; it
needs someone at a CachyOS box running the checklist above.
