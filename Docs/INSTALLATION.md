# BlocKuntu Installation

BlocKuntu currently supports Debian and Ubuntu package installations. The Debian
package is the supported path; the manual script is for development or controlled
administrator use.

## Install the Debian package

Install the supplied package with `apt`:

```bash
sudo apt install ./blockuntu_<version>_$(dpkg --print-architecture).deb
sudo usermod -aG blockuntu "$USER"
```

Sign out and back in after changing group membership. The desktop user needs the
`blockuntu` group to access `/run/blockuntu/blockuntud.sock` through the GUI and
the browser Native Messaging host.

The package installs the daemon, GUI, Native Messaging host, systemd units,
default configuration, and browser-extension artifacts. It does **not** install
or enable a browser extension for you.

## First start

1. Open BlocKuntu after signing in.
2. Store the recovery uninstall phrase and Tier 1 edit key shown in the welcome
   modal somewhere secure. They are needed for protected actions.
3. Install and enable the BlocKuntu extension in every Firefox or Chrome browser
   you want BlocKuntu to protect, then restart that browser.
4. Use **Settings → Health** to verify the browser and Native Messaging checks.

The daemon defers managed browser-policy repair until it receives the first
extension heartbeat. A missing policy file directly after package installation is
therefore expected.

For Firefox Snap or Flatpak, opening BlocKuntu starts the per-user setup. If it
does not complete, run this as the desktop user and restart Firefox:

```bash
blockuntu-setup-confined-firefox
```

## Verify the installation

Check the service and socket after signing in:

```bash
systemctl status blockuntu.socket blockuntu.service blockuntu-watchdog.service
ls -l /run/blockuntu/blockuntud.sock
id
```

The socket should be owned by `root:blockuntu` with mode `srw-rw----`, and the
desktop user should be in the `blockuntu` group. Use **Settings → Health** for
the supported browser, policy, and hosts-file checks.

If browser navigation fails closed, check the path in this order:

```text
browser extension -> blockuntu-native -> /run/blockuntu/blockuntud.sock -> blockuntud
```

Common causes are stale group membership, a browser extension that was not
installed or restarted, and a missing Native Messaging manifest for a confined
Firefox installation.

## Build a package

From the repository root:

```bash
./scripts/package-deb.sh
```

The package is written to `target/debian`. Choose an explicit release version
when needed:

```bash
./scripts/package-deb.sh --version <version>
```

The build requires release binaries, the Tauri GUI, a signed Firefox XPI, and a
Chrome CRX. Inspect the resulting package before distribution:

```bash
dpkg-deb -I target/debian/blockuntu_<version>_$(dpkg --print-architecture).deb
dpkg-deb -c target/debian/blockuntu_<version>_$(dpkg --print-architecture).deb
```

## Manual developer installation

For a development or administrator-managed installation outside Debian
packaging, use the script from the repository root:

```bash
./scripts/install-production.sh
```

It installs to `/usr/local`, unlike the Debian package, which installs its
binaries under `/usr/bin`. Review available options before using it:

```bash
./scripts/install-production.sh --help
```

Use the matching script only for this installation type:

```bash
./scripts/uninstall-production.sh --help
```

Do not use that script to remove a Debian package installation; see
[UNINSTALL.md](UNINSTALL.md) instead.

## Security boundary

BlocKuntu is intended to resist routine user-level bypasses. A user with
unrestricted root or sudo access can still alter local services, policies, and
files. Use a desktop account without sudo access when that distinction matters.
