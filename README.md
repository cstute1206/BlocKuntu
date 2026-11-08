# BlocKuntu

BlocKuntu is a local website and application focus blocker for Linux. Packages are available for Debian/Ubuntu, Fedora, and Arch-based systems on x86-64. A system daemon enforces shared rules, while the [Firefox](https://addons.mozilla.org/en-US/firefox/addon/blockuntu/) and [Chrome](https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc) extensions enforce browser navigation rules.

BlocKuntu resists casual bypasses, but cannot protect against a user with unrestricted `sudo` or root access.

https://github.com/user-attachments/assets/4178a567-3bb1-4f56-9f0b-6fb59eb27bc4

## What it does

- Blocks websites and applications through reusable blocklists and Tier 2 or Tier 3 allowlists.
- Activates rules through schedules or time-limited Detox sessions. Detox cannot be cancelled once started.
- Offers three policy tiers:
  - **Tier 1:** always blocked while enabled. Editing an active list requires a Tier 1 edit unlock.
  - **Tier 2:** strictly blocked during an attached schedule or Detox, with no allowance or manual unlock.
  - **Tier 3:** blocked during an attached schedule or Detox, with a configurable daily allowance and one two-minute manual unlock per hour.
- Provides protected list editing and uninstall, import/export, desktop notifications, health checks, and optional login startup in the system tray.

The Tier 1 edit key authorizes five minutes of editing for website and application lists across all tiers, including scheduled lists. Active Detox lists can only be made more restrictive; the key cannot end or bypass Detox. Tier 3 retains its normal allowance and manual unlock.

See [Features](Docs/FEATURES.md) for detailed behavior and [Supported browsers](Docs/supportedBrowsers.md) for compatible browser packages and setup requirements.

## Packages and validation

BlocKuntu 0.2.0 is an alpha release for fresh installations.

| Package family | Download | Validated desktop environment |
| --- | --- | --- |
| Debian / Ubuntu | Shared `.deb` for `amd64` | Ubuntu 26.04.1 LTS |
| Fedora | `.rpm` for `x86_64` | Fedora 44 Workstation |
| Arch-based systems | `.pkg.tar.zst` for `x86_64` | CachyOS, rolling installation at test time |

The DEB is built on Debian 13 and the RPM on Fedora 43. Debian has no separate desktop acceptance run in the current test gate. Other distribution versions and derivatives are not individually validated by these results. Ubuntu uses the same Debian-built DEB; there is no separate Ubuntu download.

## Install

**Upgrades remain deferred.** These instructions are for fresh installations. In-place upgrades from an earlier BlocKuntu release, including preservation of policy and history across an upgrade, are not validated for this release.

Download the package for your system and `SHA256SUMS` from [Releases](https://github.com/cstute1206/BlocKuntu/releases). Verify the downloaded package against its entry in `SHA256SUMS`, then use your system's package manager.

### Debian / Ubuntu

```bash
sudo apt install ./blockuntu_0.2.0-1_amd64.deb
```

### Fedora

```bash
sudo dnf install ./blockuntu-0.2.0-1.fc43.x86_64.rpm
```

### Arch / CachyOS

```bash
sudo pacman -U ./blockuntu-0.2.0-1-x86_64.pkg.tar.zst
```

### Complete setup

Add your desktop account to the `blockuntu` group:

```bash
sudo usermod -aG blockuntu "$USER"
```

Sign out and back in, or restart the system. Open BlocKuntu and store the recovery uninstall phrase and Tier 1 edit key shown in the welcome modal.

Install the [Firefox extension](https://addons.mozilla.org/en-US/firefox/addon/blockuntu/) or [Chrome extension](https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc), then follow the [browser setup requirements](Docs/supportedBrowsers.md).

## Uninstall

Use **Settings → Protected changes and uninstall**. Enter the recovery uninstall phrase, then select **Uninstall BlocKuntu** when protected actions are available. Active Detox prevents uninstall. Direct package-manager removal is protected by this flow.

## Build a package

| Target | Helper |
| --- | --- |
| Debian / Ubuntu | `./scripts/package-deb.sh` |
| Fedora | `./scripts/package-rpm.sh` |
| Arch / CachyOS | `./scripts/package-arch-docker.sh` |

The RPM helper requires a suitable Fedora build environment; the Arch Docker helper requires Docker. See [Package builds and validation](Testing/phase-2.md) for dependencies, output paths, build environments, and deferred upgrade coverage.

## Testing

The repository includes source tests, package validation, installed-package smoke tests, and selected VM acceptance scenarios. See [Testing](Testing/README.md) for their scope and how to run them.

## License

BlocKuntu is licensed under the [MIT License](LICENSE).
