# focus-hosts

Rust CLI for a two-tier `/etc/hosts` blocker.

- Tier 1 domains are always blocked.
- Tier 2 domains are blocked by default, but can be opened in a disposable Firefox session for a short timed window.
- `open-for` is limited to 2 successful openings per rolling hour by default.
- Optional daily allowances can limit total Tier 2 minutes per site.
- Optional recurring schedules can add stricter blocks or change Tier 2 blocking by time of day.
- Every hosts rebuild ends by running `chattr +i` on the hosts file.

## Build

```bash
cargo build --release
```

Install the command globally:

```bash
./target/release/focus-hosts install-cli
```

That copies the current binary to:

```text
/usr/local/bin/focus-hosts
```

After that, `focus-hosts` should work from any terminal, assuming `/usr/local/bin` is in your `PATH`.

## Configure

Start from the example config:

```bash
sudo mkdir -p /etc/focus-hosts
sudo cp focus-hosts.yml.example /etc/focus-hosts/config.yml
sudo editor /etc/focus-hosts/config.yml
```

The default config writes to `/etc/hosts` and logs to:

```text
~/.local/state/focus-hosts/access.jsonl
```

## Use

Inspect a URL:

```bash
./target/release/focus-hosts explain https://www.twitch.tv/example
./target/release/focus-hosts explain https://old.reddit.com/r/linux/comments/abc/title/
```

Show safe quoted `open-for` examples for every configured Tier 2 site:

```bash
./target/release/focus-hosts examples
```

Rebuild `/etc/hosts` with all Tier 1 and Tier 2 domains blocked:

```bash
./target/release/focus-hosts rebuild
```

This command removes immutability with `chattr -i`, writes the generated hosts block, then applies `chattr +i`.

After `install-cli`, the same command is:

```bash
focus-hosts rebuild
```

Install the watchdog:

```bash
./target/release/focus-hosts install-watchdog
```

This installs and enables:

```text
/etc/systemd/system/focus-hosts-watchdog.path
/etc/systemd/system/focus-hosts-watchdog.service
```

The path unit watches `/etc/hosts`. If the file changes manually, the service runs:

```bash
focus-hosts watch-repair
```

`watch-repair` restores the generated hosts block and applies `chattr +i` again. It will skip repair while an intentional `open-for` window is active, using the runtime state file:

```text
/run/focus-hosts/open.json
```

Because `install-watchdog` runs `systemctl enable --now focus-hosts-watchdog.path`, the watchdog path unit starts now and starts again automatically after reboot.

Temporarily open a Tier 2 URL:

```bash
./target/release/focus-hosts open-for "https://old.reddit.com/r/linux/comments/abc/title/"
```

After `install-cli`:

```bash
focus-hosts open-for "https://old.reddit.com/r/linux/comments/abc/title/"
```

Keep URLs in quotes. Shell characters such as `&` are interpreted by your shell before `focus-hosts` starts, so an unquoted YouTube URL can accidentally run the command in the background or cut off query parameters.

The command:

- checks that the URL is Tier 2,
- enforces the 2-per-hour limit,
- asks for a reason,
- removes the matching Tier 2 hosts entries,
- opens Firefox with a temporary profile and `--no-remote`,
- schedules a root `systemd-run` restore job,
- prints a live countdown for the remaining access window,
- kills the temporary Firefox session after 2 minutes,
- deletes the temporary Firefox profile,
- restores the hosts block.

Use `--no-countdown` if you want the command to return immediately after scheduling the restore job.

Run `open-for` as your normal user. The command will ask sudo only for the operations that need it. If you accidentally run it with `sudo`, it will try to open the temporary Firefox session as the original sudo user instead of root.

Show recent logs:

```bash
./target/release/focus-hosts logs
```

Show local usage statistics:

```bash
focus-hosts summary --today
focus-hosts summary --week
focus-hosts summary --month
```

Show or apply recurring schedules:

```bash
focus-hosts schedule-status
focus-hosts schedule-apply
focus-hosts install-schedules
```

`install-schedules` creates a systemd timer that runs `schedule-apply` once per minute, so time-based policy changes are reflected without manually rebuilding.

Start the local GUI:

```bash
focus-hosts gui
```

Then open:

```text
http://127.0.0.1:9876
```

The GUI is local-only and uses the same config, log, runtime state, rebuild, and restore paths as the CLI.

Run the Tauri desktop shell:

```bash
npm install
npm run tauri dev
```

On Ubuntu-like systems, Tauri also needs native desktop development packages. If `cargo check --manifest-path src-tauri/Cargo.toml` reports missing `dbus-1` or WebKitGTK pkg-config files, install the Tauri Linux prerequisites first, for example:

```bash
sudo apt install libdbus-1-dev pkg-config libwebkit2gtk-4.1-dev
```

Remove the watchdog:

```bash
./target/release/focus-hosts uninstall-watchdog
```

Remove the global command:

```bash
focus-hosts uninstall-cli
```

## Notes

The first real `rebuild` or `open-for` call touches `/etc/hosts`, so read the config before running it.

If a previous root run created the log file as root, the next normal run will try to repair ownership of the log directory automatically.

If your account has unrestricted sudo, this tool adds strong friction but not a mathematical security boundary. For stronger enforcement, use a daily account without broad sudo or move Tier 1 blocking to router DNS, Pi-hole, or NextDNS.
