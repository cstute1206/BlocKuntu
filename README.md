# BlocKuntu

BlocKuntu is a local focus blocker for Debian and Ubuntu; support for other distributions is still in progress. A privileged daemon enforces website and application rules, while [Firefox](https://addons.mozilla.org/en-US/firefox/addon/blockuntu/) and [Chrome](https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc) extensions enforce website rules. BlocKuntu resists casual bypasses, but cannot protect against a user with unrestricted `sudo` or root access.

## What it does

- Blocks websites and applications through reusable lists.
- Supports schedules and time-limited Detox sessions.
- Offers three policy tiers:
  - **Tier 1:** always blocked while enabled. Editing an active list requires a Tier 1 edit unlock.
  - **Tier 2:** strictly blocked only while an attached schedule or Detox is active. There is no short unlock.
  - **Tier 3:** blocked only while an attached schedule or Detox is active. It has a configurable daily allowance and one two-minute manual unlock per hour.
- Uses a browser extension plus Native Messaging for website enforcement and process matching for application enforcement.
- Provides protected Tier 1 changes, protected uninstall, policy import/export, desktop notifications, and health checks.

For detailed behavior, see [Features](Docs/FEATURES.md).

## Install

Download the latest `.deb` package from the [latest BlocKuntu release](https://github.com/cstute1206/BlocKuntu/releases/latest), then run:

```bash
sudo apt install ./blockuntu_0.1.1_amd64.deb
sudo usermod -aG blockuntu "$USER"
```

Sign out and back in, or restart the system. Open BlocKuntu and store the  
recovery uninstall phrase and Tier 1 edit key shown in the welcome modal.

Then install the extension for your browser:

[Chrome](https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc)

[Firefox](https://addons.mozilla.org/en-US/firefox/addon/blockuntu/)

See [Supported browsers](Docs/supportedBrowsers.md) for compatible browser packages and their setup requirements.

## Uninstall

Use **Settings → Protected changes and uninstall**. Enter the recovery uninstall phrase from the welcome modal, then select **Uninstall BlocKuntu**. Direct package removal is protected by this flow.

## Build a package

Clone this repository, then run:

```bash
./scripts/package-deb.sh
```

The package is written to `target/debian/`. Test that exact `.deb` in a clean Debian or Ubuntu virtual machine before distributing it.

## License

BlocKuntu is licensed under the [MIT License](LICENSE).
