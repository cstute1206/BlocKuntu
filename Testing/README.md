# BlocKuntu testing

This directory separates the original test ideas from the executable test plan.
`createTests.md` remains the initial checklist. The documents below define the
implementation order, the test layers, and the expected results for each case.

## Documents

| Document | Purpose |
| --- | --- |
| [createTests.md](createTests.md) | Original test ideas and product scenarios |
| [implementation-order.md](implementation-order.md) | Order in which the test infrastructure should be implemented |
| [phase-0.md](phase-0.md) | Implemented fixture, run-directory, and VM-template contract |
| [reports/phase-0-acceptance.md](reports/phase-0-acceptance.md) | Executed Phase 0 results and outstanding acceptance issue |
| [phase-1.md](phase-1.md) | Implemented PR jobs, local commands, reports, and case-to-test mapping |
| [layers/01-pr-ci.md](layers/01-pr-ci.md) | Fast source, policy, GUI, and extension checks for every pull request |
| [phase-2.md](phase-2.md) | Package build/inspection implementation, commands, and limitations |
| [layers/02-package-ci.md](layers/02-package-ci.md) | Package builds and static artifact inspection |
| [phase-3.md](phase-3.md) | Installed-package smoke runner, automation stack, commands and current boundaries |
| [phase-3-roadmap.md](phase-3-roadmap.md) | Small release gate, feature coverage, Ubuntu priorities and change-triggered tests |
| [layer3/README.md](layer3/README.md) | Machine-readable case contract, browser matrix, coverage planner, and serialized smoke groups |
| [layers/03-vm-acceptance.md](layers/03-vm-acceptance.md) | Installed-package, GUI, browser, blocking, and allowlist acceptance |
| [phase-4.md](phase-4.md) | Ubuntu acceptance and Fedora/CachyOS native adapters for uninstall, hosts and browser recovery, destructive-suite plan, existing Tier 1 evidence, safety guards, operator inputs, and acceptance criteria |
| [reports/phase-4-native-ports.md](reports/phase-4-native-ports.md) | Native validation: Fedora/CachyOS P4.3 qualified; Fedora P4.2 qualified; CachyOS P4.2 schedule-assertion failure and native uninstall defects documented |
| [reports/phase-4-native-uninstall-fixes.md](reports/phase-4-native-uninstall-fixes.md) | Fedora/Pacman transaction guards, corrected schedule timing evidence, candidate build and fresh validation |
| [layers/04-destructive-and-soak.md](layers/04-destructive-and-soak.md) | Hardening, recovery, uninstall, failure, and long-duration tests |

## Execution policy

Run fast source checks for every code change; focused integration for relevant
changes; package validation and three-guest smoke for releases. Broader cases
are selected by affected feature or release-candidate risk. See the
[Phase 3 roadmap](phase-3-roadmap.md). Case catalogs are not mandatory full matrices.

Run all local harness regressions without starting VMs:

```bash
python3 Testing/scripts/pr-ci.py harness
```

The same command runs in PR CI. Required fixture tools and the evidence boundary
are documented in [Phase 1](phase-1.md).

## Original checklist coverage

| Original item | Primary replacement cases |
| --- | --- |
| 1. Install packages | `PKG-BUILD-*`, `VM-INSTALL-*` |
| 2. Install extensions in supported browsers | `VM-BR-*` and the supported browser-package matrix |
| 3. Append policy TOML | `PR-DATA-*`, `VM-DATA-001` through `VM-DATA-003` |
| 4. Browser policy, disable protection, and private mode | `VM-BR-002` through `VM-BR-004` |
| 5. Website matcher types | `PR-POL-001`, `PR-POL-002`, `VM-WEB-001` through `VM-WEB-007` |
| 6. Website daily allowance | `PR-POL-008`, `VM-WEB-008` |
| 7. Application daily allowance | `PR-POL-009`, `VM-APP-004`, `VM-APP-005` |
| 8. Detox | `PR-POL-005`, `VM-ACT-002` |
| 9. Schedule transitions | `PR-POL-004`, `VM-ACT-001` |
| 10. Application blocking | `VM-APP-001` through `VM-APP-003` |
| 11. Edit an active website list | `PR-EDIT-*`, `VM-EDIT-*` |
| 12. Edit an active application list | `PR-EDIT-*`, `VM-EDIT-*` |
| 13. Overlapping rules | `PR-POL-006`, `PR-POL-007`, `VM-ACT-006`, `VM-ACT-007` |
| 14. Tier 1, Tier 2, and Tier 3 | `VM-ACT-003` through `VM-ACT-005` |
| 15. Uninstall with recovery credential | `DES-UNINSTALL-*` |
| 16. Tier 1 edit key and expiry | `DES-PROTECT-004` through `DES-PROTECT-007` |
| 17. Policy export | `PR-DATA-001`, `VM-DATA-004` |
| 18. Package update | `VM-UPGRADE-001`, `DES-UPGRADE-*` |
| 19. Hardening | `DES-UNINSTALL-*`, `DES-HOSTS-*`, `DES-DATA-*`, `DES-CLOCK-*` |
| 20. Protected setting access | `DES-PROTECT-001` through `DES-PROTECT-003` |
| 21. One-hour YouTube heartbeat | `SOAK-002` |

Additional allowlist coverage is defined under `PR-WAL-*`, `PR-AAL-*`,
`VM-WAL-*`, `VM-AAL-*`, `PKG-ALLOW-*`, and `DES-AAL-*`.

## Layer boundary

Passing a lower layer does not imply that a higher layer passed:

1. PR CI proves source-level behavior and that the components compile.
2. Package CI proves that distributable artifacts can be built and contain the
   expected files.
3. VM acceptance proves that an exact artifact works after installation in a
   supported desktop environment.
4. Destructive and soak tests prove recovery and longer-running runtime
   behavior in disposable VMs.

## Test status

The cases in these documents are specifications. A case is not considered
automated until a script or test implementation records its result. Use these
statuses when tracking implementation:

- `planned`: documented but not implemented;
- `automated`: runs without interactive steps;
- `manual`: requires an operator and has a repeatable procedure;
- `blocked`: cannot run because a fixture or supported environment is missing.

## Result evidence

Every VM result should record:

- Git commit and package filename;
- package SHA-256 checksum;
- base VM and snapshot name;
- distribution, desktop session, and kernel;
- browser name, package provenance, and version when applicable;
- pass, fail, or skipped result for every test ID;
- relevant BlocKuntu export, `journalctl` output, browser logs, and screenshots;
- the reason for every skipped case.

Do not use source-test results as evidence that an installed package or browser
combination passed.
