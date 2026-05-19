# Potential New Features

## Cold Turkey-Inspired Feature Backlog

Cold Turkey Blocker has a few mature blocker features that would map well onto this project. The goal is not to clone it, but to borrow the useful concepts and adapt them to a Linux `/etc/hosts` + `systemd` tool.

## Scheduled Blocks

Status: implemented initial CLI/systemd-timer version.

Add recurring schedules for Tier 1 and Tier 2 blocks.

Example config:

```yaml
schedules:
  workday:
    days: [mon, tue, wed, thu, fri]
    start: "09:00"
    end: "17:30"
    tier1_extra:
      - youtube
    tier2_enabled: false

  evening_shutdown:
    days: [sun, mon, tue, wed, thu]
    start: "23:00"
    end: "06:00"
    mode: strict
```

Possible commands:

```bash
focus-hosts schedule-status
focus-hosts schedule-apply
focus-hosts install-schedules
```

Implementation idea:

- Generate `systemd.timer` units from config.
- Timers call `focus-hosts schedule-apply`.
- Schedule state decides which tiers are active.
- Watchdog still restores manual edits.

## Block Applications

Add program blocking alongside website blocking.

Linux approaches:

- Kill processes by executable path.
- Kill processes by process name.
- Kill processes by `.desktop` app ID.
- Block directories of executables.
- Add a `systemd` watchdog that repeatedly terminates blocked programs.
- Optionally use cgroups for stricter process control.

Example config:

```yaml
apps:
  tier1:
    - name: steam
      match:
        process_names: ["steam", "steamwebhelper"]
    - name: discord
      match:
        process_names: ["Discord", "discord"]

  tier2:
    - name: obsidian
      match:
        desktop_ids: ["obsidian.desktop"]
      default_minutes: 10
```

Possible commands:

```bash
focus-hosts app-status
focus-hosts app-open-for steam --minutes 10
focus-hosts app-block-now steam
```

## GUI

Status: implemented initial local web dashboard and Tauri shell. Editing screens are still future work.

Build a small GUI for people who do not want to edit YAML.

Possible stack:

- Tauri + Rust backend + Svelte/React frontend.
- GTK4/libadwaita native Linux app.
- egui for a lightweight single-binary GUI.

Useful views:

- Dashboard: current block state, active Tier 2 window, remaining opens this hour.
- Tiers: edit Tier 1 and Tier 2 domains.
- Schedules: weekly calendar grid.
- Apps: configure blocked programs.
- Logs: show opens, denies, restores, and watchdog repairs.
- Settings: paths, timers, limits, browser command.

Important design rule:

The GUI should not become an easy bypass. It should call the same CLI/helper policy layer instead of writing files directly.

## Locked Blocks

Add lock modes that make changes annoying during active blocks.

Possible lock types:

- Timer lock: no config changes until a chosen time.
- Random text lock: type a long random phrase before changing config.
- Time-range lock: config changes only allowed during a maintenance window.
- Restart lock: changing strict settings requires a reboot.
- Password lock: useful if an accountability partner sets the password.
- Not unlockable at all during active block

Example:

```bash
focus-hosts lock-config --until "2026-05-18 18:00"
focus-hosts lock-config --random-text 300
focus-hosts unlock-config
```

## Breaks and Allowances

Status: implemented daily allowances and per-session caps for `open-for`.

Add planned breaks instead of only emergency `open-for`.

Types:

- Pomodoro: 25 minutes blocked, 5 minutes open.
- Allowance: Reddit gets 10 total minutes per day.
- Scheduled breaks: Tier 2 access is allowed only during specific windows.
- Manual break: requires reason and consumes allowance.
- Unlock for a payment

Example config:

```yaml
allowances:
  reddit:
    daily_minutes: 10
    max_session_minutes: 2
  youtube:
    daily_minutes: 15
    max_session_minutes: 5
```

## Frozen Turkey Mode

Cold Turkey has a mode that locks, logs off, or shuts down the computer. A Linux version could add hard stop actions.

Possible actions:

- Lock screen.
- Log out.
- Suspend.
- Shut down.
- Start a full internet block.
- Start a "walk away" timer where only essential apps remain usable.

Possible commands:

```bash
focus-hosts frozen lock-screen --minutes 20
focus-hosts frozen suspend-at "23:30"
focus-hosts frozen shutdown-at "00:00"
```

Linux implementation options:

- `loginctl lock-session`
- `systemctl suspend`
- `systemctl poweroff`
- desktop-specific logout commands

## Block Entire Internet With Exceptions

Support an allowlist mode:

```yaml
modes:
  deep_work:
    internet: block_all
    allow:
      - docs.rs
      - crates.io
      - github.com
      - stackoverflow.com
```

Implementation options:

- DNS mode is better than `/etc/hosts` for this.
- Local DNS resolver like `dnsmasq` or `unbound`.
- Firewall rules for non-DNS traffic.

## Specific URL, Keyword, and Wildcard Rules

`/etc/hosts` cannot enforce path-level rules, but future versions could support them with a browser extension or local proxy.

Feature ideas:

