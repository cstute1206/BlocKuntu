# Phase 2 validation

Status as of 2026-09-19: implementation and local validation are complete.
Ubuntu 24.04, Debian 13, Fedora 43, Arch, and both extension archives passed.
All builds use uncommitted development inputs and explicitly report
`build_provenance_verified: false`. GitHub Actions has not been executed for
these changes. No installed-package or VM acceptance is claimed.

| Check | Result | Evidence under `Testing/results/package-ci/` |
| --- | --- | --- |
| Inspector regression tests | PASS: 18 tests | `python3 Testing/scripts/test_package_ci.py` |
| Ubuntu 24.04 DEB build and inspection | PASS | `deb-1789738699173702988/` |
| Latest inspector against Ubuntu artifact | PASS | `deb-1789809998458061137/` |
| Arch Docker build and inspection | PASS | `arch-1789740095668329025/` |
| Latest inspector against Arch artifact | PASS | `arch-1789809998475767191/` |
| Firefox archive | PASS | `firefox-1789738680669873939/` |
| Chrome archive | PASS | `chrome-1789738680662460699/` |
| Fedora 43 RPM build and inspection | PASS | `fedora43-20260919/` |
| Debian 13 DEB build and inspection | PASS | `debian13-20260919/` |

The successful Fedora and Debian rerun sources and logs are retained under
`target/package-validation-1789809962766192644/`, rather than `/tmp`, so they
survive another interruption. Final reports/artifacts have been copied into
`Testing/results/package-ci/`. All retained artifact checksums were verified.

## Network findings

Default Docker networking failed to resolve distribution repository hostnames.
Even after the user disabled the VPN, containers inherited repeated `10.128.0.1`
resolver entries; host resolution worked. A disposable container using host
networking resolved the same hostname successfully. Local builds therefore use
`--network host` (Arch runner: `--docker-network host`). No host DNS, firewall,
or VPN configuration was changed. CI continues to use its default network.

## Packaging corrections discovered during validation

- RPM and Arch package the current README instead of removed installation and
  uninstall documents.
- RPM and Arch include the Chromium helper, wrapper, restricted bridge token
  creation, and daemon bridge flag.
- Arch's authorized removal hook includes recovery-file immutability cleanup.
- Debian includes its MIT license in the package.
- RPM build-id symlinks are inspected safely and restricted to packaged binaries.

Upgrade cases remain explicitly skipped for the first release. Binary-based
schema/allowlist behavior assertions are retired. The known Debian upgrade-hook
concern remains deferred, as agreed.
