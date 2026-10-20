# BlocKuntu Features

This document lists the features of BlocKuntu.

## Rules and tiers

Website and application lists can contain multiple entries with different blocking tiers. Tier 2 and Tier 3 lists can either block their entries or allow only their entries.

| Tier   | When it blocks                              | Access behavior                                                         |
| ------ | ------------------------------------------- | ----------------------------------------------------------------------- |
| Tier 1 | Always                                      | Strict. Editing an active list requires a temporary Tier 1 edit unlock. |
| Tier 2 | During an attached schedule or active Detox | Strict. No allowance or manual unlock.                                  |
| Tier 3 | During an attached schedule or active Detox | A daily allowance and a short manual unlock remain available.           |

## Schedules, allowances and Detox

- Reusable schedules support multiple windows, grouped days, individual weekdays and overnight windows.
- Schedule times can be entered in 24-hour or AM/PM format.
- Tier 2 and Tier 3 need to be attached to a schedule or Detox to be active.
- Tier 3 offers a daily allowance, before the block becomes active.
- Detox activates selected Tier 2 and Tier 3 lists for one minute through twelve weeks. Tier 2 remains strict, while Tier 3 retains its normal allowance and manual-unlock behavior.
- While a blocklist is active, you can append blocked rows but cannot remove them. While an allowlist is active, you can remove allowed rows but cannot add or edit them. Both are restriction-tightening changes. Tier 1 lists are active at all times while enabled, so broader edits require the Tier 1 unlock key. The same key authorizes broader edits to scheduled Tier 2 and Tier 3 website and application lists for five minutes, under the existing protected-access eligibility setting. Active schedule windows remain protected. Detox-attached lists remain limited to tightening changes even with the key; their tier, mode, enabled state, entries already blocked and allowance cannot be weakened or replaced. Direct attachment edits stay locked; deleting an idle schedule may clean up its attachment without ending Detox, which independently retains its selected list IDs.

## Browser and application enforcement

Native Firefox policy activation and repair preserve unrelated settings and other extensions. GUI-authorized uninstall restores the previous values of settings BlocKuntu changed and preserves unrelated additions made during use.

The default strict browser heartbeat grace is 60 seconds. Initial browser startup allows at least 60 seconds, and packaged manual setup waits for the first extension connection. After a daemon restart or sustained pause, previously connected browser sessions receive a bounded 60-second reconnect window. Without a fresh heartbeat, strict browser closure resumes; extension fail-closed behavior and application rules remain active.

The system daemon enforces shared policy independently of the GUI session.
Logging out or switching to another regular-user account does not stop the
daemon or clear active rules, Detox sessions, or usage state.

Install the extension for Firefox or Chrome:

