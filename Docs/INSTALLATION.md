# BlocKuntu Installation

BlocKuntu supports Debian and Ubuntu package installations. A self-hosted Fedora
RPM build path is available for clean-VM validation; it is not a supported
release path until its Fedora acceptance checks have passed.

## Install the Debian package

Install the supplied package with `apt`:

```bash
sudo apt install ./blockuntu_<version>_$(dpkg --print-architecture).deb
sudo usermod -aG blockuntu "$USER"
```

Sign out and back in after changing group membership. The desktop user needs the  
`blockuntu` group to access `/run/blockuntu/blockuntud.sock` through the GUI and  
the browser Native Messaging host.

The package installs the daemon, GUI, Native Messaging host, systemd units, and
default configuration. It does **not** bundle, install, or enable browser
extensions for you.

## First start

1. Open BlocKuntu after signing in.
2. Store the recovery uninstall phrase and Tier 1 edit key shown in the welcome
  modal somewhere secure. They are needed for protected actions.
3. Install and enable the BlocKuntu Firefox extension from AMO in Firefox,
  LibreWolf, or Waterfox, or the BlocKuntu Chrome Web Store extension in every
  supported Chromium-family browser you want BlocKuntu to protect: Chrome,
  Chromium Snap or native Chromium, native Brave, native Opera, Microsoft
  Edge, or native Vivaldi. In Opera and Edge, first turn on
  **Allow extensions from other stores**. In Vivaldi, enable Web Store under
  **Settings → Privacy and Security → Google Extensions**. Then restart each browser.
4. Use **Settings → Health** to verify the browser and Native Messaging checks.

Browser policies are deferred independently until their matching extension
sends its first verified heartbeat. BlocKuntu then writes a policy that
force-installs and locks that same AMO or Chrome Web Store extension when the
browser installation supports managed policy. Chromium Snap receives a
per-user Native Messaging host inside its Snap-accessible profile when
BlocKuntu starts. That host reaches the daemon through BlocKuntu's
authenticated loopback bridge because strict Snaps cannot access the system
Unix socket. Restart Chromium Snap after opening BlocKuntu and again after its
first heartbeat.

### Unsupported browser packages

Chromium Flatpak and Brave, Opera, and Vivaldi installed as strict Snaps are
currently **unsupported**. Whenever the automatic Tier 1 blocked-browser list
is active, BlocKuntu terminates these package variants. Chromium Snap remains a
separate, supported installation and is not included in this block.

These browsers cannot load BlocKuntu's managed browser policy. A verified
heartbeat is not sufficient: without policy, the extension can be removed and
private-browsing protection can be changed. This is a package-layout limitation,
not a malformed BlocKuntu policy. Validate any future publisher change in a
clean VM before moving a package variant into the supported set.

Choose the Chromium private-browsing behavior in **Settings → Protected changes
and uninstall**. “Allow with manual extension consent” is user-controlled and
can be turned off in the browser. “Disable private browsing” applies all the
time by default, or only during an active schedule or Detox. “Block URLs by
browser policy” includes active Hard, Scheduled Block, and Controlled Access
domain, exact-URL, and full URL-prefix rules; Controlled Access rules are still
blocked while an allowance remains. URL-contains and path-only rules are shown
as omitted because the browser policy cannot safely express them. A separate
change window protects these settings: all the time (default), only while no
schedule or Detox is active, or Sunday from 20:00 through 23:59. The private
URL-blocklist requires a supporting browser version and refuses lists over
1,000 patterns. Test this mode in the target clean VM.

For Firefox Snap or Flatpak, opening BlocKuntu starts the per-user Native
Messaging setup. Firefox Flatpak receives its store-extension policy after the
first verified Firefox heartbeat. If setup does not complete, run this as the
desktop user and restart Firefox:

```bash
blockuntu-setup-confined-firefox
```

For Chromium installed as a Snap, BlocKuntu performs the per-user Native
Messaging setup automatically when the GUI starts. If Chromium Snap was updated
while BlocKuntu was not running or its Health check still reports a missing
integration, run this as the desktop user and restart Chromium. This does not
enable the blocked browser package variants above:

