# BlocKuntu Installation

BlocKuntu currently supports Debian and Ubuntu package installations. The Debian
package is the only supported installation and test path.

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
3. Install and enable the BlocKuntu Firefox extension from AMO and the
  BlocKuntu Chrome extension from the Chrome Web Store in every browser you
  want BlocKuntu to protect, then restart each browser.
4. Use **Settings → Health** to verify the browser and Native Messaging checks.

Both browser policies are deferred until the matching extension sends its first
verified heartbeat. BlocKuntu then writes a policy that force-installs and locks
that same AMO or Chrome Web Store extension.

For Firefox Snap or Flatpak, opening BlocKuntu starts the per-user Native
Messaging setup. Firefox Flatpak receives its store-extension policy after the
first verified Firefox heartbeat. If setup does not complete, run this as the
desktop user and restart Firefox:

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

The build requires the release binaries and the Tauri GUI. Browser extensions
are retrieved from AMO and the Chrome Web Store; neither an XPI nor a CRX is
bundled. Inspect the resulting package before distribution:

```bash
dpkg-deb -I target/debian/blockuntu_<version>_$(dpkg --print-architecture).deb
dpkg-deb -c target/debian/blockuntu_<version>_$(dpkg --print-architecture).deb
```

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

## Security boundary

BlocKuntu is intended to resist routine user-level bypasses. A user with  
unrestricted root or sudo access can still alter local services, policies, and  
files. Use a desktop account without sudo access when that distinction matters.