Install the [Firefox extension](https://addons.mozilla.org/en-US/firefox/addon/blockuntu/).

Install the [Chrome extension](https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc).

For a list of supported browsers, see [Supported browsers](supportedBrowsers.md).

## Overview

- Display active rules.
- Test a URL.
- Manually unlock active Tier 3 websites and applications, including during Detox.
- **Manual Unlock targets:** use a domain or URL (`example.com`, `https://example.com/path`), an exact configured application matcher value (`editor`, `/usr/bin/editor`, `editor.desktop`), an application list display name (`Games`), or an application list ID (`app-rule-1`). Application list names ignore case and surrounding spaces. A display name selects all matching active Tier 3 application lists, with one reason, one hourly quota slot and the same two-minute expiry. Disabled/inactive lists are not unlocked, and Tier 1/Tier 2 restrictions continue to apply. Names take precedence if an input also happens to equal a list ID; otherwise a list ID selects one list. For allowlists, enter an excluded URL or application identity, or the application list name/ID. Website list names and IDs are not supported; enter a URL rejected by that list instead. List IDs are shown in each list editor.
- An unlock applies to the entire selected Tier 3 list, not only the entered target. Other active lists continue to apply. For overlapping website lists the existing resolver selects a blocking Tier 3 match, preferring the stricter list. An application list name selects all eligible lists with that name; a list ID otherwise selects its list.
- Existing rules remain: a new reason with at least 20 letters, one unlock globally per rolling hour, and a two-minute expiry without refilling the daily allowance. Tier 1 and Tier 2 remain strict.

## Websites

- Create website lists.
- Edit website lists:
  - Name the website list.
  - Choose **Block listed websites** or **Allow only listed websites**.
  - Select a tier.
  - Attach it to a schedule.
  - Add a domain, exact URL, URL prefix, path prefix, or URL-contains pattern.
    - Select whether the pattern also applies to subdomains.
  - Add a new pattern while a blocklist is active, or remove a pattern while an allowlist is active.
- Website allowlists support Tier 2 and Tier 3 and remain inactive until an attached schedule or selected Detox session is active. Tier 1 allowlists are not supported.
- Tier 2 allowlists block every non-matching website immediately while active. Tier 3 allowlists count time on non-matching websites against their daily allowance, then block those websites after exhaustion while retaining manual unlocks. Matching allowed websites do not consume the allowance.
- Saving a Tier 1 website blocklist or any website allowlist requires confirmation in a warning dialog.
- Entries within one allowlist are alternatives, while multiple active allowlists are intersected: a URL must be accepted by every active allowlist. A matching blocklist always wins.
- Website allowlists cover top-level HTTP and HTTPS navigation. Redirect and sign-in domains may need separate entries.

## Applications

- Create application lists.
- Edit application lists:
  - Name the application list.
  - Choose **Block listed applications** or **Allow only listed applications**.
  - Select a tier.
  - Attach it to a schedule.
  - Add a command, binary, path, desktop ID, title-contains, or exact-title matcher.
    - Select matchers by searching active regular-user processes.
  - Add a matcher while a blocklist is active, or remove an allowed identity while an allowlist is active.
- Application allowlists support Tier 2 and Tier 3 and remain inactive until an attached schedule or selected Detox session is active. Tier 1 allowlists are not supported.
- **Append** and **Add all detected** discard empty or whitespace-only draft matchers, retain populated entries and deduplicate incoming identities. Empty matchers are excluded from saves.
- Detected regular-user processes are collapsed by default. **Add all detected** adds one preferred identity for each currently detected process without expanding the list and deduplicates identical matchers.
- Tier 2 closes each non-allowlisted process independently while active. Tier 3 counts time while at least one non-allowlisted process is running, including in the background or while minimized, then closes rejected processes after the daily allowance is exhausted unless manually unlocked.
- Permission is not inherited by child, helper, renderer, or related processes. Each process must match the allowlist with its own identity.
- Active application allowlists evaluate regular-user processes across accounts against the same policy.
- BlocKuntu processes, their required descendants, and non-desktop system accounts remain exempt for recovery. Regular-user session infrastructure is evaluated normally.
- Saving an application allowlist requires confirmation in a warning dialog that lists the risks to D-Bus, portals, authentication, audio, clipboard, browser helpers, IDE language servers, and terminal children.

## Detox

- Start and name a Detox session.
- Choose a duration in minutes, hours, days, or weeks, up to 12 weeks.
- Select the website lists or application lists to attach to the Detox session.
- Once started, Detox cannot be cancelled, including after a protected edit unlock. It ends at its configured end time. The legacy cancellation RPC rejects cancellation.
- Attached lists cannot be deleted, disabled, switched to another tier or mode, or given a larger allowance during Detox. Blocklists can gain entries; allowlists can lose allowed entries. Normal Tier 3 allowance and manual-unlock rules remain unchanged.
- Protected uninstall is unavailable while any Detox is active, including the emergency-code path, to prevent indirect cancellation.
- List active Detox sessions.
- List recent Detox sessions.

## Schedule

- Create and name a schedule.
- Select individual weekdays, weekdays, every day, or weekends, and choose when the schedule is active.
  - One schedule can have multiple windows.
- Select the attached website lists and application lists. The edit key also authorizes attachment changes on scheduled lists, except Detox-attached lists.
- Deleting a website list, application list or idle schedule uses the same native confirmation dialog as changing protected settings access. The red **Delete** button opens a confirmation naming the item and explaining its consequences; canceling or dismissing it makes no changes. Deleting a list removes its schedule attachments and unused allowance, preserving schedules and other lists. Deleting a schedule removes its ID from all remaining lists, even if a previously attached list has already been deleted. Lists without another active schedule or Detox become inactive (Tier 1 stays active). Active schedules cannot be deleted. Active lists require the protected edit key for deletion; Detox-attached lists cannot be deleted.

## Statistics

- Display total recorded events.
- Display grouped events.
- Display total active time for schedules.

## Settings pages

- Health
  - Overview of the enforcement checks.
- Rules and logging
  - Import and export rules. Import appends to your existing rules.
  - Export logs.
- Protected changes and uninstall
  - Set when uninstallation and protected list editing are available.
  - Choose how Chromium private browsing is handled. Manual extension consent is the default; disabling private browsing is recommended for full allowlist protection.
  - Chromium private-browsing mode and disable-scope controls are locked while any website allowlist is active through a schedule or Detox.
  - Display the welcome modal.
  - Remove the uninstall phrase from the welcome modal.
  - Enter the Tier 1 edit key to edit website and application lists across all tiers for five minutes. Detox protection remains in force.
  - Enter the uninstall phrase.
- Notifications
  - Configure desktop notifications. Delivery runs while the GUI or tray process runs. Blocking events predating that process are persistently suppressed; blocking notifications expire after one minute, including delivery failures. Repeated block events stay deduplicated. Suppression changes only the notification outbox; event history remains in statistics and logs.
  - Enable **Start BlocKuntu in the system tray when I log in** under application preferences. This per-user setting creates `$XDG_CONFIG_HOME/autostart/blockuntu-gui.desktop` (default `~/.config/autostart/`) and launches with `--tray`. Disabling removes that entry. Manual launches open the window, including when a tray instance is already running; repeated login launches use the existing single instance.
  - The tray offers **Show BlocKuntu**, **Open Detox**, and **Open Settings**. If tray creation fails, the desktop has no registered tray display host, or `gdbus` is unavailable, startup opens the window instead. Losing the tray host opens the window within the next ten-second check. Login startup is supported on desktops implementing XDG Autostart; moving a portable binary requires enabling the setting again.
  - Choose a time format: 24-hour or AM/PM.
  - Display the build, installation serial, and update link.

## Boundaries and limitations

- BlocKuntu cannot protect against a user with unrestricted root or sudo access.
- Hosts-file fallback supports domain patterns only. Exact-URL and path patterns require the browser extension.
- Website allowlists always require browser-extension enforcement; the hosts file cannot express “block everything except these entries.”
- Website allowlists can be attached to schedules and started in Detox with every Chromium private-browsing setting. Regular windows remain protected, but full private-window allowlist protection requires either disabling private browsing or manually granting the extension access there.
- Native-browser acceptance on Ubuntu, Fedora and CachyOS verified the published Chrome/Chromium extension 0.2.6 with split incognito mode: its block page renders correctly after granting “Allow in Incognito” and restarting the browser. This fixes the 0.2.5 `ERR_BLOCKED_BY_CLIENT` failure. Without browser-owned consent, incognito remains unprotected; users can withdraw that consent. Disabled private browsing remains recommended for full allowlist protection. See the [validation evidence](../Testing/reports/phase-3-validation.md).
- Application allowlists evaluate regular-user processes individually. Essential session services and application helpers must be explicitly covered or they can be terminated.
- Only some browsers are supported. See [Supported browsers](supportedBrowsers.md).
