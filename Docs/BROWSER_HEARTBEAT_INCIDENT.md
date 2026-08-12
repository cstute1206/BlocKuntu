# Browser heartbeat incident: unexpected browser closures and tab replacement

## Status

**Diagnosed, not repaired.** This note records the August 2026 investigation of
two related behaviours:

- a browser sometimes closes shortly after it is opened, but works after being
  opened again;
- an already-open tab, including a YouTube video, is replaced by the
  "missing heartbeat" block page.

The evidence confirms why BlocKuntu performs both actions. It does not yet
identify the original cause of every missed heartbeat in the native-messaging
bridge.

## Observed evidence

`blockuntuExample.log` contains repeated `browser_killed_extension_stale`
events from 9 to 12 August for Firefox, Opera, and Chromium. The events show
both enforcement conditions:

| Example | Recorded condition | Meaning |
| --- | --- | --- |
| Firefox, 11 August 13:00 | `heartbeat_missing_since_launch_seconds=69`, `startup_grace_seconds=60` | The current browser session never delivered its first heartbeat. |
| Firefox, 11 August 14:11 | `heartbeat_age_seconds=39`, `grace_seconds=30` | A heartbeat had been received, then became stale. |
| Firefox, 12 August 08:37 | `heartbeat_age_seconds=31`, `grace_seconds=30` | A transient loss crossed the configured stale threshold. |

The many lines at the same timestamp represent individual browser processes
(content, GPU, utility, and so on) being terminated together; they are not
separate browser launches.

The log also contains `url_blocked` events for YouTube-related URLs. Those
specific events are ordinary `ScheduledBlock` decisions for the Tier 2 site
list, not records of a heartbeat-page redirect. The daemon event log does not
record the extension-side redirect itself.

## Confirmed cause of the visible behaviour

The two symptoms are different fail-closed reactions to the same condition:
the extension's heartbeat chain is no longer healthy.

```text
Browser extension
    -> Native Messaging host (one RPC, 3 second timeout)
    -> blockuntud daemon
    -> heartbeat stored for the current browser session
```

### Why a browser closes

Strict mode is configured to terminate a supported browser when its extension
heartbeat is absent or stale:

- Before the first heartbeat of a new session, the daemon allows a startup
  grace period of at least 60 seconds.
- After a current-session heartbeat exists, the default stale-heartbeat grace
  period is 30 seconds.
- The process scanner then terminates all processes belonging to that browser.

This explains why a browser can appear to close directly after launch and then
work after reopening: the initial launch fails to establish a heartbeat before
the startup deadline, while a later launch establishes one in time.

### Why an open tab is replaced

The Firefox extension sends a heartbeat every five seconds, gives each native
RPC three seconds, and treats a successful heartbeat as stale after 15
seconds. It also revalidates open tabs every five seconds. If the backend is
unhealthy during that revalidation, the extension redirects the tab to the
fail-closed page with the summary:

> BlocKuntu cannot verify the daemon heartbeat.

Consequently, this affects any active HTTP(S) tab, including an ongoing YouTube
video; it is not a YouTube-specific rule decision.

## Likely contributing factors, not yet proven root cause

The service journal collected during the investigation contained repeated
`Broken pipe` errors from `blockuntud`. They are consistent with a native
client closing its connection before the daemon can send a reply. Combined
with the three-second RPC timeout, this establishes that requests are being
abandoned, but it does **not** show whether the initial delay or disconnect is
in the extension, native host, daemon, or browser runtime.

The problem affects more than Firefox, so Firefox alone cannot explain all
events. There is nevertheless a Firefox-specific risk: the Manifest V3
background script currently uses `setInterval` for heartbeats. Firefox MV3
background scripts are non-persistent, and Mozilla documents that ordinary
timer APIs are unsuitable for loaded-on-demand background pages; use the
alarms API for periodic background work instead.

- [Firefox background script lifecycle](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background)
- [Firefox alarms API](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/alarms)

The sample starts showing failures shortly after the browser-integration work
that produced extension version `0.2.4`. This is a timing clue only; the
heartbeat intervals themselves were not changed by that work, so it is not
proof of a regression in that commit.

## Proposed solution

The repair should preserve fail-closed protection after a sustained outage,
but avoid treating a short transport fault as a reason to destroy a browsing
session.

1. Add end-to-end heartbeat diagnostics first.
   Record the browser, RPC method, elapsed time, timeout/disconnect reason, and
   daemon handling time at the extension/native-host/daemon boundary. This
   identifies which link fails instead of leaving only a later stale-heartbeat
   event.
2. Make Firefox heartbeat scheduling lifecycle-safe.
   Use `browser.alarms` rather than `setInterval` for periodic heartbeat and
   tab-revalidation work, and restore the required state when the MV3
   background is started again.
3. Add bounded retry tolerance before the destructive response.
   Keep a short, explicit distinction between a single failed heartbeat and a
   sustained loss. Only redirect tabs or terminate the browser after repeated
   failures or a longer independently defined enforcement deadline. Do not
   silently disable blocking or reuse old-session heartbeats for a new browser
   session.
4. Keep the daemon and extension thresholds coherent.
   The extension's 15-second unhealthy decision and the daemon's 30-second
   kill threshold should be reviewed together. The intended user-facing state
   during a recoverable interruption must be specified and tested.
5. Validate the shipped package, not only source code.
   Build a new `.deb`, install it in a clean Debian or Ubuntu VM, and exercise
   initial launch, restart, a long-running YouTube video, background-tab
   suspension, daemon restart, genuine native-host failure, and recovery. A
   real sustained bridge failure must still fail closed and produce actionable
   diagnostics.

## Acceptance criteria for the repair

- A normal browser launch establishes its first heartbeat without a transient
  browser termination.
- A short native-messaging delay does not replace an active video tab or close
  the browser.
- A sustained extension, native-host, or daemon failure still blocks browsing
  and is visible in logs with the failing component and timing.
- Scheduled site-list blocks remain distinct from heartbeat-health failures in
  both the UI and logs.
- Firefox, Chromium, and supported browser variants are tested independently;
  a Firefox-specific timer fix must not be treated as proof that the common
  native-messaging path is reliable.
