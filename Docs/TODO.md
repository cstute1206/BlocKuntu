# Strict Mode TODO

## Stronger Network Fallback

Current strict mode is process-based:

- force-install supported browser extensions through managed policy
- require recent Firefox/Chrome extension heartbeats
- close Firefox/Chrome when their required extension heartbeat is stale
- hard-block unsupported browsers as applications

Future hardening option:

- add a root-only network fallback that blocks browser web traffic when extension health is stale
- evaluate `nftables` first because it can target user/process/network traffic more honestly than `/etc/hosts`
- consider a local DNS or proxy layer only if it can preserve exact policy semantics and fail safely
- expose this as an explicit opt-in strict-mode setting, not as the default dev behavior
- add install, uninstall, status, and repair commands before enabling it in production

Acceptance criteria:

- stale extension heartbeat blocks supported browser network access without killing unrelated apps
- recovery is automatic after a fresh extension heartbeat
- uninstall removes every firewall/DNS/proxy rule created by BlocKuntu
- docs include verification commands for active rules and cleanup

## Full Debian Package

Initial `.deb` packaging exists in `scripts/package-deb.sh` and
`packaging/deb/`. It packages the daemon, native host, Tauri GUI, systemd
units, Native Messaging manifests, a minimal strict-browser config, and local
extension artifacts used as later policy install sources. Browser policy repair
is deferred until the first extension heartbeat.

Remaining packaging hardening:

- verify the package on a clean Ubuntu/Debian VM
- move from raw `dpkg-deb` metadata to a fuller Debian packaging workflow if
  external distribution becomes a goal
- audit runtime dependencies across Ubuntu LTS versions; `wmctrl` is currently
  only recommended because title matching can degrade without blocking install
- preserve `/etc/blockuntu/config.toml` as a conffile across upgrades
- document that package installation cannot safely infer which desktop user
  should be added to the `blockuntu` group without an explicit installer prompt
  or post-install command

Acceptance criteria:

- `apt install ./blockuntu_VERSION_amd64.deb` installs runtime dependencies and
  starts the daemon on a clean Ubuntu/Debian VM
- the GUI launches from the app launcher without build tools installed
- `node`, `npm`, `rustc`, and `cargo` are only needed on the build machine
- upgrades preserve user config and restart systemd units cleanly
- removal stops units and removes installed BlocKuntu-owned files without
  deleting user policy/config state unexpectedly
