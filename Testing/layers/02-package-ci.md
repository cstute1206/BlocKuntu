# Layer 2: Package CI

## Purpose

This layer builds the distribution artifacts and inspects their contents. It
does not start a real desktop session, validate Native Messaging in a browser,
or prove that systemd services operate after installation.

## Agreed initial-release scope

Distribution packages share upstream version `0.2.0` and revision `1`:
Debian `0.2.0-1`, RPM `0.2.0-1` (plus Fedora's distribution suffix),
and Arch `0.2.0-1`. Application and browser-extension versions remain independent.
The machine-readable contract is `Testing/package-ci/contract.json`.

Targets are Ubuntu 24.04, Debian 13, Fedora 43, and rolling Arch/CachyOS,
all x86-64 (`amd64` in Debian metadata). These are build targets; installed
acceptance remains Layer 3. Reproducibility means functional builds from recorded
source and locked language dependencies, not byte-identical package archives.
There is no previous accepted release; both upgrade cases are explicitly deferred.

Implementation and commands: [../phase-2.md](../phase-2.md).

## Package matrix

| Distribution target | Build command | Expected artifact |
| --- | --- | --- |
| Debian/Ubuntu | `scripts/package-deb.sh` | `target/debian/blockuntu_VERSION_ARCH.deb` |
| Fedora | `scripts/package-rpm.sh` | `target/rpm/blockuntu-VERSION-RELEASE.ARCH.rpm` |
| Arch/CachyOS | `scripts/package-arch-docker.sh` | `target/arch/blockuntu-VERSION-RELEASE-ARCH.pkg.tar.zst` or another valid `.pkg.tar.*` compression |

A source `.tar.gz` is not an installable CachyOS artifact.

## Build and metadata cases

| ID | Test case | Expected result |
| --- | --- | --- |
| PKG-BUILD-001 | Build the Debian artifact from a clean checkout and committed lockfiles. | The build succeeds and produces exactly the expected `.deb`. |
| PKG-BUILD-002 | Build the RPM in its supported build environment. | The build succeeds and produces exactly the expected binary RPM. |
| PKG-BUILD-003 | Build the Arch artifact using the disposable Arch builder. | The build succeeds as a non-root package user and produces `.pkg.tar.*`. |
| PKG-BUILD-004 | Build both browser extensions from lockfiles. | Firefox and Chrome extension archives contain their expected manifests and built scripts. |
| PKG-META-001 | Compare application, distribution, package, extension, and release versions according to the release-version policy. | Distribution version/revision matches the contract; extension manifest and npm versions agree within each extension. No cross-component equality is required. |
| PKG-META-002 | Inspect package architecture, name, dependency declarations, license, and description. | Metadata matches the selected distribution and architecture. |
| PKG-META-003 | Compute SHA-256 checksums for every artifact. | Every artifact has an unambiguous checksum recorded with the workflow result. |

## Package-content cases

Run these assertions against the unpacked package tree rather than the source
tree.

| ID | Test case | Expected result |
| --- | --- | --- |
| PKG-CONTENT-001 | Inspect installed executables. | The daemon, native host, and GUI exist at the package-defined paths and are executable. |
| PKG-CONTENT-002 | Inspect systemd units. | Socket, daemon, watchdog, hosts path, and hosts repair units are present at distribution-appropriate paths. |
| PKG-CONTENT-003 | Inspect the default configuration. | Strict-mode defaults are present and the config is valid TOML. |
| PKG-CONTENT-004 | Inspect Native Messaging manifests. | Firefox-family and Chromium-family manifests point to the packaged native-host executable and contain the expected extension IDs/origins. |
| PKG-CONTENT-005 | Inspect browser-policy target paths. | Native Messaging paths are present; packaged cleanup hooks represent the documented browser-policy paths. Runtime policy creation is tested in Layer 3. |
| PKG-CONTENT-006 | Inspect confined-browser helpers. | Required Firefox and Chromium Snap/Flatpak setup helpers are present where the package advertises them. |
| PKG-CONTENT-007 | Inspect desktop integration. | Desktop entry and required application icons exist with readable permissions. |
| PKG-CONTENT-008 | Inspect maintainer scripts and package hooks. | Install, removal authorization, service lifecycle, hosts cleanup, and recovery-file handling are included with valid shell syntax. Upgrade semantics are deferred. |
| PKG-CONTENT-009 | Inspect ownership and permissions declared by the package. | Archive ownership/modes are restricted; runtime credentials, tokens, and recovery policies are not prepopulated in the package. Runtime-created permissions require Layer 3. |
| PKG-CONTENT-010 | Search the package tree for development paths. | No manifest or unit references `/usr/local` or a checkout-only build path unless explicitly part of the package contract. |

## Static hardening and allowlist cases

| ID | Test case | Expected result |
| --- | --- | --- |
| PKG-HARD-001 | Inspect Debian, RPM, and Arch removal hooks. | Direct removal protection and authorized handoff logic are packaged consistently. |
| PKG-HARD-002 | Inspect watchdog and hosts-repair installation. | Both enforcement recovery mechanisms are enabled by the installed package contract. |
| PKG-HARD-003 | Inspect policy-recovery paths and initial credential creation. | Packaged hooks declare recovery paths and restricted credential/token creation permissions. Actual creation requires Layer 3. |
| PKG-ALLOW-001 | Parse the packaged default configuration. | The strict-mode-only config introduces no policy lists or accidental empty allowlists. |

`PKG-ALLOW-002` and `PKG-ALLOW-003` are retired: binary contents do not prove schema compatibility or allowlist behavior. Layer 1 covers source behavior and Layer 3 covers installed integration.

## Upgrade-artifact cases (deferred)

| ID | Test case | Expected result |
| --- | --- | --- |
| PKG-UPGRADE-001 | Compare the candidate with the previous accepted package version. | `apt`, `dnf`, or `pacman` considers the candidate newer. |
| PKG-UPGRADE-002 | Inspect upgrade hooks for service and policy handling. | An upgrade is distinguishable from final removal and does not execute destructive purge cleanup. |

Both cases are reported as skipped for this first release because no accepted baseline exists and upgrade implementation is deferred. Before the next release, implement these checks and the actual upgrade/policy-preservation tests in Layers 3 and 4. The known Debian `prerm` upgrade cleanup concern remains open.

## Completion criteria

- Artifacts and inspection reports are retained together.
- A successful package job never claims installed-package acceptance.
- Published release candidates use only artifacts built from the tested commit;
  failed artifacts are not silently replaced under the same version.
