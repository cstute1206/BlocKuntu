# BlocKuntu Features

This document lists the features of BlocKuntu.

## Rules and tiers

Website and application lists can contain multiple entries and can be enabled or disabled independently with different blocking tiers.

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
- While a schedule or Detox is active, you can append new rows. You can only delete rows while a Detox is active. Tier 1 lists are active at all times, which is why you need the Tier 1 unlock key to delete rows from them.

## Browser and application enforcement

Install the extension for Firefox or Chrome:

Install the [Firefox extension](https://addons.mozilla.org/en-US/firefox/addon/blockuntu/).

Install the [Chrome extension](https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc).

For a list of supported browsers, see [Supported browsers](supportedBrowsers.md).

## Overview

- Display active rules.
- Test a URL.
- Manually unlock Tier 3 websites.

## Websites

- Create website lists.
- Edit website lists:
  - Name the website list.
  - Select a tier.
  - Attach it to a schedule.
  - Add a domain, exact URL, URL prefix, path prefix, or URL-contains pattern.
    - Select whether the pattern also applies to subdomains.
  - Add a new pattern while the list is active.
  - Delete rows while the list is not active.

## Applications

- Create application lists.
- Edit application lists:
  - Name the application list.
  - Select a tier.
  - Attach it to a schedule.
  - Add a command, binary, path, desktop ID, title-contains, or exact-title matcher.
    - Select matchers by searching active applications.
  - Add a new matcher while the list is active.
  - Delete rows while the list is not active.

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
  - Choose how Chromium private browsing is handled.
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
- Only some browsers are supported. See [Supported browsers](supportedBrowsers.md).
