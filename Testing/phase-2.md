# Phase 2: Package CI

Local execution evidence: [reports/phase-2-validation.md](reports/phase-2-validation.md).

`.github/workflows/package-ci.yml` runs on main, `v*` tags, and manual dispatch.
It builds Ubuntu 24.04 and Debian 13 DEBs, Fedora 43 RPMs, Arch packages, and
both browser extension archives on x86-64 runners. Inspector regression tests
must pass first. Jobs are independent, and failures do not cancel sibling jobs.
The workflow does not publish releases or replace Layer 1 source validation.
Release acceptance requires Layer 1 and the later VM layers on the same commit.

## Version and build contract

`package-ci/contract.json` defines distribution version `0.2.0`, revision `1`,
architecture, dependencies, package paths, and native-host identities. The
builders have matching defaults. Fedora can append its `.fcNN` release suffix.
GUI/crate versions remain unchanged; Firefox and Chrome retain independent
versions. Version tags for this workflow use `v0.2.0-1` and must match the contract.

Builds use `npm ci`, Cargo `--locked`, and Tauri's forwarded `-- --locked`.
CI requires a clean checkout and committed lockfiles, and checks that the build
has not changed them. The Debian and Ubuntu builds use Rust 1.93.0; Fedora and
Arch use distribution toolchains. Live OS repositories and the rolling Arch
image mean this is functional reproducibility, not byte-for-byte reproducibility.
Record environment details when investigating differences between runs.

## Commands

Run from the repository root. Each invocation creates a new result directory:

```bash
python3 Testing/scripts/test_package_ci.py
python3 Testing/scripts/package-ci.py deb
python3 Testing/scripts/package-ci.py rpm
python3 Testing/scripts/package-ci.py arch
python3 Testing/scripts/package-ci.py firefox
python3 Testing/scripts/package-ci.py chrome
```

The Debian command needs the native build dependencies listed in CI. The RPM
command runs inside the Fedora build environment (`package-ci/Dockerfile.fedora`).
The Arch command requires Docker access and invokes the non-root package user
in the existing disposable Arch builder. Python 3.11+, `dpkg-deb`, `rpm`,
`rpmbuild`, and `bsdtar` are needed to run all inspector regression tests.
Extension jobs additionally need Node/npm and `zip`.

If local Docker DNS is broken while host DNS works, the Arch runner accepts
`--docker-network host`; this affects only its disposable builds. Likewise,
local Debian/Fedora Docker builds and runs can use `--network host`. CI keeps
the default Docker network. No host resolver or firewall changes are made.
Keep long-running local source copies under an ignored `target/` directory,
rather than `/tmp`, and retain final reports under `Testing/results/package-ci/`.

For local, uncommitted development, add `--allow-dirty`. Reports then explicitly
set `build_provenance_verified: false`; they are not evidence of a clean release
commit. This does not bypass content assertions or lockfile-change detection.

To inspect an existing package without rebuilding it:

```bash
python3 Testing/scripts/package-ci.py deb --artifact /absolute/path/blockuntu_0.2.0-1_amd64.deb
```

Use `rpm` or `arch` for the other formats. Inspection-only reports skip the
build case and do not assert artifact provenance. `--output-dir` selects a new,
empty output directory; reusing an old result directory is rejected.

## Evidence

`Testing/results/package-ci/<component>-<run>/` contains:

- `result.json`: commit, clean/dirty state, environment, lockfile hashes,
  command results, per-case outcomes, metadata, artifact hashes, and limitations;
- `report.md`: readable case results, including deferred upgrade cases;
- numbered build logs, including failed commands;
- `inventory.json` and `hooks.json` for distribution artifacts;
- `artifacts/` and `SHA256SUMS` for the exact inspected files.

GitHub retains these together for 14 days, also on failure. Setup/image-pull
failures before the runner starts remain in the Actions log. Source archives
emitted by the Arch builder are retained and hashed but never count as binary
packages. Firefox XPI is an unsigned build artifact, not proof of store signing
or browser acceptance.

## Assertions and boundaries

`package_inspect.py` reads archive payloads without extraction or hook execution.
It validates x86-64 ELF headers, unit paths and commands, strict-mode TOML,
Native Messaging identities, both confined-browser helpers and their wrappers,
desktop assets, package ownership/modes, dependencies, licenses, and packaged
lifecycle/recovery declarations. Invalid archives, duplicate members, path
traversal, unexpected links, and device files fail closed. RPM build-id symlinks
are allowed only when they resolve to one of the three packaged executables.

Lifecycle assertions establish packaged declarations and shell syntax only.
They do not prove removal protection, systemd execution, runtime file permissions,
or actual browser-policy installation. These need VM acceptance, particularly
package-manager-specific removal semantics. Runtime credentials and policy
snapshots must not be baked into the package.

Extension inspection verifies manifests, versions, referenced assets, and that
archived scripts match this run's build output. It makes no behavioral claim
about allowlists or daemon/GUI schema compatibility. `PKG-ALLOW-002` and
`PKG-ALLOW-003` are retired, not silently reported as passing.

`PKG-UPGRADE-001` and `PKG-UPGRADE-002` are explicitly skipped: there is no
previous accepted release and upgrade implementation is deferred by agreement.
Before shipping a subsequent release, implement the version comparison and
upgrade-path coverage, including the known Debian `prerm` cleanup concern.

The fixture suite uses synthetic ELF headers to exercise production Debian
assembly and damaged-package detection. Those fixtures are not runnable builds;
real distribution build results are separate evidence.
