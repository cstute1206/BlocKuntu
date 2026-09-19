# Layer 3: Installed-package VM acceptance

## Purpose

This layer validates the exact distributable package in a graphical VM. It
covers systemd, the daemon socket, GUI-to-daemon RPC, browser Native Messaging,
managed policy, real process termination, and the effective policy seen by a
user.

## VM and package matrix

| Guest | Package installation |
| --- | --- |
| Ubuntu | `sudo apt install ./blockuntu_VERSION_ARCH.deb` |
| Fedora | `sudo dnf install ./blockuntu-VERSION-RELEASE.ARCH.rpm` |
| CachyOS | `sudo pacman -U ./blockuntu-VERSION-RELEASE-ARCH.pkg.tar.zst` |

Run each suite from a named clean snapshot or a new disposable clone. After
installation, add the desktop test user to the `blockuntu` group and start a new
login session or reboot before testing GUI access.

## Required fixtures

- A local HTTP server with deterministic hostnames and routes for all website
  matcher types. Public websites are not used for policy assertions.
- Two harmless long-running executables, `blockuntu-test-app-a` and
  `blockuntu-test-app-b`, with distinct identities.
- A parent fixture that can spawn `blockuntu-test-helper` as a differently
  identified child process.
- A generated policy TOML whose IDs contain the test-run ID.
- Browser profiles dedicated to the suite. Never use a person's normal profile.

## Installation and desktop cases

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-INSTALL-001 | Install the native package for the guest distribution. | The package manager exits successfully and reports the expected installed version. |
| VM-INSTALL-002 | Inspect the socket, daemon, watchdog, and hosts-repair units after installation. | Required units are enabled or active according to their unit type; no startup loop or fatal journal error exists. |
| VM-INSTALL-003 | Log in as the configured desktop user and open BlocKuntu from its desktop entry. | The GUI opens, reaches the daemon, and reports healthy core enforcement. |
| VM-INSTALL-004 | Open the first-run recovery modal. | Installation ID and locally generated recovery credentials are presented according to the product workflow and have restricted filesystem permissions. |
| VM-INSTALL-005 | Reboot the guest and log in again. | Enforcement and GUI connectivity recover without a manual daemon start. |

## Policy import, export, and live-edit cases

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-DATA-001 | Import the generated policy through the GUI. | New objects append, the GUI displays them, and the daemon uses them. |
| VM-DATA-002 | Import the same policy again. | Identical IDs do not create duplicates. |
| VM-DATA-003 | Import a changed object under an existing ID. | The conflict is rejected without partially modifying the current policy. |
| VM-DATA-004 | Export policy TOML through the GUI and parse the saved file. | The file is valid, contains the expected blocklists and allowlists, and omits protected service state. |
| VM-EDIT-001 | Append a blocked website or application identity to an active blocklist. | The stricter change succeeds and the newly listed target becomes blocked. |
| VM-EDIT-002 | Attempt to remove a blocked entry from an active blocklist. | The weakening change is rejected. |
| VM-EDIT-003 | Remove an allowed website or process identity from an active allowlist. | The stricter change succeeds and that target becomes rejected. |
| VM-EDIT-004 | Attempt to add an allowed target to an active allowlist. | The weakening change is rejected. |
| VM-EDIT-005 | Attempt to change mode, tier, schedules, allowance, enabled state, or other protected settings of an active list. | The GUI and daemon reject the change. |

## Website blocklist cases

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-WEB-001 | Navigate to the configured domain and a covered subdomain, then to a near-miss domain. | Intended domains are blocked and the near miss is allowed. |
| VM-WEB-002 | Navigate to exact-URL positive and negative paths. | Only the exact normalized URL is blocked. |
| VM-WEB-003 | Navigate to URL-prefix positive and near-miss URLs. | Only URLs under the configured prefix are blocked. |
| VM-WEB-004 | Navigate to URLs with and without the configured URL-containing token. | Only the containing URLs are blocked. |
| VM-WEB-005 | Navigate to path-prefix positive and near-miss paths. | Only paths within the configured prefix are blocked. |
| VM-WEB-006 | Keep a matching page open, activate its rule, and wait for tab revalidation. | The already-open HTTP(S) tab is replaced by the BlocKuntu blocked page. |
| VM-WEB-007 | Perform an in-page history navigation to a newly matching URL. | The SPA-style navigation is re-evaluated and blocked. |
| VM-WEB-008 | Activate a one-minute Controlled Access allowance and keep the page active with visit heartbeats. | Access works before exhaustion and is blocked after confirmed usage reaches the allowance. |

