# BlocKuntu Uninstall

This document describes the current uninstall behavior for the Debian package
and the older production-style script. The two paths are intentionally separate:
the Debian package is the normal user-facing install path, while
`scripts/install-production.sh` and `scripts/uninstall-production.sh` are manual
development/admin helpers.

## Debian Package Uninstall

For package installs, use the GUI Admin tab when possible. Type either valid
uninstall phrase and run the uninstall action.

The GUI validates the phrase locally, then runs:

```bash
pkexec dpkg --purge blockuntu
```

The terminal equivalent is:

```bash
sudo dpkg --purge blockuntu
```

The GUI path therefore has the same cleanup behavior as a Debian package purge.

## Uninstall Phrases

The GUI accepts two generated phrases:

| Phrase                 | Path                                                                                                           | Owner and mode           | Created by                                    | Displayed in GUI |
| ---------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------ | --------------------------------------------- | ---------------- |
| First-run phrase       | `$XDG_DATA_HOME/blockuntu/uninstall-confirmation.txt` or `~/.local/share/blockuntu/uninstall-confirmation.txt` | current user, `0600`     | GUI on first read if missing or empty         | Yes              |
| System recovery phrase | `/etc/blockuntu/uninstall-recovery.txt`                                                                        | `root:blockuntu`, `0640` | Debian package `postinst` if missing or empty | No               |

Both phrases are generated from 24 random bytes from `/dev/urandom` and formatted
as uppercase hex chunks. The normal first-run phrase starts with
`BLOCKUNTU-UNINSTALL-`. The system recovery phrase starts with
`BLOCKUNTU-UNINSTALL-RECOVERY-`.

The recovery phrase is not a hardcoded master password. It is generated on the
installed system and preserved across package upgrades as long as the file
already exists.

To read the system recovery phrase:

```bash
sudo cat /etc/blockuntu/uninstall-recovery.txt
```

After the desktop user has logged in with `blockuntu` group membership, this may
also work without `sudo`:

```bash
cat /etc/blockuntu/uninstall-recovery.txt
```

## Phrase Validation

The frontend only checks that the uninstall input is non-empty. The Tauri backend
is the authority:

1. It trims the input.
2. It compares the input with the per-user first-run phrase.
3. If that does not match, it reads `/etc/blockuntu/uninstall-recovery.txt`.
4. It accepts the input if it exactly matches any non-empty line in that file.
5. If neither phrase matches, uninstall is rejected before `pkexec` is invoked.

If the recovery phrase file is missing or unreadable to the GUI process, the GUI
still works with the first-run phrase. Permission errors for the recovery phrase
are treated as "recovery phrase unavailable", not as a hard GUI failure.

## What Package Purge Removes

The package `prerm` stops and disables BlocKuntu services, allows them to stop
cleanly even when manual stop is normally refused, and removes runtime
enforcement state before package files disappear.

Package purge removes or cleans:

- `blockuntu.socket`
- `blockuntu.service`
- `blockuntu-watchdog.service`
- `blockuntu-hosts.path`
- `blockuntu-hosts.service`
- `/run/blockuntu`
- the BlocKuntu managed block in `/etc/hosts`
- system Native Messaging manifests installed by the package
- package binaries under `/usr/bin`
- package GUI launcher and icons
- `/etc/blockuntu`
- `/var/lib/blockuntu`

Browser policy cleanup is also part of package removal:

- Firefox: removes `/etc/firefox/policies/policies.json` only if the file exists
and looks BlocKuntu-owned by containing `blockuntu`.
- Chrome: removes `/etc/opt/chrome/policies/managed/blockuntu.json`.
- Chrome update manifest: removes
`/usr/local/share/blockuntu/chrome-extension-updates.xml` if present.

Restart Firefox or Chrome after uninstall if the browser still shows old managed
policy state.

## What Package Purge Does Not Remove

Package purge does not remove user-owned GUI state outside the package, including
the first-run uninstall phrase under `~/.local/share/blockuntu` or
`$XDG_DATA_HOME/blockuntu`.

Package purge also does not remove stale development Native Messaging manifests
or confined-browser Firefox files from user profile locations such as:

```text
~/.mozilla/native-messaging-hosts/blockuntu_native.json
~/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/blockuntu_native.json
~/.var/app/org.mozilla.firefox/data/blockuntu/blockuntu-native
~/.var/app/org.mozilla.firefox/data/blockuntu/BlocKuntu-Signed.xpi
$XDG_DATA_HOME/flatpak/extension/org.mozilla.firefox.systemconfig/*/stable/policies/policies.json
~/snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json
~/snap/firefox/common/.local/share/blockuntu/blockuntu-native
~/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json
~/.config/chromium/NativeMessagingHosts/blockuntu_native.json
```

Those user-level files can still matter if a browser extension was previously
connected to a development install.

If Firefox Flatpak was configured, remove the user override with:

```bash
flatpak override --user --nofilesystem=/run/blockuntu org.mozilla.firefox
```

## Remove Versus Purge

The GUI uses `dpkg --purge`, not a plain remove. That matters:

- `dpkg --remove blockuntu` removes package files but can preserve package data
and local files under `/etc/blockuntu`.
- `dpkg --purge blockuntu` removes package files and purges `/etc/blockuntu`,
`/var/lib/blockuntu`, and `/run/blockuntu`.

Use purge when the goal is a real package uninstall.

## Manual Production Script

If the machine was installed with `./scripts/install-production.sh`, use:

```bash
./scripts/uninstall-production.sh
```

That script does not use the GUI uninstall phrases. By default it removes the
manual `/usr/local` install, systemd units, Native Messaging manifests, runtime
files, current-user Firefox Snap/Flatpak helper files, and the BlocKuntu-managed
hosts block. For Firefox Flatpak, that includes the copied XPI and the per-user
`org.mozilla.firefox.systemconfig` policy if it looks BlocKuntu-owned.

It preserves system browser policy files by default. Remove them only when you
intentionally want to delete BlocKuntu-owned system browser policy state:

```bash
./scripts/uninstall-production.sh --remove-browser-policies
```

Optional destructive cleanup flags:

```bash
./scripts/uninstall-production.sh --purge-data
./scripts/uninstall-production.sh --remove-browser-policies
./scripts/uninstall-production.sh --remove-group
```

Use the package uninstall path for Debian package installs, and the script path
only for the manual production-style install.