- Block specific URLs.
- Block YouTube channels.
- Block URL keywords.
- Block search queries containing specific words.
- Block embedded content such as YouTube iframes.
- Allow exceptions inside otherwise blocked domains.

Possible implementation:

- Browser extension for Firefox.
- Local HTTP(S) proxy with generated CA, though this is much more invasive.
- Native messaging bridge from browser extension to `focus-hosts`.

## Statistics and Analytics

Status: implemented local JSONL summaries for today/week/month.

Add local-only usage stats.

Possible reports:

```bash
focus-hosts summary --today
focus-hosts summary --week
focus-hosts summary --month
```

Metrics:

- top opened Tier 2 sites,
- denied Tier 1 attempts,
- total time temporarily opened,
- watchdog repair count,
- hourly heatmap,
- most common reasons,
- streaks without Tier 2 opens.

Keep this local-first. No cloud account, no telemetry.

## Application Password / Accountability Mode

Add an optional password gate around sensitive settings.

Use cases:

- shared computer,
- accountability partner,
- parental controls,
- "I do not want config edits to be casual."

Protected actions:

- changing Tier 1,
- disabling watchdog,
- uninstalling CLI,
- increasing open limits,
- reducing cooldowns,
- disabling schedules.

Implementation note:

This is only meaningful if the password hash/config is root-owned and the daily user cannot casually edit it without noticing. With unrestricted sudo, it is still friction rather than a hard boundary.

## Prevent Time-Change Bypass

Scheduled blockers can be bypassed by changing the system clock.

Potential mitigations:

- Detect large backward time jumps.
- Store monotonic timestamps where possible.
- Log clock changes.
- Watch `timedatectl` changes.
- Temporarily deny `open-for` after suspicious time jumps.

Linux options:

- Compare wall-clock time with monotonic uptime.
- Track last-seen timestamps in `/var/lib/focus-hosts/state.json`.
- Watch systemd journal events for time sync or time changes.

## Block Task Managers / Process Kill Tools

Cold Turkey has advanced anti-bypass options around task managers. Linux equivalents are tricky because unrestricted sudo can always win, but we can add friction.

Possible targets:

- `gnome-system-monitor`
- `plasma-systemmonitor`
- `ksysguard`
- `htop`
- `btop`
- `killall`
- `pkill`

Possible policy:

- During strict scheduled blocks, kill graphical system monitors.
- Log attempts to open process managers.
- Warn when focus-hosts service processes are stopped.
- Right now it is maybe even good enough for now. We are rebooting as soon as systemd is killed.

This should be optional. Developers often need process tools for legitimate work.

## Custom Block Page

When using only `/etc/hosts`, blocked sites usually just fail to load. A future DNS/proxy/browser-extension mode could show a local block page.

Ideas:

- Show the reason the site is blocked.
- Show time until unblock.
- Show today's open count.
- Show a short reminder message.
- Offer "queue this URL for later" instead of opening now.

## Command Line Automation

Expand the CLI so external tools can start, stop, toggle, and inspect blocks.

Possible commands:

```bash
focus-hosts mode start deep_work
focus-hosts mode stop deep_work
focus-hosts mode toggle strict
focus-hosts blocklist add reddit reddit.com www.reddit.com
focus-hosts blocklist remove reddit
focus-hosts export-state --json
```

This would make it easier to integrate with:

- shell scripts,
- keyboard shortcuts,
- window manager bindings,
- cron/systemd timers,
- a future GUI.

## Direct ioctl Locking

Replace `chattr` subprocess calls with direct `FS_IOC_GETFLAGS` and `FS_IOC_SETFLAGS` calls.

Benefits:

- fewer external command dependencies,
- better errors,
- easier logging around immutable flag changes.

## Narrow Sudo Helper

Split the project into:

```text
focus-hosts
focus-hosts-helper
```

The normal CLI would stay unprivileged. The helper would be root-owned and only support a small set of privileged operations.

This would make sudoers rules easier to constrain later, even if the daily user currently keeps broad sudo.

## Desktop Notifications

Send notifications when:

- Tier 2 opens,
- Tier 2 restores,
- the watchdog repaired manual edits,
- the hourly limit is reached.

Possible command:

```bash
notify-send "focus-hosts" "Reddit restored and blocked again"
```

## Better Watchdog Status

Add:

```bash
focus-hosts watchdog-status
```

It could print:

- whether the path unit is enabled,
- whether it is active,
- last repair event,
- last restore event,
- current runtime state.

## Safer Install Flow

Add:

```bash
focus-hosts init
```

This could:

- copy example config to `/etc/focus-hosts/config.yml`,
- build or install the binary,
- run `rebuild`,
- install the watchdog,
- print verification commands.

## Config Validation

Add:

```bash
focus-hosts check-config
```

Checks:

- duplicate domains,
- Tier 1/Tier 2 conflicts,
- invalid URLs or domains,
- missing Firefox,
- missing systemd,
- missing `chattr`.

## Better Limit Policies

Current policy is 2 successful `open-for` calls per rolling hour.

Future options:

