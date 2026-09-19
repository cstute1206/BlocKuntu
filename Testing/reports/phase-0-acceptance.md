# Phase 0 acceptance status — 2026-09-17

Status: implementation present; final repeatable acceptance remains open.

## Passing evidence

`verify-phase0.sh` passed: host HTTP fixture health, policy generation/schema
validation, executable identities, and distinct parent/helper processes.
`test_phase0_vm.py` passed all eight clone-guard tests.

Run [phase0-20260917-b](../results/phase0-20260917-b/summary.json) records passing
guest verification for all three templates. The runner evolved during that
development run; its per-guest source hashes differ. CachyOS was retried after
fixing Btrfs snapshot inspection, Plasma Login Manager configuration and fish
shell compatibility. Earlier failure evidence is retained in that run.

| Guest-reported OS | Desktop session | Guest verification |
| --- | --- | --- |
| Ubuntu 26.04 LTS | Wayland / GNOME | Passed |
| Fedora Linux 44 Workstation | Wayland / GNOME | Passed |
| CachyOS rolling | Wayland / Plasma | Passed |

Each passing guest result includes:

- dynamically discovered IP and non-interactive SSH using an offline-pinned key;
- distinct machine ID and SSH host key, stable through reboot;
- preserved `authorized_keys` and `sshd_config` checksums;
- a mapped GUI window before and after reboot, with screenshots;
- two HTTP/process fixture cycles, distinct helper identity, child reaping and shutdown;
- unchanged base definitions and disk/NVRAM metadata.

All completed clones from the development runs have been removed. The
unregistered disk/NVRAM copy from the first failed CachyOS inspection was also
removed. Logs, public-key evidence and screenshots remain under `Testing/results/`.
These generated files are ignored by Git.

## Latest fresh run

The finalized runner was invoked with:

```bash
python3 Testing/scripts/phase0-vm.py run ubuntu fedora cachyos --run-id phase0-20260917-final
```

Runner SHA-256:
`280fcddac2f9bf50cac723483701bee0af67d14bd4b658fa678941ed53f84526`.

Ubuntu copied, prepared and booted, but IP discovery returned no lease within
180 seconds. The [failure screenshot](../results/phase0-20260917-final/ubuntu/failure.png)
shows a running desktop. The runner shut down the clone and recorded the
[failure](../results/phase0-20260917-final/ubuntu/result.json). Fedora and CachyOS
were not attempted in this fresh run.

The stopped domain `blockuntu-test-ubuntu-phase0-20260917-final` and its disk
remain for diagnosis. Guest NetworkManager logs confirm DHCP timeouts with
carrier connected; the host DHCP server recorded no requests during the failed
boot. The host journal records Eddie/AirVPN Network Lock activation at 20:50
CEST, between the passing tests and this failure. VPN firewall interference is
the leading explanation, consistent with the operator's confirmation that the
VPN was activated. It has not been proven by a VPN-on/off comparison; no VPN or
firewall settings were changed. Do not describe this as a completed consolidated
acceptance run.

The runner now records read-only network diagnostics on DHCP timeout and gives
an actionable error. All 12 runner regression tests and the local Phase 0
fixture verification passed after that change. These diagnostics improve
failure reporting; they do not bypass or fix a host VPN policy.

Next: diagnose guest networking versus lease discovery, address the cause,
then repeat the complete fresh-clone run. After diagnosis, its stopped clone
can be removed with:

```bash
python3 Testing/scripts/phase0-vm.py cleanup ubuntu --run-id phase0-20260917-final
```

## Scope

Phase 0 validates test infrastructure. Real BlocKuntu blocking, allowlist
enforcement, package installation, browser integration, destructive recovery
and CI wiring remain later-phase work. Base integrity checks compare domain
definitions and storage metadata, not full disk hashes.
