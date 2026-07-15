# Settings Implementation

This document defines the Settings implementation for BlocKuntu. It replaces
the current Admin page with a broader control center while keeping privileged
actions protected. Changes to uninstall behavior are out of scope.

## Scope

- Rename the visible `Admin` navigation and tray entry to `Settings`.
- Keep the existing route and internal identifiers unchanged for now.
- Retain the existing Health, Tier 1 edit, policy transfer, and uninstall
  behavior while reorganizing their presentation.
- Write daemon events to a plain local log file at `/etc/blockuntu/blockuntu.log`.
- Do not add a GUI log viewer, filters, or log-export workflow.
- Base Statistics on a daemon-parsed summary of that log file rather than the
  database event feed.
- Keep settings that weaken enforcement behind the existing protected-change
  rules.

## Current Implementation Status

The Settings modal and plain event log are implemented. Settings uses the
existing daemon health, enforcement, policy-transfer, and protected-edit APIs;
it does not invent configuration that the daemon cannot persist or enforce.

| Area | Implemented now | Still required for the full design |
| --- | --- | --- |
| Naming and navigation | Visible `Admin` wording is `Settings` in the sidebar and tray menu. It is aligned at the bottom of the main sidebar and opens a fixed-size modal control centre with a sidebar of settings sections; the internal `admin` route and tray event remain unchanged. | Nothing for this scope. |
| Health | Existing live health rows, technical detail, last health-check time, refresh, and copy-diagnostics action. | A dedicated structured last-successful-heartbeat value only if the daemon needs to expose more than the current health detail. |
| Enforcement | Read-only live state for enforcement, Firefox and Chrome policy compliance, hosts immutability, unsupported-browser rule, and X11 window-title availability. | Daemon-backed settings for heartbeat grace period, stale-browser handling, scan interval, and any other enforcement behaviour. These must preserve the existing protected-change rules. |
| Browser Integration | Existing Firefox/Chrome/Snap/Flatpak/Native Messaging health checks, extension IDs, managed paths supplied by the checks, and the confined-Firefox repair command. | A reviewed GUI action to run a repair helper, plus any additional daemon API needed for richer per-browser state. |
| Policy and Recovery | TOML export and append/import, including the current result message. | Policy database and snapshot paths, recovery-snapshot creation, recovery-snapshot restore, and persistent transfer/snapshot history. |
| Protected Changes | Sunday operator-window state, five-minute Tier 1 edit unlock, current unlock status, and expiry. | Nothing in this Settings scope. |
| Application UI | Persistent local preferences for restoring the last selected page and the GUI refresh interval, plus restoring the first-run overview. | Start-on-login, configurable tray-close behaviour, and desktop/tray notifications through reviewed native-runtime support. |
| Logging and statistics | The daemon appends each recorded event to `/etc/blockuntu/blockuntu.log`; Settings shows the path and terminal commands to inspect it, while Statistics gets its total and event-kind counts by parsing that file. | Nothing for this simplified scope. |
| Maintenance | Reset first-run state and the existing restricted uninstall workflow. | Nothing for this simplified scope. |

The event log is intentionally plain and append-only. It is primarily a
terminal-facing diagnostic file: use `sudo tail -f /etc/blockuntu/blockuntu.log`
or `sudo less /etc/blockuntu/blockuntu.log`. Statistics only presents the
daemon-parsed total and event-kind counts; it does not show individual entries.

## Page Layout

Settings is a single control-center page with these sections, in order:

1. Health
2. Enforcement
3. Browser Integration
4. Policy And Recovery
5. Protected Changes
6. Application UI
7. Logging
8. Maintenance

Settings opens as a fixed-size modal control centre over the current page. Its
own sidebar selects one section at a time, so the user does not need to scroll
through a single long settings page. Each selected section can scroll
independently when needed, without changing the modal's dimensions.

## Health

Health remains the first Settings section and extends the current health
overview.

Show:

- Daemon and socket status
- Browser extension and Native Messaging status
- Firefox and Chrome/Chromium managed-policy status
- Firefox Snap and Flatpak integration status
- Hosts-file enforcement status
- Last successful browser heartbeat
- Last health-check timestamp

Actions:

- Refresh health
- Copy a diagnostics summary
- Show the technical detail supplied for each health row

## Enforcement

This section exposes enforcement behavior and its availability. Settings that
would weaken the blocker stay read-only until the existing protected-change
rules allow the change.

