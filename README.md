# BlocKuntu

BlocKuntu is a local focus blocker for Debian and Ubuntu. A privileged daemon
enforces website and application rules, while Firefox and Chrome browser
extensions check browser navigation through the same policy engine.

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
welcome modal somewhere safe, and install the matching Firefox and/or Chrome
extension. Firefox Snap and Flatpak users can run
`blockuntu-setup-confined-firefox` as their desktop user if automatic setup does
not complete.

See [production installation](Docs/INSTALLATION.md) for verification, package
building, confined Firefox, and manual developer installation.

## Documentation

- [Installation](Docs/INSTALLATION.md) — supported package installation,
  verification, updates, and developer/manual installation.
- [Uninstall](Docs/UNINSTALL.md) — protected package removal and manual-install
  cleanup.
- [Features](Docs/FEATURES.md) — current user-visible behavior and policy model.
- [Hardening](Docs/HARDENING.md) — enforcement boundaries, implemented
  hardening, and known limitations.

Historical proof-of-concept and design material lives in the local ignored
`Archive/` directory and is not current operating guidance.

## Development

From the repository root, start the non-root development daemon and install the
development Native Messaging manifests:

```bash
./scripts/start-dev-daemon.sh
./scripts/install-dev-native-host.sh
```

Run the desktop UI separately:

```bash
cd focus-gui
npm install
npm run tauri dev
```

The development daemon uses `/tmp/blockuntu/blockuntud.sock`; the GUI checks the
production socket first and then falls back to that path.

## Verification

```bash
./scripts/verify-focus-core.sh
./scripts/verify-focusd.sh
./scripts/verify-focus-gui.sh
./scripts/verify-native-host.sh
./scripts/verify-firefox-extension.sh
./scripts/verify-chrome-extension.sh
```
