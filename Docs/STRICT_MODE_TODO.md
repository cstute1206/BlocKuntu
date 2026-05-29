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