Show or configure when supported by the daemon:

- Firefox extension requirement
- Chrome/Chromium extension requirement
- Browser handling when an extension heartbeat is stale
- Unsupported-browser blocking behavior
- Browser heartbeat grace period
- Hosts-file immutability requirement
- Application scan interval
- X11 window-title matching availability
- Unsupported-session behavior on Wayland or other non-X11 sessions

Every unavailable capability must state why it is unavailable rather than
silently hiding the setting.

## Browser Integration

Browser Integration is repair-oriented. It shows the current state and offers
the next supported action.

Show:

- Firefox, Chrome/Chromium, Firefox Snap, and Firefox Flatpak integration state
- Native Messaging manifest paths
- Installed extension IDs
- Managed-policy paths
- Last heartbeat for each supported browser
- Whether required managed setup is active or deferred

Actions:

- Recheck browser integration
- Show the supported repair command
- Run a supported repair helper from the GUI when that helper has an explicit,
  reviewed GUI path

For confined Firefox, user-facing repair guidance must explicitly say to open a
terminal and run `blockuntu-setup-confined-firefox`.

## Policy And Recovery

This section replaces the current Policy Files panel.

Show:

- Policy database path
- Policy recovery-snapshot path
- Most recent recovery-snapshot status
- Most recent policy import or export result

Actions:

- Export policy as TOML
- Append policy from TOML
- Create a recovery snapshot
- Restore a recovery snapshot

Export is lower risk. Import, snapshot restore, and any action that changes an
active policy must use the existing protected-change rules.

## Protected Changes

This section retains the existing Tier 1 edit-unlock behavior.

Show:

- Operator-window state and label
- Protected-edit unlock state
- Unlock expiration time

Action:

- Unlock protected edits for five minutes using the existing Tier 1 edit key

This section is the only place that exposes the edit-unlock workflow.

## Application UI

Application UI contains preferences that do not weaken enforcement.

Settings:

- Start the GUI on login
- Minimize to tray on close
- Restore the last selected page on startup
- Dashboard refresh interval
- Show the first-run overview again
- Desktop or tray notifications for blocked websites, blocked applications,
  Detox state, degraded browser integration, and health-check failures

## Logging

Settings only shows where to find the local log. The daemon appends every
recorded event to `/etc/blockuntu/blockuntu.log` in a single plain-text format.

Use a terminal to inspect it:

```bash
sudo tail -f /etc/blockuntu/blockuntu.log
sudo less /etc/blockuntu/blockuntu.log
```

No GUI viewer, filters, export, privacy mode, or log-redaction mode is part of
this implementation.

## Maintenance

Maintenance is the final Settings section for disruptive actions.

Actions:

- Reset first-run/onboarding state
- Uninstall BlocKuntu

Uninstall keeps its current confirmation-phrase and Sunday 20:00-23:59
operator-window restrictions. This Settings work does not change uninstall
credentials, package-purge behavior, or browser-uninstall handoff.

## Event Log File

There is no `Logs` main-navigation page. The plain daemon event log is kept at
`/etc/blockuntu/blockuntu.log` so it can be inspected with normal terminal
tools without turning the GUI into a log-analysis application. Statistics uses
the daemon's parsed total and event-kind counts from this same file.

## Implementation Order

Completed:

1. Change visible `Admin` wording to `Settings` in navigation and tray UI.
2. Move Settings into a modal with a section sidebar, while retaining the
   existing Health, Protected Changes, Policy And Recovery, and Maintenance
   behavior.
3. Add Browser Integration and Enforcement sections backed by existing health
   and enforcement data, with explicit unavailable states.
4. Add the supported Application UI preferences with persistent local storage.
5. Add the daemon-owned plain event log at `/etc/blockuntu/blockuntu.log` and
   show its location in Settings.
6. Keep the Settings modal dimensions stable across sections, place the
   Settings launcher at the bottom of the main sidebar, and derive Statistics
   from the plain event log.

Next:

1. Add daemon-owned policy recovery snapshots, including creation, restore,
   paths, status, and protected-change checks for policy-changing actions.
2. Add reviewed GUI entry points for supported browser-repair helpers.
3. Add reviewed native-runtime support for login startup, tray-close behaviour,
   and notifications.

Each step should preserve the daemon as the authority for policy and protected
actions. The GUI presents status and requests actions; it must not duplicate or
weaken backend enforcement checks.