- per-site limits,
- daily limits,
- escalating cooldowns,
- queue-only mode after repeated opens,
- different limits by day/time.

## Shell Completion

Generate completions:

```bash
focus-hosts completions bash
focus-hosts completions zsh
focus-hosts completions fish
```

## Import Existing Hosts Blocks

Add a command that reads an existing hosts file and suggests config entries:

```bash
focus-hosts import-hosts
```

## Log Summaries

Add:

```bash
focus-hosts summary --today
focus-hosts summary --week
```

Useful output:

- opens by site,
- denied attempts,
- watchdog repairs,
- most common reasons.

## External DNS Mode

Support generating blocklists for:

- Pi-hole,
- NextDNS,
- dnsmasq,
- unbound.

This would make Tier 1 less dependent on local `/etc/hosts`.

## Tamper-Resistant Watchdog Hardening

Make the watchdog harder to stop accidentally or impulsively.

One possible Windows-specific idea would be a watchdog DLL loaded into a protected or long-lived process so the machine has to restart if that host process is killed. This should be treated as a high-risk research idea, not a default implementation path: DLL injection into system processes is brittle, can look malware-like to security tools, and can destabilize the operating system.

Safer Linux-first options for this project:

- Run the watchdog as a root-owned `systemd` service/path unit with `Restart=always` where useful.
- Add a second timer-based repair unit in addition to the path unit.
- Lock down config and unit files with root ownership and limited sudo rules.
- Add a `focus-hosts health` command that verifies the hosts block, immutable flag, watchdog units, and runtime state.
- Add an optional "strict mode" installer that explains the limits and asks for explicit confirmation before enabling hardening.

Possible commands:

```bash
focus-hosts install-watchdog --strict
focus-hosts health
focus-hosts repair-all
```

## Desktop Widget

Add a small desktop widget for glanceable status and quick safe actions.

Useful widget states:

- Current mode: fully blocked, Tier 2 open, or watchdog repairing.
- Remaining `open-for` uses in the rolling hour.
- Active temporary opening with countdown.
- Last watchdog repair time.
- Quick buttons for status, logs, and opening the full GUI.

Possible Linux implementations:

- Tauri tray app with a compact popover.
- GTK/libadwaita panel-style mini window.
- KDE Plasma widget that shells out to `focus-hosts status --json`.
- GNOME extension backed by a small JSON status command.

Design note:

The widget should be status-first. Any action that changes blocking state should call the same CLI policy layer and should not directly edit `/etc/hosts`, runtime state, or config files.

## To-Do List and Intent Tracking

Add a lightweight local to-do list that connects blocking decisions to the user's actual intention for the day.

Useful behavior:

- Daily task list with active, done, deferred, and abandoned states.
- A current-focus task that appears in `status`, the widget, or the GUI.
- Optional requirement to choose a task before opening Tier 2 access.
- Open-for reasons can link to a task instead of being free text only.
- Completed tasks can appear in daily and weekly summaries.
- Track time worked on specific tasks and what windows/sites were open to adjust for latter tasks 

Possible commands:

```bash
focus-hosts todo add "Write project notes"
focus-hosts todo start 3
focus-hosts todo done 3
focus-hosts todo list --today
```

## Automatic Productivity Feedback

Generate local-only feedback based on blocks, opens, reasons, tasks, and optional activity data.

Useful feedback:

- Daily review: productive blocks, distraction attempts, completed tasks, and repeated patterns.
- Gentle warnings when Tier 2 opens cluster around certain hours.
- Suggestions for better schedules, stricter cooldowns, or planned breaks.
- Streaks for days without emergency opens.
- "Friction score" that shows how much the blocker helped interrupt impulses.

Possible commands:

```bash
focus-hosts feedback --today
focus-hosts feedback --week
focus-hosts feedback suggest-config
```

Privacy rule:

Feedback should be computed locally by default. If any future AI summary is added, it should require explicit opt-in and make clear which data is sent.

## ActivityWatch Integration

Integrate with ActivityWatch so `focus-hosts` can correlate block events with real app and browser activity.

Potential data sources:

- Active window titles and app names from ActivityWatch.
- Browser URL buckets if the user has enabled browser tracking.
- AFK status and session boundaries.
- Time spent in productive, neutral, and distracting categories.

Possible behavior:

- Import ActivityWatch summaries into the local focus-hosts log database.
- Compare `open-for` windows with actual browser usage.
- Detect when a blocked-site urge shifted into another distracting app.
- Generate better productivity feedback and schedule suggestions.
- Show activity context in the future GUI or desktop widget.

Possible commands:

```bash
focus-hosts activitywatch import --today
focus-hosts activitywatch summary --week
focus-hosts feedback --with-activitywatch
```

Implementation notes:

- Use ActivityWatch's local API rather than reading internal files directly.
- Keep imported data minimal and local.
- Add category mapping in config so users decide what counts as productive or distracting.

## Support Tasker

Support Tasker or something similar, so that a scheduled block then could also start a blocker with the app "digital detox" on android.

## Rename Project

I dislike the name focus-hosts. We need a nice name. Maybe something like:

- Blockist
- BlocKuntu
