# BlocKuntu Features

This is a concise description of the behavior available in the current  
application. It is not a substitute for the installation and uninstall guides.

## Rules and tiers

Website and application lists can contain multiple entries and can be enabled or  
disabled independently.

| Tier   | When it blocks                              | Access behavior                                                         |
| ------ | ------------------------------------------- | ----------------------------------------------------------------------- |
| Tier 1 | Always                                      | Strict. Editing an active list requires a temporary Tier 1 edit unlock. |
| Tier 2 | During an attached schedule or active Detox | Strict. No allowance or manual unlock.                                  |
| Tier 3 | During an attached schedule or active Detox | A daily allowance and a short manual unlock remain available.           |

Tier 2 and Tier 3 lists therefore need a schedule or Detox to become active.  
Tier 1 domain rules and active Tier 2 domain rules are also represented in the  
managed hosts-file fallback. Tier 3 is intentionally browser-enforced so its  
allowance and manual-unlock behavior can work.

Website patterns support domains, exact URLs, URL prefixes, URL contains, and  
path prefixes. Domain patterns can include subdomains. Application matching  
supports executable paths with their names, command names, desktop IDs and window-title  
matching, where the desktop session supports it.

## Schedules, allowances and Detox

- Reusable schedules support multiple windows, grouped days, individual weekdays,  
and overnight windows.
- Schedule times can be entered in 24-hour or AM/PM format.
- Tier 3 daily allowances may be zero minutes.
- Detox activates selected Tier 2 and Tier 3 lists for one minute through  
twelve weeks. Tier 2 remains strict, while Tier 3 retains its normal allowance and  
manual-unlock behavior.
- While a schedule or detox is active you can only append new rows. You can only delete, while a detox is active. Tier 1 lists are active everytime, which is why you need the Tier 1 unlock key to delete rows from there.

## Browser and application enforcement

Firefox and Chrome/Chromium extensions send navigations through the local daemon  
using Native Messaging. They fail closed for top-level HTTP/HTTPS navigation if  
they cannot verify the daemon connection. The daemon also monitors configured  
applications and terminates matching processes.

The Health section reports daemon, browser integration, policy, and hosts-file  
checks. Browser extensions must be installed by the user and managed browser policy  
is written after the first successful extension heartbeat.

## Overview and Settings

The Overview page contains:

- a first-run welcome modal explaining tiers, extensions and recovery  
credentials
- a URL probe, where you can check the status of urls
- a manual Tier 3 unlock form, that unlocks tier 3 sites

Settings contains Health, Enforcement, Export and Import Rules, Protected  
Changes and Uninstall, Notifications, and Logging. Import uses **Append**:  
existing rules remain and rules are augmented by the new ones.  
Protected Changes and Uninstall provides the five-minute Tier 1 edit unlock,  
the optional Sunday restriction, time-format preference, recovery-credential  
hiding, the welcome-modal action and package uninstall. The Sunday restriction  
is off by default, when enabled Tier 1 editing and uninstall are available only  
on Sunday from 20:00 through 23:59. It can be disabled only during that window.

The package-generated recovery uninstall phrase and Tier 1 edit key are shown  
in the welcome modal until they are hidden. Hiding them removes both files from  
`/etc/blockuntu` and persists that decision across upgrades.

## Notifications and logs

Notification preferences cover website/application blocks, allowance warnings,  
schedule boundaries, and Detox lifecycle events. Notifications are delivered  
while the GUI or tray process is running. Repeated block events are deduplicated.

The daemon writes its event log to `/etc/blockuntu/blockuntu.log`. Statistics  
shows aggregate event counts and schedule active-time totals. Detailed log  
inspection remains terminal-based.

## Boundaries and limitations

- BlocKuntu cannot protect against a user with unrestricted root or sudo access.
- Hosts-file fallback supports domain patterns only; exact-URL and path patterns  
require the browser extension.
- Firefox and Google Chrome are the supported browser-enforcement paths; other  
browsers may be handled as blocked applications in strict mode.
