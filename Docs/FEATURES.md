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
- While a blocklist is active, you can append blocked rows but cannot remove them. While an allowlist is active, you can remove allowed rows but cannot add or edit them. Both are restriction-tightening changes. Tier 1 lists are active at all times while enabled, so broader edits require the Tier 1 unlock key.

## Browser and application enforcement

Install the extension for Firefox or Chrome:

Install the [Firefox extension](https://addons.mozilla.org/en-US/firefox/addon/blockuntu/).

Install the [Chrome extension](https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc).

For a list of supported browsers, see [Supported browsers](supportedBrowsers.md).

## Overview

- Display active rules.
- Test a URL.
- Manually unlock Tier 3 websites and applications.

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
- Detected regular-user processes are collapsed by default. **Add all detected** adds one preferred identity for each currently detected process without expanding the list and deduplicates identical matchers.
- Tier 2 closes each non-allowlisted process independently while active. Tier 3 counts time while at least one non-allowlisted process is running, including in the background or while minimized, then closes rejected processes after the daily allowance is exhausted unless manually unlocked.
- Permission is not inherited by child, helper, renderer, or related processes. Each process must match the allowlist with its own identity.
- BlocKuntu processes, their required descendants, and non-desktop system accounts remain exempt for recovery. Regular-user session infrastructure is evaluated normally.
- Saving an application allowlist requires confirmation in a warning dialog that lists the risks to D-Bus, portals, authentication, audio, clipboard, browser helpers, IDE language servers, and terminal children.

## Detox

- Start and name a Detox session.
- Choose a duration in minutes, hours, days, or weeks, up to 12 weeks.
- Select the website lists or application lists to attach to the Detox session.
- List active Detox sessions.
- List recent Detox sessions.

## Schedule

- Create and name a schedule.
- Select individual weekdays, weekdays, every day, or weekends, and choose when the schedule is active.
  - One schedule can have multiple windows.
- Select the attached website lists and application lists.

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
  - Set when uninstallation and Tier 1 editing are available.
  - Choose how Chromium private browsing is handled. Manual extension consent is the default; disabling private browsing is recommended for full allowlist protection.
  - Chromium private-browsing mode and disable-scope controls are locked while any website allowlist is active through a schedule or Detox.
  - Display the welcome modal.
  - Remove the uninstall phrase from the welcome modal.
  - Enter the Tier 1 edit key to edit Tier 1 rules for five minutes.
  - Enter the uninstall phrase.
- Notifications
  - Configure desktop notifications.
  - Choose a time format: 24-hour or AM/PM.
  - Display the build, installation serial, and update link.

## Boundaries and limitations

- BlocKuntu cannot protect against a user with unrestricted root or sudo access.
- Hosts-file fallback supports domain patterns only. Exact-URL and path patterns require the browser extension.
- Website allowlists always require browser-extension enforcement; the hosts file cannot express “block everything except these entries.”
- Website allowlists can be attached to schedules and started in Detox with every Chromium private-browsing setting. Regular windows remain protected, but full private-window allowlist protection requires either disabling private browsing or manually granting the extension access there.
- Application allowlists evaluate regular-user processes individually. Essential session services and application helpers must be explicitly covered or they can be terminated.
- Only some browsers are supported. See [Supported browsers](supportedBrowsers.md).