## Application blocklist cases

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-APP-001 | Start an application before activating its matching rule. | The running matching process is terminated on a subsequent daemon scan. |
| VM-APP-002 | Start the same application while its matching rule is already active. | It may start, but it is detected and terminated within the documented scan interval. |
| VM-APP-003 | Start a near-miss process whose identity resembles but does not equal the matcher. | It remains running. |
| VM-APP-004 | Run a one-minute Controlled Access application block. | The process remains before exhaustion, consumes eligible runtime, and is terminated after exhaustion. |
| VM-APP-005 | Minimize or background the metered application. | Runtime continues to count because application metering is process-based. |

## Schedule, Detox, tier, and precedence cases

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-ACT-001 | Observe a website and application immediately before, during, and after a schedule window. | They are allowed before, blocked during, and allowed after the active window. |
| VM-ACT-002 | Start a short Detox containing website and application rules and let it finish naturally. | Selected rules activate during Detox and deactivate after its absolute end. |
| VM-ACT-003 | Evaluate a Tier 1 list without any attached schedule. | Matching targets remain blocked while the list is enabled. |
| VM-ACT-004 | Evaluate Tier 2 inside and outside its schedule and try Manual Unlock. | It blocks only while activated and cannot be manually unlocked. Domain patterns appear in the managed hosts block only while Tier 2 is active. |
| VM-ACT-005 | Evaluate Tier 3 inside and outside its schedule and request a valid unlock. | It is active only through schedule or Detox, supports its valid temporary unlock, and is not represented in `/etc/hosts`. |
| VM-ACT-006 | Apply overlapping Hard, Scheduled Block, and Controlled Access rules. | Hard wins, then active Scheduled Block, then Controlled Access. |
| VM-ACT-007 | Apply two Controlled Access rules with different allowances. | The strictest applicable allowance determines access. |

## Browser integration cases

Run only one browser of a family at a time. Record the browser package source,
version, extension version, profile path, and policy path.

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-BR-001 | With a clean onboarding profile, install the signed extension from its store after BlocKuntu installation. | The first verified heartbeat arrives without replacing an ordinary allowed page. |
| VM-BR-002 | Restart the browser after the first verified heartbeat. | Managed policy is loaded and the extension remains enabled according to that browser's supported policy mechanism. |
| VM-BR-003 | Try to disable or remove the managed extension. | The supported browser prevents the protected action. |
| VM-BR-004 | Test a normal window and the configured private-browsing mode. | Firefox private browsing or the selected Chromium incognito strategy matches the GUI setting; the result is not generalized across browser families. |
| VM-BR-005 | Close the browser, wait, and reopen it. | A new browser session receives startup grace and then a current heartbeat; an old-session heartbeat is not reused. |
| VM-BR-006 | Close and restart the BlocKuntu GUI while leaving the daemon and browser running. | Browser enforcement and heartbeat continue independently of the GUI window. |
| VM-BR-007 | Exercise a supported confined Firefox or Chromium package. | Per-user Native Messaging integration and managed policy work after the documented setup/restart. |
| VM-BR-008 | Exercise an unsupported browser package variant. | It is reported as unsupported or handled by the configured unsupported-browser rule; a heartbeat alone does not turn it into supported coverage. |

### Supported browser-package coverage

The release matrix must eventually include each combination documented in
`Docs/supportedBrowsers.md`:

- Firefox native, Snap, and Flatpak;
- LibreWolf native layout;
- Waterfox native layout;
- Google Chrome native package;
- Chromium native and Snap;
- Brave, Opera, Microsoft Edge, and Vivaldi native packages.

Chromium Flatpak, Brave Snap, Opera Snap, Vivaldi Snap, Vivaldi Flatpak, and
unknown Firefox-family layouts are negative or unsupported cases rather than
successful managed-policy cases.

## Website allowlist acceptance

Use at least two local hostnames, one explicitly allowed and one outside the
allowlist. Confirm both positive access and fail-closed rejection.

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-WAL-001 | Save a Tier 1 website allowlist. | The GUI prevents the unsupported combination or the daemon rejects it. |
| VM-WAL-002 | Attach a Tier 2 website allowlist to an inactive schedule. | Both the allowed site and outside site remain accessible before activation. |
| VM-WAL-003 | Enter the schedule window for that Tier 2 allowlist. | The listed site remains accessible and the outside site is blocked. |
| VM-WAL-004 | Activate the Tier 2 allowlist through Detox instead of its schedule. | The same allow-only behavior applies for the Detox duration. |
| VM-WAL-005 | Inspect `/etc/hosts` while the website allowlist is active. | BlocKuntu does not attempt to express the allowlist as a catch-all hosts rule. |
| VM-WAL-006 | Keep an outside-site tab open and then activate the allowlist. | Existing-tab revalidation replaces it with the allowlist-specific blocked page. |
| VM-WAL-007 | Activate two website allowlists whose permitted sets differ. | Only a URL matching both lists remains accessible. |
| VM-WAL-008 | Allow a URL in the allowlist while also matching it with a blocklist. | The matching blocklist wins. |
| VM-WAL-009 | Use a Tier 3 allowlist with a one-minute allowance and visit only its listed site. | The listed site stays accessible and does not consume the allowlist allowance. |
| VM-WAL-010 | Visit an outside site under the same Tier 3 allowlist. | Usage is charged and the outside site is blocked after exhaustion. |
| VM-WAL-011 | Request a valid Tier 3 unlock for the rejecting allowlist. | The outside site is temporarily accessible, subject to all other active rules. |
| VM-WAL-012 | Attempt Tier 3 unlock while the allowlist is a Detox target. | The unlock is rejected. |
| VM-WAL-013 | Test regular and private windows. | Regular windows are protected; private-window behavior matches the explicit Firefox or Chromium setting. |

