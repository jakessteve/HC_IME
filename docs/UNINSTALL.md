# HC_IME — Uninstall

Removing HC_IME, in whole or in part. For installation see
[INSTALL.md](INSTALL.md).

## Two guarantees

- **Fcitx5 is never removed.** Uninstalling HC_IME leaves your input-method
  framework, and every other input method that uses it, untouched.
- **No package is removed unless this installer is the one that installed it.**
  The installer records what it added in `~/.local/share/hcime/receipt.ini`; a
  package that was already on the machine is never a candidate for removal.

Everything else is opt-in, component by component.

## Guided uninstall (Debian, Ubuntu)

```bash
./scripts/uninstall.sh
```

```
  #  Component                      What happens
  ─────────────────────────────────────────────────────────────────
  1  Installed addon (.so + .conf)  ✓ 4 file(s) from the manifest
  2  hcime in the Fcitx5 profile    ✓ remove from the group, hand DefaultIM to another IM
  3  Candidate window font          ✓ restore from classicui.conf.hcime-backup-…
  4  Environment drop-in            ✓ remove 90-hcime-fcitx5.conf
  5  Hán Nôm font packages (apt)    ✓ fonts-hanazono
  ─  Fcitx5 and build packages      NOT removed

  a) remove addon + configuration  (1 2 3 4)
  c) choose components yourself
  q) quit
```

`a` removes the addon and its configuration and leaves the fonts. `c` takes
component numbers, so "remove the fonts but keep the IME" is `5`, and "remove
the IME but keep the fonts" is `1 2 3 4`. Each step confirms before it runs, and
a failure moves on to the next component rather than aborting.

A row marked `·` instead of `✓` is not applicable — most often component 5 when
the fonts were already on the machine before HC_IME.

## What each component removes

| Component | Effect |
| --- | --- |
| Addon | The files listed in `~/.local/share/hcime/install_manifest.txt`: `libhcime.so`, `libhc_core.so`, and the two `hcime.conf` metadata files |
| Profile | Removes `hcime` from every Fcitx5 input-method group and hands `DefaultIM` to another entry. The profile is backed up first |
| Candidate font | Restores `classicui.conf` from the backup taken at install time. With no backup, removes only the `Font=` line the installer added |
| Environment | Deletes `~/.config/environment.d/90-hcime-fcitx5.conf` |
| Font packages | `apt-get remove` for font packages the receipt says this installer added — nothing else |

Backups are always left in place. Nothing under `~/.local/share/hcime/` other
than the manifest is deleted, so a later reinstall still knows what it did.

## Machines installed by hand

An installation built and installed manually has no manifest. The uninstaller
still finds it, by asking Fcitx5 where addons live rather than trusting its own
records, and lists it as what it is:

```
  1  Installed addon (.so + .conf)  ✓ 4 file(s) found on disk, not recorded by this script
```

Because these are files the script did not put there, it prints every path and
asks a second time before removing them.

## Manual uninstall

Works on any distribution, and is the current path on Arch/CachyOS until the
guided uninstaller supports pacman.

```bash
# 1. The installed files, at the paths Fcitx5 itself reports
ADDON_DIR="$(pkg-config --variable=libdir Fcitx5Core)/fcitx5"
DATA_DIR="$(pkg-config --variable=prefix Fcitx5Core)/share/fcitx5"

sudo rm -f "$ADDON_DIR/libhcime.so" "$ADDON_DIR/libhc_core.so" \
           "$DATA_DIR/addon/hcime.conf" "$DATA_DIR/inputmethod/hcime.conf"

# or, if the manifest exists:
sudo xargs -d '\n' -a ~/.local/share/hcime/install_manifest.txt rm -f --

# 2. Per-user configuration
rm -f ~/.config/environment.d/90-hcime-fcitx5.conf
rm -f ~/.config/conf/hcime.conf          # HC_IME's own settings, see INSTALL.md
rmdir ~/.config/conf 2>/dev/null || true

# 3. Restore the candidate font, if the installer changed it
ls ~/.config/fcitx5/conf/classicui.conf.hcime-backup-*
cp ~/.config/fcitx5/conf/classicui.conf.hcime-backup-<stamp> \
   ~/.config/fcitx5/conf/classicui.conf

# 4. Remove hcime from the input-method group, then restart
gdbus call --session --dest org.fcitx.Fcitx5 --object-path /controller \
  --method org.fcitx.Fcitx.Controller1.InputMethodGroupInfo "Default"
gdbus call --session --dest org.fcitx.Fcitx5 --object-path /controller \
  --method org.fcitx.Fcitx.Controller1.SetInputMethodGroupInfo \
  "Default" "us" "[('keyboard-us','')]"
fcitx5 -r
```

Step 4 rewrites the group with the entries you want to keep; read it first with
`InputMethodGroupInfo` and drop only the `hcime` pair.

Do not `apt-get remove fcitx5` or `pacman -R fcitx5` to uninstall HC_IME — that
removes the framework every input method on the machine depends on.

## Verifying it is gone

```bash
ls "$(pkg-config --variable=libdir Fcitx5Core)/fcitx5/libhcime.so"    # should not exist
gdbus call --session --dest org.fcitx.Fcitx5 --object-path /controller \
  --method org.fcitx.Fcitx.Controller1.AvailableInputMethods | grep hcime   # no match
./scripts/install.sh --status                                          # rows 5–8 back to ✗
```
