> [!CAUTION]
> This is still very Work in Progress. Use at your own risk!

# BlocKuntu

BlocKuntu is a local focus blocker for Debian and Ubuntu. A privileged daemon  
enforces website and application rules, while Firefox, LibreWolf, Waterfox, Chrome, Chromium, Brave,
Opera, Microsoft Edge, and Vivaldi extensions check browser navigation through the same policy engine. A Fedora
RPM build path is available for clean-VM validation; it is not yet a validated
release path.

> [!IMPORTANT]
> BlocKuntu makes casual bypasses harder; it is not a security boundary against  
> a user with unrestricted `sudo` or root access. Use it from a desktop account  
> without sudo access when that distinction matters.

## What it does

- Blocks websites and applications through reusable lists.
- Supports schedules and time-limited Detox sessions.
- Offers three policy tiers:
  - **Tier 1:** always blocked while enabled.
  - **Tier 2:** strictly blocked only while an attached schedule or Detox is  
  active; it cannot be manually unlocked.
  - **Tier 3:** active only during a schedule or Detox, with an optional daily  
  allowance and a short manual unlock.
- Uses a browser extension plus Native Messaging for website enforcement, and  
process matching for application enforcement.
- Provides protected Tier 1 changes, protected uninstall, policy import/export,  
desktop notifications, and health checks.

## Install

Install the supplied Debian package with `apt`, then add the desktop user to the  
`blockuntu` group and sign out/in:

```bash
sudo apt install ./blockuntu_<version>_$(dpkg --print-architecture).deb
sudo usermod -aG blockuntu "$USER"
```

Open BlocKuntu after signing in, keep the recovery credentials displayed in the
welcome modal somewhere safe, and install the Firefox extension from AMO in Firefox, LibreWolf, or Waterfox,
or the Chrome Web Store extension in Chrome, Chromium, Brave, Opera, Microsoft Edge, or Vivaldi.
In Opera and Edge, first turn on **Allow extensions from other stores**; in Vivaldi, enable Web Store
under **Settings → Privacy and Security → Google Extensions**. After each first
verified extension heartbeat, BlocKuntu locks that same store-installed
extension through browser policy when that browser installation supports
managed policy. Firefox Snap and Flatpak users can run
`blockuntu-setup-confined-firefox`, while Chromium, Brave, Opera, and Vivaldi
Snap users can run `blockuntu-setup-confined-chromium`, as their desktop user
if automatic setup does not complete.

> **Opera and Vivaldi Snap limitation:** Their current strict Snap packages can
> use BlocKuntu's per-user Native Messaging bridge, but cannot use its managed
> browser policies. The Snap sandboxes do not expose their policy directories
> to host-installed policy files, so BlocKuntu cannot force-install or lock the
> extension, disable private browsing, or apply the private URL-blocklist policy
> in those two Snap browsers. See the
> [installation limitation note](Docs/INSTALLATION.md#opera-and-vivaldi-snap-policy-limitation)
> for the diagnostic and the required upstream fix.

See [production installation](Docs/INSTALLATION.md) for verification, package  
building, confined browsers, and virtual-machine testing.

## Documentation

- [Installation](Docs/INSTALLATION.md) — supported package installation,  
verification, updates, and virtual-machine testing.
- [Uninstall](Docs/UNINSTALL.md) — protected package removal.
- [Features](Docs/FEATURES.md) — current user-visible behavior and policy model.
- [Hardening](Docs/HARDENING.md) — enforcement boundaries, implemented  
hardening, and known limitations.

Historical proof-of-concept and design material lives in the local ignored  
`Archive/` directory and is not current operating guidance.

## Build and test in a virtual machine

Build the Debian package from the repository root:

```bash
./scripts/package-deb.sh
```

Test the resulting package in a clean Debian or Ubuntu virtual machine. Copy the
`.deb` from `target/debian/` into that VM, install it with `apt`, and follow the
[installation guide](Docs/INSTALLATION.md). Use this package installation as the
only test path.

For a Fedora candidate, build `./scripts/package-rpm.sh`, then test the RPM from
`target/rpm/` in a clean Fedora Workstation VM with SELinux enforcing. The
[installation guide](Docs/INSTALLATION.md#fedora-rpm-acceptance) contains the
required acceptance checks.