## Process-level application allowlist acceptance

These tests are safety-critical. Start with dedicated test processes. Desktop
infrastructure cases should run only after the test fixture can recover and
collect evidence without relying on the graphical session.

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-AAL-001 | Save a Tier 1 application allowlist. | The GUI prevents the unsupported combination or the daemon rejects it. |
| VM-AAL-002 | Attach a Tier 2 application allowlist containing only test app A to an inactive schedule; run A and B. | Both remain running while the list is inactive. |
| VM-AAL-003 | Activate that schedule. | A remains running and independently evaluated test app B is terminated. |
| VM-AAL-004 | Activate the same Tier 2 allowlist through Detox. | The same per-process allow-only behavior applies. |
| VM-AAL-005 | Allow the parent fixture and let it start the differently identified helper. | The parent remains; permission does not transfer and the helper is terminated. |
| VM-AAL-006 | Allow a browser main process without listing its renderer, GPU, utility, or crash-handler identities. | Every non-matching eligible helper is independently rejected; the result demonstrates why all required identities must be explicit. |
| VM-AAL-007 | Use Add all detected, then start a helper that did not exist during the snapshot. | The later helper is not implicitly allowed. Add all detected is confirmed as a point-in-time snapshot. |
| VM-AAL-008 | Activate two application allowlists whose permitted identities differ. | A process must match both active lists to remain running. |
| VM-AAL-009 | Allow test app A but also match A with an active blocklist. | The blocklist wins and A is terminated. |
| VM-AAL-010 | Run only allowed processes under a Tier 3 application allowlist. | The allowlist allowance remains unused. |
| VM-AAL-011 | Start a rejected process in the foreground, background, and minimized states under a one-minute Tier 3 allowance. | Each state consumes process-based runtime and is terminated after exhaustion. |
| VM-AAL-012 | Run two rejected processes simultaneously for one Tier 3 allowlist. | Elapsed time is charged once to the rule, not multiplied by process count. |
| VM-AAL-013 | Request Manual Unlock using the allowlist ID and relaunch a rejected process. | The process remains during the fixed two-minute unlock if no other rule rejects it. |
| VM-AAL-014 | Request Manual Unlock using a rejected process identity. | It resolves to the rejecting allowlist and follows the same preconditions, duration, and quota. |
| VM-AAL-015 | Leave another allowlist or blocklist rejecting the process while one rule is unlocked. | The process is still terminated by the other restriction. |
| VM-AAL-016 | Let the unlock expire while the rejected process is running. | The process is terminated on a subsequent scan and exhausted allowance remains exhausted. |
| VM-AAL-017 | Remove A from an active allowlist. | The stricter edit succeeds and A becomes eligible for termination. |
| VM-AAL-018 | Try to add B to an active allowlist. | The weakening edit is rejected until the protected setting becomes editable. |
| VM-AAL-019 | Observe daemon, GUI, and Native Messaging processes during restrictive allowlist enforcement. | BlocKuntu recovery components and required descendants remain available. |
| VM-AAL-020 | Observe PID 1, root-owned services, and non-desktop system accounts. | They remain outside normal application-allowlist termination. |
| VM-AAL-021 | With an explicitly curated safety list, verify user D-Bus, portals/authentication, audio, clipboard, terminal children, IDE language servers, and browser helpers. | Every required regular-user process has its own matching identity and the desktop remains healthy. |

## Upgrade smoke case

| ID | Actions | Expected result |
| --- | --- | --- |
| VM-UPGRADE-001 | Install a previously accepted package, create blocklists and allowlists, use them, then install the newer package with the native package manager. | Upgrade succeeds; configuration, policy modes, installation identity, intended credentials, and enforcement survive; services and browser heartbeat recover. |

## Completion criteria

- Every pass references the exact package checksum.
- Core acceptance passes on Ubuntu, Fedora, and CachyOS.
- Every supported browser package has an explicit result rather than a generic
  browser-family assumption.
- Application allowlist results show individual process identities; they do not
  infer permission from parentage or application grouping.
- Logs and screenshots are collected before the VM is reverted.
