# Phase 0: Test fixtures and run contract

## Status

Implemented, with final acceptance still open as of 2026-09-17. Ubuntu,
Fedora and CachyOS passed the guest checks in `phase0-20260917-b`. A later fresh
run, `phase0-20260917-final`, reached Ubuntu's desktop but timed out waiting
for its DHCP lease. It stopped before Fedora/CachyOS and retained the stopped
Ubuntu clone for diagnosis. Resolve this repeatability issue and repeat the
consolidated run before marking Phase 0 complete.

See [reports/phase-0-acceptance.md](reports/phase-0-acceptance.md) for the
evidence and [config/README.md](config/README.md) for setup and runner scope.

## Implemented

- Stable test IDs are defined in the layer documents.
- `scripts/prepare-run.py` creates an isolated result directory and generates a
  policy with run-specific object IDs.
- The generated policy includes blocklist and allowlist fixtures for website
  and application Tier 2/Tier 3 behavior, plus one-minute allowances.
- `fixtures/test-site/server.py` provides deterministic local HTTP routes for
  domain, exact URL, URL prefix, URL contains, path prefix, SPA, allowlist, and
  long-running cases.
- `fixtures/test-processes/test_process.c` provides distinct allowed, rejected,
  blocked, parent, and helper process identities.
- `scripts/verify-phase0.sh` builds and exercises the fixtures and validates the
  generated policy with the current daemon/core schema.
- `config/vm-templates.json` records the existing `Ubuntu`, `Fedora`, and
  `CachyOS` libvirt domains as immutable bases.
- `scripts/verify-vm-templates.sh` confirms that configured bases exist and are
  shut off without changing them.
- The templates use SSH user `akhi`; the machine-local client key is referenced
  by the ignored `config/vm-templates.local.json`.
- `scripts/prepare-clone-identity.sh` guards the immutable bases and defines an
  offline reset of copied machine IDs and SSH host keys.
- `scripts/discover-vm-ip.sh` waits for a clone's address from its libvirt DHCP
  lease instead of hard-coding a template address.
- `scripts/phase0-vm.py` copies disk/NVRAM volumes into `Testing/runtime/`,
  prepares a unique guest identity, pins the SSH host key, verifies the desktop
  and fixtures before/after reboot, then removes successful clones.
- `scripts/test_phase0_vm.py` checks rejection of base UUID aliases, changed
  domains, shared storage, hardlinks, symlinks and backing files.
- Guest bootstrap supports GDM, SDDM and Plasma Login Manager. The active
  CachyOS Btrfs subvolumes are mounted explicitly to avoid modifying Snapper
  snapshots. SSH commands invoke `/bin/sh` even when the user's shell is fish.
- Generated binaries, run results, and temporary verification state remain
  under `Testing/` and are ignored by Git.

## Generate a run

From the repository root:

```bash
python3 Testing/scripts/prepare-run.py \
  --vm-template ubuntu \
  --site-port 18080
```

Optionally record an exact package and browser selection:

```bash
python3 Testing/scripts/prepare-run.py \
  --run-id ubuntu-firefox-smoke-001 \
  --vm-template ubuntu \
  --package target/debian/blockuntu_VERSION_amd64.deb \
  --browser Firefox \
  --browser-package snap
```

The run directory contains:

```text
Testing/results/RUN_ID/
├── README.md
├── metadata.json
├── results.json
├── policy/blockuntu-policy.toml
├── fixtures/hosts.entries
├── fixtures/inventory.json
├── evidence/browser/
├── evidence/exports/
├── evidence/journal/
├── evidence/screenshots/
└── reports/
```

The Tier 2 and Tier 3 rules in the generated policy initially reference an
empty inactive schedule. This prevents an import from immediately enabling a
broad website or application allowlist. Later suites activate one selected
rule through Detox or replace its schedule deliberately. Hard fixture rules
are active immediately but target only reserved `.test` hostnames and the
dedicated `bk-test-block` process.

## Start the site and processes

```bash
python3 Testing/fixtures/test-site/server.py --port 18080

Testing/scripts/build-test-processes.sh
Testing/artifacts/fixtures/bin/blockuntu-test-app-a --lifetime 120
```

The generated `fixtures/hosts.entries` line must be installed in the disposable
guest before using the local hostnames. Do not add it permanently to the
development host or clean VM template.

## Verify Phase 0 locally

```bash
Testing/scripts/verify-phase0.sh
```

This check keeps its compiler output and temporary daemon database under
`Testing/runtime/` and deletes that temporary directory when it finishes.

The VM definitions have a separate non-destructive check:

```bash
Testing/scripts/verify-vm-templates.sh
```

## Run VM verification

```bash
python3 Testing/scripts/phase0-vm.py run ubuntu fedora cachyos --run-id phase0-new-run
```

Use a new run ID each time. If guestfs cannot read the host kernel, first run
`bash Testing/scripts/prepare-guestfs.sh`. See the configuration guide for
prerequisites and individual prepare/verify/cleanup commands.

Each disposable clone must pass:

1. the offline identity reset succeeds;
2. DHCP lease discovery returns exactly one IPv4 address;
3. non-interactive public-key SSH succeeds as `akhi`;
4. the clone has a different machine ID and SSH host-key fingerprint from its
   base and from the other clones;
5. a window appears in `akhi`'s real desktop after automatic login and reboot;
6. HTTP and process fixtures start and stop twice with correct identities;
7. SSH host identity and machine ID remain stable after reboot;
8. the base definitions and storage metadata remain unchanged;
9. successful disposable domains and copied storage are removed.

The clean base domains currently have no internal libvirt snapshots. The first
VM harness should therefore create full disposable clones and leave the base
domains shut off. Clone SSH connections must use a per-run known-hosts file
at `Testing/results/RUN_ID/TEMPLATE/known_hosts`.

This phase proves fixture and VM infrastructure readiness. It does not install
BlocKuntu or test package lifecycle, browser extensions, real blocking,
allowlist enforcement, recovery or CI integration. Those remain in the later
phases of [implementation-order.md](implementation-order.md).
