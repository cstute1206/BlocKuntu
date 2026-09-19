# Phase 0 guest fixtures

- `guest-bootstrap.sh` runs offline only inside a validated disposable clone.
  It prepares SSH keys, desktop automatic login and test sudo access. The
  machine-ID reset happens in the host runner before bootstrap.
- `gui-smoke.py` uses Python's standard library and the guest's GTK 3 library.
  It creates a window in the logged-in desktop and writes `gui-ready.json`
  only when GTK reports the window mapped. The runner captures a screenshot
  and stops its transient user service. It repeats this after reboot.
- `guest-smoke.py` exercises the local HTTP server and all dedicated process
  executables twice. It checks `/proc` identities, parent/helper separation,
  child reaping and process shutdown. No policy is activated.

These files are copied to `/home/akhi/Testing/phase0/` in the guest. The process
executables are built on the Ubuntu host; a guest with an incompatible C
runtime will fail the smoke check instead of silently skipping it.
