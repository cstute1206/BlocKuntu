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

Firefox, LibreWolf, Waterfox, Chrome, Chromium, Brave, Opera, Microsoft Edge, and Vivaldi extensions send navigations through the local daemon
using Native Messaging. They fail closed for top-level HTTP/HTTPS navigation if  
they cannot verify the daemon connection. The daemon also monitors configured  
applications and terminates matching processes.

The Health section reports daemon, browser integration, policy, and hosts-file
checks. Users install the Firefox extension from AMO in Firefox, LibreWolf, or Waterfox,
and the Chrome Web Store extension in Chrome, Chromium, Brave, Opera, Microsoft Edge, or
Vivaldi. After each extension's first successful heartbeat, BlocKuntu writes its managed policy
and locks that same store-installed extension when the browser installation supports managed
policy. Opera and Edge require turning on **Allow extensions from other stores**; Vivaldi requires
enabling Web Store in its Google Extensions setting. Strict
Chromium, Brave, Opera, and Vivaldi Snaps receive a per-user copy of the Native
Messaging bridge and a manifest in their Snap-visible profile when BlocKuntu
starts. The bridge uses an authenticated local TCP connection because strict
Snaps cannot reach the daemon's Unix socket. Opera and Vivaldi Snap extensions
can therefore send heartbeats, but their current strict Snap packages cannot
read host-managed policy files. Their Snap installations cannot be force-
installed or locked by policy, have private browsing disabled by policy, or use
the private URL-blocklist policy. See
[the installation limitation](INSTALLATION.md#opera-and-vivaldi-snap-policy-limitation)
for the reason and support boundary. Vivaldi Flatpak still requires clean-VM
validation.

In **Settings → Protected changes and uninstall**, Chromium-family private browsing has three
explicit modes: disable private windows, leave the extension toggle to the user's manual consent,
or use the browser's private URL-blocklist policy. Manual consent can be revoked in the browser.
Private windows are disabled all the time by default, or only while a schedule or Detox session is
active. A separate protected-change window controls when either Chromium private-browsing setting
can be altered: all the time (the default), only while no schedule or Detox is active, or Sunday
from 20:00 through 23:59.

URL-policy mode includes active Hard, Scheduled Block, and Controlled Access domain, exact-URL,
and full URL-prefix patterns. Controlled Access patterns remain blocked there even when their daily
allowance still has time. URL-contains and path-only patterns cannot be represented safely by the
browser policy and are reported as omitted. The policy is limited to 1,000 patterns and requires a
browser release that supports the private URL-blocklist policy, so validate the chosen
browser/version in a clean VM.

## Overview and Settings

The Overview page contains:

- a first-run welcome modal explaining tiers, extensions and recovery  
credentials
- a URL probe, where you can check the status of urls
- a manual Tier 3 unlock form, that unlocks tier 3 sites

Settings contains Health (including Enforcement), Rules and logging, Protected
Changes and Uninstall, and Notifications. Import uses **Append**: existing
rules remain and rules are augmented by the new ones.
Protected Changes and Uninstall provides the five-minute Tier 1 edit unlock,
time-format preference, recovery-credential hiding, the welcome-modal action
and package uninstall. You can choose whether Tier 1 editing and uninstall are
available on Sunday from 20:00 through 23:59, only while no schedule or Detox
is active, or at any time. The same choice controls when the automatic Tier 1
blocked-browser list is active; it fails closed if clock tampering is detected.

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
- Firefox, LibreWolf, Waterfox, Google Chrome, Chromium, Brave, Opera, Microsoft Edge, and Vivaldi are the
supported browser-enforcement paths. Other browsers may be handled as blocked
applications in strict mode.
- Opera and Vivaldi strict Snaps are not supported for **managed-policy**
  enforcement. Their Native Messaging integration can work, but policy-backed
  extension locking, Incognito disabling, and private URL blocking cannot work
  until their Snap packages expose a policy directory to the sandbox.
