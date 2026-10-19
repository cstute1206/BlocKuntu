# Quality-of-life changes — source validation, 2026-10-19

This validates the current working tree, including pre-existing unrelated changes. It is not installed-package or desktop-login acceptance evidence.

## Behavior

- Appending detected application identities removes empty and whitespace-only draft rows, preserves populated rows and deduplicates incoming identities. Save normalization continues to exclude empty matchers.
- Active Detox has no Cancel action. The daemon rejects the legacy cancellation RPC even after an edit unlock. Protected uninstall is also refused during Detox; Detox cannot start during an uninstall handoff.
- The Tier 1 edit key authorizes five minutes of editing and deleting website/application lists across all three tiers, including active scheduled lists. Existing credential, eligibility and expiry rules remain. Schedule windows themselves remain protected.
- Detox-attached lists remain limited to tightening changes: append blocked identities/patterns or remove allowed ones. Their settings and allowances stay locked. Deleting an idle schedule can clean up its attachment without affecting the independent Detox selection or end time. Normal Tier 3 allowances and manual unlocks remain available.
- Manual Unlock supports website domains/URLs, application matcher values/rejected identities and application list IDs. It unlocks an entire matching Tier 3 list. Website list ID selection is a missing existing capability, not added here; display names are not list identifiers. Helpers, visible list IDs and allowlist documentation now explain this. Reason uniqueness/length, global hourly quota and two-minute expiry remain unchanged.
- Delete actions use the existing red destructive style and a shared confirmation modal naming the saved item. Cancel and Escape leave configuration unchanged. Schedule deletion removes its ID from both website and application lists, including after a former website attachment has been deleted. Other schedules and list attachments survive. Persistence failures are checked without changing the stored policy or in-memory configuration.
- Login startup is a per-user XDG Autostart entry launching the GUI with `--tray`. Normal manual launches show the main window through the existing single-instance plugin. Disabling removes the entry. Tray creation failure or an unconfirmed display host opens the GUI instead, and host disappearance reopens it on the next periodic check. Notifications run independently of tray creation.
- Blocking notifications predating GUI/tray process startup are persistently suppressed. One-minute expiry also applies to old outbox rows and failed deliveries. Suppressed rows retain deduplication information; block event history and aggregate statistics remain untouched. Lifecycle notification preferences and delivery retry behavior remain available.

The startup entry and escaping follow the [XDG Autostart specification](https://specifications.freedesktop.org/autostart/latest/) and [Desktop Entry Exec rules](https://specifications.freedesktop.org/desktop-entry/latest-single/). Display-host detection checks `IsStatusNotifierHostRegistered` on the [KDE watcher interface](https://github.com/KDE/plasma-workspace/blob/master/xembed-sni-proxy/org.kde.StatusNotifierWatcher.xml), rather than assuming successful icon creation means a visible tray exists.

## Checks run

| Check | Result |
| --- | --- |
| `cargo test --locked --manifest-path focus-core/Cargo.toml` | 72 tests passed (16 unit, 43 integration, 13 PR regression); doc tests passed |
| `cargo test --locked --manifest-path focusd/Cargo.toml` | 133 tests passed; binary/doc targets passed |
| `cargo test --locked --manifest-path focus-gui/src-tauri/Cargo.toml --lib` | 11 tests passed, including startup persistence/removal, Exec escaping and launch flags |
| `cargo clippy --locked --all-targets -- -D warnings` for core, daemon and Tauri manifests | Passed |
| `npm run check` in `focus-gui` | Zero errors and zero warnings |
| `npm test` in `focus-gui` | 16 component/helper tests passed, including Append, all list tiers, Detox locks, schedule attachment controls, deletion/Cancel and no Detox Cancel action |
| `npm run build` in `focus-gui` | Passed |
| `python3 Testing/scripts/check-pr-coverage.py` | All 57 existing Layer 1 cases reference automated tests |
| `git diff --check` | Passed |

Native compilation initially lacked GTK/WebKit development metadata. System installation required terminal authentication, so development packages were downloaded and extracted into `/tmp/blockuntu-gui-devdeps` without installing system packages. Native commands used:

```bash
PKG_CONFIG_PATH=/tmp/blockuntu-gui-devdeps/root/usr/lib/x86_64-linux-gnu/pkgconfig:/tmp/blockuntu-gui-devdeps/root/usr/share/pkgconfig \
LIBRARY_PATH=/tmp/blockuntu-gui-devdeps/root/usr/lib/x86_64-linux-gnu \
cargo test --locked --manifest-path focus-gui/src-tauri/Cargo.toml --lib
```

Clippy used the same environment. GUI tests ran outside the filesystem/network sandbox because its localhost resolution failed.

## Remaining runtime validation

Actual desktop login, visible tray menus, second-process activation, tray-host disappearance and real desktop notification banners have not been exercised in a graphical installed-package session here. Before release, check these on the target desktop, with startup enabled and disabled, and with a tray host absent. The fallback deliberately shows the GUI when `gdbus` or a registered display host cannot be confirmed. Portable binary moves require enabling startup again to update its executable path. No full VM matrix was run.