```bash
blockuntu-setup-confined-chromium
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
browser installation. Vivaldi Flatpak and Firefox-family packages installed
outside their native package locations require separate clean-VM validation
before they can be treated as supported package paths.

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

The build requires the release binaries and the Tauri GUI. Browser extensions
are retrieved from AMO and the Chrome Web Store; neither an XPI nor a CRX is
bundled. Inspect the resulting package before distribution:

```bash
dpkg-deb -I target/debian/blockuntu_<version>_$(dpkg --print-architecture).deb
dpkg-deb -c target/debian/blockuntu_<version>_$(dpkg --print-architecture).deb
```

## Build a Fedora RPM candidate

Build the self-hosted RPM on Fedora, preferably in an isolated build
environment or VM:

```bash
./scripts/package-rpm.sh
```

The RPM is written to `target/rpm`. Inspect it before distribution:

```bash
rpm -qpi target/rpm/blockuntu-<version>-<release>.<arch>.rpm
rpm -qpl target/rpm/blockuntu-<version>-<release>.<arch>.rpm
rpm -qpR target/rpm/blockuntu-<version>-<release>.<arch>.rpm
rpm -Vvp target/rpm/blockuntu-<version>-<release>.<arch>.rpm
```

The build compiles the Tauri GUI with embedded frontend assets. It is a
self-hosted release workflow, not a Fedora repository submission workflow:
Fedora repository builds need offline/vendored Rust and npm dependencies.

### Build on Ubuntu for a Fedora VM test

Ubuntu can create the RPM artifact, but cannot satisfy the spec's Fedora RPM
`BuildRequires` through its Debian package database. This is suitable only for
producing a candidate to install and test in a clean Fedora VM; it does not
make BlocKuntu an Ubuntu RPM target or replace Fedora acceptance testing.

Install the Ubuntu-native toolchain and libraries (Rust and Node.js must also
be available in `PATH`):

```bash
sudo apt update
sudo apt install -y rpm build-essential libayatana-appindicator3-dev \
  libwebkit2gtk-4.1-dev libxdo-dev librsvg2-dev libssl-dev libudev-dev \
  pkg-config
```

Then explicitly bypass only RPM-database dependency verification:

```bash
./scripts/package-rpm.sh --ignore-buildrequires
```

The output is still a Fedora-targeted RPM. Do not install it on Ubuntu; copy
it out of `target/rpm/` and install it only in the clean Fedora test VM.

## Test a package in a virtual machine

Test each build in a clean Debian or Ubuntu virtual machine. Copy the `.deb` from
`target/debian/` into the VM, then install it there:

```bash
sudo apt install ./blockuntu_<version>_$(dpkg --print-architecture).deb
sudo usermod -aG blockuntu "$USER"
```

Sign out and back in, then complete the checks in
[Verify the installation](#verify-the-installation). Use this package
installation as the only test path.

### Fedora RPM acceptance

Test each Fedora RPM candidate in a clean Fedora Workstation VM with SELinux
enforcing. Copy the artifact from `target/rpm/` into the VM, inspect it, then
install it with `dnf`:

```bash
rpm -qpi ./blockuntu-<version>-<release>.<arch>.rpm
sudo dnf install ./blockuntu-<version>-<release>.<arch>.rpm
sudo usermod -aG blockuntu "$USER"
```

Sign out and back in. Then complete the normal service, socket, browser
heartbeat, and blocking checks, plus these Fedora-specific checks:

```bash
getenforce
sudo ausearch -m AVC,USER_AVC -ts recent
```

Keep SELinux enforcing. Check for denials after hosts-file enforcement,
policy-recovery writes, browser-policy repair, reboot, upgrade, rejected direct
removal, and the authorized GUI uninstall. See the
[Fedora RPM roadmap](FEDORA_RPM_ROADMAP.md) for the full acceptance sequence.

## Security boundary

BlocKuntu is intended to resist routine user-level bypasses. A user with  
unrestricted root or sudo access can still alter local services, policies, and  
files. Use a desktop account without sudo access when that distinction matters.
