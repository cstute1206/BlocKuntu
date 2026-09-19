# Test implementation order

This roadmap introduces fast feedback first and adds expensive, stateful and destructive coverage only after the underlying fixtures are reliable.

## Phase 0: Normalize the test specification and fixtures

Repository-side status and commands: [phase-0.md](phase-0.md).

Create reusable fixtures before adding automation:

1. Assign stable IDs to all cases in the four layer documents.
2. Create a generated policy fixture whose object IDs include the test-run ID. Imports append unique objects, so fixed IDs must not be reused in a dirty VM.
3. Create a deterministic local HTTP test site with separate domain, exact-URL, URL-prefix, URL-contains, and path-prefix routes.
4. Create two harmless test executables with distinct process identities. One fixture must be able to start a differently identified child/helper process.
5. Define a result directory containing a machine-readable result file, logs, screenshots and package metadata.
6. Define clean VM templates for Ubuntu, Fedora, and CachyOS. Verify SSH,
   identities, fixtures and the desktop on independent disposable copies.
   The initial harness uses full disk/NVRAM copies; libvirt snapshots are optional.

Exit criterion: fixtures can be started and stopped repeatedly without BlocKuntu, and each produces a deterministic identity or URL.

## Phase 1: Improve pull-request CI

Repository implementation and commands: [phase-1.md](phase-1.md).

Implement [Layer 1](layers/01-pr-ci.md):

1. Split the current workflow into independent Rust, GUI, Firefox-extension,
  and Chrome-extension jobs so they can run concurrently.
2. Run Cargo commands with `--locked` and add Clippy for all targets.
3. Retain Svelte checking and production builds.
4. Add unit tests for pure browser-extension decisions, especially URL matching,
  heartbeat health transitions, blocked-page selection, and allowlist wording.
5. Extend Rust tests for every allowlist invariant listed in Layer 1.
6. Publish test reports when a job fails.

Exit criterion: policy regressions, including website and process-level  
application allowlists, fail before a package is built.

## Phase 2: Build and inspect packages

Implement [Layer 2](layers/02-package-ci.md):

1. Build the Debian package on Ubuntu.
2. Build the RPM in its supported build environment.
3. Build the Arch package with the existing Arch container workflow.
4. Inspect package metadata, paths, permissions, systemd units, maintainer
  scripts, Native Messaging manifests, version consistency, and checksums.
5. Upload packages and inspection reports as workflow artifacts.

Run this layer on `main`, version tags, and manual dispatch. It need not delay  
every pull request once Phase 1 provides fast source feedback.

Exit criterion: each declared artifact can be functionally rebuilt from the tested commit  
and passes its static package assertions.

## Phase 3: Automate one Ubuntu VM smoke path

Start with one environment rather than the complete browser matrix:

1. Keep the automation runner on the virtualization host, not inside the VM
  that will be reverted.
2. Revert or clone the Ubuntu clean snapshot for each suite.
3. Copy the exact Debian artifact and test fixtures into the guest.
4. Install BlocKuntu, add the desktop user to the `blockuntu` group, and start a
  fresh login session or reboot.
5. Verify the systemd services, daemon RPC, GUI startup, policy import/export,
  one Firefox-family browser, one Chromium-family browser, website blocking,  
   application blocking, and both allowlist types.
6. Collect results before discarding the clone.

Initially expose this as a local command such as:

```bash
Testing/host/run-vm-suite.sh \
  --vm ubuntu-clean \
  --package target/debian/blockuntu_VERSION_amd64.deb \
  --suite smoke
```

Exit criterion: a clean Ubuntu clone can run the core cases in  
[Layer 3](layers/03-vm-acceptance.md) repeatedly without manual state cleanup.

## Phase 4: Add Fedora and CachyOS

Reuse the guest test contract while changing only distribution-specific  
operations:

- Ubuntu installs a `.deb` with `apt install`;
- Fedora installs an `.rpm` with `dnf install`;
- CachyOS installs a `.pkg.tar.zst` with `pacman -U`.

Do not treat a source `.tar.gz` as an installed Arch/CachyOS package.

Exit criterion: installation, service health, import/export, one website rule,  
one application rule, and one allowlist case pass on all three distributions.

## Phase 5: Expand the supported-browser matrix

Parameterize Layer 3 by browser and package provenance. Run only one browser of  
a family at a time so browser identity and heartbeat attribution are  
unambiguous.

Use two browser-profile classes:

- onboarding profiles: browsers installed but no BlocKuntu extension, used for  
manual store-install and first-heartbeat acceptance;
- regression profiles: the signed store extension already installed, used for  
automated heartbeat, managed-policy, blocking, restart, and recovery tests.

Unsupported package variants receive explicit negative tests. They must not be  
reported as supported merely because a heartbeat happened to arrive.

Exit criterion: every package variant listed as supported in  
`Docs/supportedBrowsers.md` has a recorded acceptance result.

## Phase 6: Add destructive and long-running suites

Implement [Layer 4](layers/04-destructive-and-soak.md) only on disposable clones:

1. Direct package-manager removal rejection.
2. Hosts-file alteration and deletion recovery.
3. SQLite deletion and policy-snapshot recovery.
4. Clock-tamper fail-closed behavior.
5. Protected-setting restrictions and authorized GUI uninstall.
6. Upgrade from a previously accepted package.
7. Browser outage/recovery tests and the one-hour heartbeat soak.
8. Application-allowlist desktop-safety and recovery-boundary tests.

Exit criterion: destructive cases cannot damage the reusable base snapshots,  
and failure evidence is collected even when the guest desktop becomes unusable.

## Phase 7: Connect VM suites to CI

After the local harness is stable, add a manually triggered and release-gated  
workflow using a dedicated self-hosted runner with libvirt access. Restrict it  
to trusted branches and tags; do not run untrusted pull-request code on the  
virtualization host.

Recommended triggers:

| Trigger               | Suites                                      |
| --------------------- | ------------------------------------------- |
| Pull request          | Layer 1 only                                |
| Push to `main`        | Layers 1 and 2                              |
| Nightly schedule      | Layer 3 smoke plus selected browser cases   |
| Manual dispatch       | Any selected VM, browser, or suite          |
| Release candidate/tag | Layers 1–4, with required acceptance review |

Use a concurrency group of one for the libvirt runner until the harness has  
proven that VM names, networks, snapshots, and result directories are isolated  
per run.
