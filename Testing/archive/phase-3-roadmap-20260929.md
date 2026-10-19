> Historical plan, superseded by the [smaller Phase 3 policy](../phase-3-roadmap.md) on 2026-10-01. Its completion requirements no longer govern routine releases.

# Phase 3 roadmap: Complete Layer 3 acceptance

Updated: 2026-09-29. Status: **P3.0 complete; Ubuntu P3.2 website blocking covered; P3.1 and P3.2 allowlists in progress**.

This roadmap defines the remaining implementation and validation work for
[Layer 3](../layers/03-vm-acceptance.md). The existing command and automation stack
remain documented in [phase-3.md](../phase-3.md); executed results belong in the
[validation report](../reports/phase-3-validation.md). Checkboxes below represent
work still to do, not runtime acceptance evidence.

## Agreed scope and artifact target

- Use the existing immutable Ubuntu, Fedora, and CachyOS templates and their
  graphical sessions. No Debian guest or additional OS versions are required.
- Develop against the exact artifacts from [Package validation run
  36269349435](https://github.com/cstute1206/BlocKuntu/actions/runs/36269349435),
  source commit `605d707b92cc5fa007674169f0e74ce6fc879c33`.
- GitHub API inspection on 2026-09-27 confirmed that this run completed
  successfully and its Ubuntu, Fedora, and Arch artifact bundles were not
  expired. The three bundles have since been downloaded and their reports,
  provenance flags, source commits, and exact package checksums verified;
  workflow/package verification is not VM acceptance.
- Pin that run for development and comparable acceptance runs. If a product
  fix requires a newer artifact, record the replacement run/commit/checksums
  explicitly and rerun affected coverage plus smoke. Do not silently follow a
  moving "latest" artifact or rebuild BlocKuntu inside an acceptance guest.
- Use published Firefox Add-ons and Chrome Web Store extensions. The CI
  extension archives do not substitute for signed store onboarding. Record
  browser/package and store-extension versions independently of package source.
- Cover every documented supported browser/package variant and explicit
  negative cases for unsupported variants, using the matrix below.
- Keep `VM-UPGRADE-001` deferred by the user's 2026-09-27 decision. Never count
  it as passed. Destructive recovery, uninstall, clock tampering, and long soak
  tests remain Layer 4 work.
- The user reports the earlier VPN issue resolved. No VPN investigation or
  recurring VPN confirmation is required. Retain ordinary clone DHCP, SSH,
  reboot-connectivity checks and existing host/network ownership safeguards.

## Current baseline and remaining coverage

The current smoke baseline uses the selected Layer 2 run `36269349435`.
Each guest has two consecutive fresh-clone passes with 21 observations per run,
covering 17 distinct case IDs:
installation `001–005`, data `001–004`, application blocking `001–003`,
application allowlist `004`, and browser `001–002`, website `002`, website
allowlist `004` in each of two browsers. All six runs have identical harness
manifests; package/browser/extension versions match within each guest's pair.
See the validation report for exact evidence and historical artifact runs.

The specification has 77 distinct IDs: 17 implemented smoke IDs, three additional
IDs (`VM-WAL-001`, `VM-WAL-005`, and `VM-AAL-001`) automated only on Ubuntu,
six website IDs (`VM-WEB-001`, `003–007`) now bound to Ubuntu native
Firefox and Chrome, 50 other non-upgrade IDs still planned, and one deferred upgrade ID. Browser
and scenario dimensions increase the
number of required observations beyond these counts. Existing smoke cases
also need extension to cover all applicable variants.

| Remaining area | IDs needing full coverage | Count | Milestones |
| --- | --- | ---: | --- |
| Website blocking | `VM-WEB-001`, `003–008` | 7 | P3.2, P3.4 |
| Activation and precedence | `VM-ACT-001–007` | 7 | P3.3, P3.4 |
| Live edits | `VM-EDIT-001–005` | 5 | P3.3 |
| Application metering | `VM-APP-004–005` | 2 | P3.4 |
| Website allowlists | `VM-WAL-001–003`, `005–013` | 12 | P3.2, P3.4, P3.6 |
| Application allowlists | `VM-AAL-001–003`, `005–021` | 20 | P3.4, P3.5 |
| Browser integration | `VM-BR-003–008` | 6 | P3.6 |
| Upgrade | `VM-UPGRADE-001` | 1 deferred | Outside the gate |

## P3.0 — Verify the new artifacts and establish smoke results

- [x] Download the `package-ubuntu-x86_64-605d707b92cc5fa007674169f0e74ce6fc879c33`,
  `package-fedora-x86_64-605d707b92cc5fa007674169f0e74ce6fc879c33`, and
  `package-arch-x86_64-605d707b92cc5fa007674169f0e74ce6fc879c33` bundles into a
  new ignored `Testing/results/package-ci/github-36269349435/` directory.
- [x] Require passed, provenance-verified reports and exact package hashes;
  record package filenames, versions, checksums, and source commit. Select the
  Ubuntu bundle's DEB for Ubuntu, RPM for Fedora, and binary Arch package for
  CachyOS. The Debian bundle is outside this guest scope.
- [x] Run the existing smoke command on all three guests with those reports.
  Resolve failures using retained evidence; separate harness fixes from
  product fixes, and use a new run ID for each attempt.
- [x] Establish two consecutive fresh-clone smoke passes per guest with the
  same final harness and package/store-extension versions within each pair.
  Preserve failed attempts and record any manual onboarding interactions.

Exit: the selected artifacts have verified provenance and repeatable smoke
evidence on all three guests. Older passes remain historical evidence only.

Verified inputs are recorded in
`results/package-ci/github-36269349435/verified-inputs.json`. Runs
`p30-36269349435-{ubuntu,fedora,cachyos}-{a,b}` each passed 21/21 observations.
The [pair checker report](../results/p30-smoke-pairs.json) verifies all three
comparable pairs. The separate [final host check](../results/p30-final-state.json)
verifies six distinct clone identities, removed copied storage, unchanged bases,
current package hashes, and the frozen harness. Only the three stopped bases
remain. See the [validation report](../reports/phase-3-validation.md).

## P3.1 — Make the harness support the complete case contract

Depends on P3.0. Extend the existing Python/libvirt/SSH/AT-SPI stack.

The [case contract and planner](../layer3/README.md) now feed the smoke runner's
preflight and result gate. Its selected-case mode projects a complete smoke
execution for diagnosis. Core, browser-matrix, and full requests with missing
bindings are rejected before a VM is created. New scenario execution, isolation,
and browser adapters remain pending. The modified runner passed one
[fresh smoke regression on each guest](../results/p31-runtime-regression.json),
21/21 observations each, with one current harness and all clones removed.
Subsequent P3.1 changes add six evidence-backed native smoke browser-cell
layouts, scenario-derived harness budgets, and failure-dependent execution
guards. The [newer three-guest regression](../results/p31b-runtime-regression.json)
passed 21/21 on each guest with identical current harness files and removed clones.
The [serialized smoke-group runner](../layer3/README.md#serialized-smoke-groups)
now preflights every selected artifact, executes one fresh-clone group at a time,
and fails closed on missing result or clone-removal evidence. The
[P3.1 group audit](../results/p31cd-runtime-regression.json) verifies 63/63
observations across the latest successful runs, with one earlier CachyOS
Chrome Web Store consent HTTP 502 attempt retained as a failure. Group
execution beyond the smoke subset remains open.

The next P3.1 batch added atomic process checkpoints, a strict guest-to-host
UI action protocol, bounded planning budgets for implemented isolated groups,
and a reusable live group audit. The [batched-harness audit](../results/p31ef-group-audit.json)
passed 63/63 latest smoke observations after one three-guest regression. The
first CachyOS attempt again encountered a Google consent HTTP 502 during
published-store onboarding; its failure remains recorded, and one fresh retry
passed. These are smoke results, not core/browser-matrix/full acceptance.

The first isolated non-smoke group now executes `VM-WAL-001` on a fresh Ubuntu
clone using the exact pinned DEB. The [Ubuntu group result](../results/p31wal-36269349435-a-group-run/summary.json)
passed its one required observation: ordinary-user daemon RPC rejected a Tier 1
website allowlist, the policy remained unchanged, and enforcement stayed active.
The bundle verifies package provenance, Phase 0 identity, base integrity, and
clone removal. A live libvirt check found only the three templates. This is a
selected case result, not a complete core suite. Fedora, CachyOS, other
scenarios, and 36 unresolved browser cells remain open.

The [next serialized Ubuntu batch](../results/p31more-36269349435-a-group-run/summary.json)
passed two more selected cases on separate fresh clones. `VM-WAL-005` activated
a Tier 2 website allowlist through Detox, confirmed the outside URL was rejected
for that allowlist while the listed URL was allowed, and found the managed
`/etc/hosts` block unchanged and compliant. `VM-AAL-001` confirmed that an
ordinary-user RPC cannot save a Tier 1 application allowlist and that rejection
leaves the policy unchanged. Both exact-artifact bundles passed provenance,
Phase 0 identity, base-integrity, observation, and clone-removal checks. The
live inventory again contained only the three stopped templates. These are
two selected Ubuntu observations, not a complete core-suite pass.
The existing `VM-WAL-001` assertion also passed a [fresh Ubuntu regression](../results/p31wal-current-36269349435-group-run/summary.json)
with the same current harness as the two-case batch. All three isolated
Ubuntu bindings therefore have current-harness evidence, still short of
cross-distribution or complete-suite acceptance.

- [x] Add a machine-readable case registry, for example
  `Testing/layer3/cases.json`, containing every specification ID, implementation
  status, suite, dependencies, isolation requirements, applicable guest/browser
  cells, scenario variants, deadlines, and required evidence.
- [ ] Complete the browser matrix configuration, for example
  `Testing/config/layer3-browsers.json`, with source/version, install method,
  executable, profile, policy, and Native Messaging paths per selected cell.
  The 42 cells are inventoried and six native smoke cells have observed layouts
  [cross-checked against package archives](../results/p31-browser-cells.json);
  36 cells remain explicitly unresolved.
- [ ] Add suite/case selection to `scripts/vm-acceptance.py`: retain `smoke`,
  add `core`, `browser-matrix`, and `full`. Case selection is for diagnosis;
  a selected subset must not report full acceptance. These interfaces are
  exposed in the CLI; unimplemented selections fail before cloning. Case
  selection usually reports a diagnostic projection of full smoke execution;
  `--suite core --guest ubuntu --case VM-WAL-001` executes one isolated
  non-smoke observation; the serialized group runner can batch all three
  implemented Ubuntu selections, each in its own clone.
- [x] Replace the hard-coded smoke result gate in `fixtures/layer3/guest.py`
  with required observations generated from the registry. Key results by case,
  guest, browser package, and scenario, not just case ID/browser family.
- [x] Separate implementation status (`planned`, `automated`, `manual`,
  `blocked`) from execution outcomes. Missing, skipped, blocked, duplicate, or
  contradictory required observations cannot satisfy a pass. Preserve setup
  failures even if downstream observations happen to pass.
- [ ] Isolate scenario groups with fresh clones or verified disposable
  checkpoints. Define setup/teardown and dependency failure handling; do not
  continue dependent cases after a failed setup. Avoid accumulated Tier 1
  rules, active Detox, exhausted counters, and hourly unlock quotas leaking
  between independent scenarios. `--prepared` is not a dirty-VM resume option.
  The current smoke sequence uses one fresh clone and now stops dependent
  cases after a failed service, GUI, browser, or process prerequisite. The
  [read-only group planner](../layer3/README.md) assigns separate fresh-clone
  boundaries to new observations. The serialized group runner executes whole
  smoke groups and three isolated Ubuntu case groups on fresh clones;
  additional scenario executors and safe sharing rules remain to be implemented.
- [ ] Use normal product operations for policy setup, and unprivileged desktop
  user RPC for authorization assertions. Root supervision must not accidentally
  turn a rejected ordinary-user operation into a successful privileged action.
- [ ] Replace the fixed 30-minute guest-suite and five-minute process-supervisor
  limits with bounded group budgets derived from the planned scenarios. Keep
  per-observation deadlines unchanged; do not reset clocks/counters, shorten
  product timers, or hide failures with automatic retries.
  The current smoke group now derives limits from 17 bounded baseline scenarios;
  implemented observations in non-runnable plans have bounded planning
  budgets. The executable non-smoke groups have 390- or 420-second setup,
  observation, and teardown budgets.
- [ ] Extend evidence collection to all cases and browser adapters, including
  incremental process evidence if the desktop or guest suite fails. Retain
  redaction and private evidence permissions. Keep the host runner serialized
  until independent concurrent VM execution has been validated.
  The collector now derives browser failure-dump names from the plan, including
  Chromium. The process supervisor checkpoints completed cases atomically, and
  the guest retains partial process rows if completion never arrives. The
  smoke group runner records each group before/after execution, retains logs,
  and stops on failed or missing evidence. Broader evidence collection is
  still required for new cases.
- [ ] Add focused harness tests for registry completeness, matrix dimensions,
  result aggregation, missing/dependency failures, clone ownership, artifact
  provenance, and the restricted host-action protocol. The latter now rejects
  malformed actions and reused IDs with changed content; the saved group audit
  checks latest-attempt status, serial timing, clone removal, and live inventory.

Exit: the existing smoke suite still passes; all 77 IDs are accounted for and
the runner distinguishes partial execution from complete acceptance.

## P3.2 — Finish website matching and strict website allowlists

New IDs: `VM-WEB-001`, `VM-WEB-003–007`, `VM-WAL-001–003`, `VM-WAL-005–008`.
Retain `VM-WEB-002` and `VM-WAL-004` as regressions.

- [ ] Expand fixture hosts/routes and the restricted native URL-entry adapter
  for subdomains, near misses, query tokens, normalization, and SPA navigation.
  Test accepted fixture input and rejected external/arbitrary input.
- [ ] Provide deterministic guest-local DNS/resolution and a test endpoint
  whose successful response can be distinguished from a hosts-blocked address.
  Avoid conflicting loopback entries outside BlocKuntu's managed hosts block.
  Prove baseline resolution/access, managed entry transitions, and recovery.
- [ ] Test all matcher positives and negatives, already-open tab revalidation,
  and actual `history.pushState` navigation using the fixture's UI control.
- [ ] Test unsupported Tier 1 allowlists, inactive/active Tier 2 schedules,
  allowlist intersections, blocklist precedence, and the absence of catch-all
  hosts entries for allowlists.
- [ ] Require the expected visible BlocKuntu page and rule/reason for extension
  blocking; a network error or closed browser is not sufficient. Keep hosts
  enforcement evidence distinct from extension blocked-page assertions.

Exit: these cases pass in native Firefox and Chrome on Ubuntu/Fedora, and
native Firefox and Chromium on CachyOS, with browser-specific results.

Ubuntu native-browser matcher checkpoint: the [selected group](../results/p32web-36269349435-c-group-run/summary.json)
passed `VM-WEB-003–007` in Firefox and Chrome (10/10 dimensioned observations)
with all 21 smoke prerequisites passing. It used the pinned DEB, published
store extensions, a fresh clone, browser-specific fixture routes/rules,
visible blocked-page and rule-name checks, screenshots, bounded case times,
base-integrity verification, and guarded clone removal. The [first attempt](../results/p32web-36269349435-a-group-run/summary.json)
retains a Firefox SPA control no-op (9/10), and the [second attempt](../results/p32web-36269349435-b-group-run/summary.json)
retains a Chrome store-tab onboarding failure after Firefox passed 5/5. The
input adapters were corrected before the passing fresh attempt; neither
failed run counts toward acceptance. All three copied clones were removed,
the immutable bases were rechecked, and live libvirt inventory contains only
the three stopped templates.

The separate [Ubuntu domain group](../results/p32domain-36269349435-a-group-run/summary.json)
passed `VM-WEB-001` in Firefox and Chrome (2/2 observations), with 21/21
smoke prerequisites. Guest-local `dnsmasq` supplied baseline fixture
resolution and forwarded public DNS to the clone's original upstream;
BlocKuntu's managed hosts entries changed the target domain and `www`
subdomain to `0.0.0.0` while the near miss remained at `127.0.0.1`. Both
browsers showed the expected blocked page and rule name for positives, and
accessed the near miss. The [DNS checkpoint](../results/p32domain-36269349435-a-ubuntu/ubuntu/acceptance/domain-dns.json),
package, timing, screenshots, base integrity, and clone removal verified.
The Ubuntu website-blocking IDs `VM-WEB-001–007` now have native browser
evidence (`VM-WEB-002` is the retained smoke regression).

P3.2 is still open on Ubuntu for website allowlists: `VM-WAL-002–003` need
inactive/active schedule windows; `VM-WAL-006–008` need open-tab allowlist
revalidation, list intersection, and blocklist precedence. `VM-WAL-001` and
`VM-WAL-005` have isolated Ubuntu passes, while `VM-WAL-004` remains a smoke
regression. Fedora and CachyOS P3.2 browser execution, normalization variants,
and the complete core-suite gate remain pending.

## P3.3 — Activation boundaries, live edits, and reboot enforcement

New IDs: `VM-ACT-001–004`, `VM-ACT-006`, `VM-EDIT-001–005`.

- [ ] Exercise the before/during/after schedule and natural Detox boundaries
  for both websites and applications. Record guest timezone/wall time and
  monotonic observations; use the specified 30-second baseline and three-minute
  windows. Relaunch killed fixtures and navigate again to prove restored access.
- [ ] Prove Tier 1 activation without a schedule, Tier 2's rejected unlock,
  correct managed hosts transitions, and overlapping-tier precedence.
- [ ] Drive stricter and weakening edits through the GUI and verify daemon
  rejection independently as the desktop user. Cover website/application,
  blocklist/allowlist, and all protected fields listed in `VM-EDIT-005`.
  Check configuration remains unchanged after rejected operations.
- [ ] Strengthen `VM-INSTALL-005`: create active rules before reboot and verify
  actual browser/process enforcement afterward, alongside GUI/RPC recovery.
- [ ] Audit existing installation and import/export assertions against the
  full expected outcomes; extend policy fixtures to application lists as well
  as website lists where needed. An old case ID is not proof of every variant.

Exit: activation, edit permissions, and installed-policy persistence pass on
all three guests using real user-visible and process observations.

## P3.4 — Allowance accounting and manual unlocks

New IDs: `VM-WEB-008`, `VM-APP-004–005`, `VM-ACT-005`, `VM-ACT-007`,
`VM-WAL-009–012`, `VM-AAL-010–016`.

- [ ] Add a harmless graphical application fixture with a distinct process
  identity and observable foreground/background/minimized states. Keep the C
  fixtures for process/helper tests; a headless process cannot prove minimization.
- [ ] Begin each independent usage variant with zero recorded usage. Prove
  access before exhaustion, positive usage within 30 seconds, and exhaustion
  within 120 seconds, followed by the specified enforcement deadline.
- [ ] Prove listed-only activity does not consume allowlist allowance, rejected
  activity does, and two concurrent rejected processes charge elapsed time
  once per rule. Test all three application window states separately.
- [ ] Exercise strictest-allowance precedence and both rule-ID and rejected
  identity unlock resolution. Record ordinary reason/quota preconditions,
  current usage, grant expiry, and competing restrictions.
- [ ] Prove the fixed 120-second grant, access during the grant, and reblocking
  after expiry without refilling allowance. Cover schedule activation and
  targeted five-minute Detox, so Detox remains active beyond expiry.
- [ ] Prove another rejecting rule still wins while one rule is unlocked.
  Use isolated state for independent grants rather than waiting away or
  modifying the global hourly quota.

Exit: accounting and unlock cases pass on all guests; website observations are
recorded in both core browsers. Counter changes alone never prove enforcement.

## P3.5 — Process identity boundaries and desktop safety

New IDs: `VM-AAL-001–003`, `VM-AAL-005–009`, `VM-AAL-017–021`.
Retain `VM-AAL-004` and P3.4 metering cases as regressions.

- [ ] Implement inactive/scheduled allowlists, rejected Tier 1 combinations,
  parent/helper independence, intersections, and blocklist precedence.
- [ ] Exercise the real GUI's Add all detected action, then launch a new helper
  to prove it was a point-in-time snapshot. Test stricter removal and rejected
  addition to an active list.
- [ ] Record PID, start time, owner, executable, and matcher for each target;
  verify original-process termination and continued allowed-process liveness.
- [ ] Test browser main/helper identities in separate disposable scenarios.
  A killed browser helper is process-case evidence, not a website-blocking pass.
- [ ] Preserve evidence through the root supervisor and SSH before running
  restrictive desktop scenarios. Verify BlocKuntu components/descendants,
  PID 1, root services, and non-desktop system accounts as specified.
- [ ] Build explicit GNOME/KDE safety fixtures for D-Bus, portals/authentication,
  audio, clipboard, terminal children, IDE language servers, and browser helpers.
  Exercise representative functions during enforcement. Allowing every
  currently detected executable does not establish curated desktop safety.

Exit: all 21 application-allowlist IDs have their required scenario results on
each guest, with evidence collectable even when a desktop scenario fails.

## P3.6 — Browser protection and package matrix

New IDs: `VM-BR-003–008`, `VM-WAL-013`. Extend existing browser/web cases to
the selected packages; browser-family reuse supplies code, not passing results.

### Planned positive matrix

`Required` is a target to implement and validate, not a claim that an installer
or managed-policy layout has already been verified. For each cell, first
identify a reproducible package source/layout and record exact provenance.
Unavailable prerequisites remain blocked and prevent completion; do not
silently drop a cell or replace its package type.

| Browser/package variant | Ubuntu | Fedora | CachyOS |
| --- | --- | --- | --- |
| Firefox native | Required | Required | Required |
| Firefox Snap | Required | Outside matrix | Outside matrix |
| Firefox Flatpak | Required | Required | Required |
| LibreWolf native layout | Required | Required | Required |
| Waterfox native layout | Required | Required | Required |
| Google Chrome native | Required | Required | Outside matrix |
| Chromium native | Outside matrix | Required | Required |
| Chromium Snap | Required | Outside matrix | Outside matrix |
| Brave native | Required | Required | Required |
| Opera native | Required | Required | Required |
| Microsoft Edge native | Required | Required | Required |
| Vivaldi native | Required | Required | Required |

This is 30 positive guest/package cells covering all 12 documented variants.
Snap coverage is assigned to Ubuntu; Chrome covers Ubuntu/Fedora and native
Chromium covers Fedora/CachyOS. Outside-matrix combinations receive no runtime
support claim from this milestone. Changes to required cells must be recorded
explicitly before acceptance, not inferred from a missing installer.

### Planned negative matrix

| Unsupported variant | Required guest(s) | Case |
| --- | --- | --- |
| Chromium Flatpak | Ubuntu, Fedora, CachyOS | `VM-BR-008` |
| Brave Snap | Ubuntu | `VM-BR-008` |
| Opera Snap | Ubuntu | `VM-BR-008` |
| Vivaldi Snap | Ubuntu | `VM-BR-008` |
| Vivaldi Flatpak | Ubuntu, Fedora, CachyOS | `VM-BR-008` |
| Unknown Firefox-family layout fixture | Ubuntu, Fedora, CachyOS | `VM-BR-008` |

- [ ] Implement package-specific browser adapters with dedicated profiles,
  one browser of a family running at a time, and verified package/policy paths.
- [ ] Use fresh profiles for store onboarding and distinct store-installed
  regression profiles. Record prompts versus managed installation and any
  operator interaction. Store outages block onboarding, not local policy tests.
- [ ] Prove disable/remove protection, new-session startup grace and fresh
  heartbeat, and continued enforcement when only the BlocKuntu GUI is closed.
- [ ] Cover supported Firefox private-browsing behavior and each configured
  Chromium incognito strategy, including extension permission where required.
  Apply the expected setting-specific result rather than assuming all private
  windows are protected identically.
- [ ] Prove confined per-user Native Messaging and managed policies after
  documented setup/restart. An unrestricted native-browser pass cannot cover
  the confined package's result.
- [ ] Run `VM-BR-001–006` and browser-facing website/allowlist/activation
  observations on every positive cell; add `VM-BR-007` on confined cells.
  Reuse common daemon/process checks per guest, but record browser-specific
  helper scenarios where identities differ. Negative cells run `VM-BR-008`.
- [ ] Keep unsupported status explicit even if an extension heartbeat arrives.
  Verify configured unsupported-browser handling with an ordinary allowed
  browser as a control; fixture installation failure is not a negative pass.
- [ ] Detect browser/extension version changes during a suite and repeat
  affected observations under a consistent version set.

Exit: every required matrix observation has explicit passing evidence, and
unsupported-package expectations pass without being presented as support.

## P3.7 — Consolidate full acceptance and handoff

- [ ] Generate JSON plus a readable coverage report from the registry and
  execution results, mapping all 77 IDs and every required scenario/matrix cell.
- [ ] Record package and harness commits/hashes, guest base identity, OS/kernel,
  desktop/session, wall time/timezone, browser provenance/version, extension
  version, policy paths, timings, and evidence links. Retain per-case logs,
  screenshots, policy exports, and process identities as appropriate.
- [ ] Require two consecutive clean passes for each required configuration
  using the same final relevant harness and package/browser/extension versions
  within the pair. Independent scenario groups may use separate clones; do
  not require one unsafe, state-contaminated all-in-one run.
- [ ] Ensure selected-case development runs cannot fill unrelated missing
  observations, and diagnostic retries do not erase original failures.
- [ ] Verify evidence capture, stopped/removed clone handling, and immutable
  base integrity. Mark cleanup/integrity failure explicitly.
- [ ] Update `phase-3.md`, the Layer 3 specification, and the validation report
  with actual commands, status, limitations, and exact accepted artifacts.

Exit: all 76 non-upgrade IDs and their applicable required observations pass;
`VM-UPGRADE-001` is the sole agreed deferral. Report the result as **Layer 3
accepted for the recorded guest/browser matrix, with upgrades deferred**.
No planned/blocked required case is hidden behind an overall green result.

## Execution order and boundaries

Implement P3.0, then P3.1–P3.7 in order. Develop each core group on Ubuntu,
then validate it on Fedora and CachyOS before closing that milestone. Preserve
the smoke command as the quick regression path throughout. Start matrix
source/provenance discovery during P3.1; full browser runs follow core stability.

The immediate next action is to extend the working isolated executor pattern
to more core scenarios and both remaining guests, while discovering verified
browser adapter layouts for the 36 unresolved cells. Planning budgets and
three isolated Ubuntu scenario passes do not make the remaining selections executable.
Preserve the verified P3.0 baseline from run `36269349435`; its previous
passing evidence does not validate changed code, so the new three-guest smoke
regression is recorded separately.

Local execution is the Phase 3 deliverable. Dedicated self-hosted CI scheduling
can follow after the local full gate is stable, under the separate CI phase in
[implementation-order.md](../implementation-order.md). No CI deployment or
additional user decision is needed to begin the agreed local work.
