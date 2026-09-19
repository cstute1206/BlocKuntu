# Layer 4: Destructive, recovery, upgrade, and soak tests

## Safety boundary

Run every case in a disposable VM clone with a verified clean snapshot. These
tests intentionally alter protected files, package state, time state, browser
policy, or the desktop user's processes.

Before each case:

1. record the VM and snapshot name;
2. verify out-of-band SSH or guest-agent access;
3. copy the exact package checksum and current policy export to the host;
4. make the result directory unique to the run;
5. ensure the automation can collect evidence even if the graphical session is
   terminated.

Never run these cases on the development host or a personal browser profile.

## Direct-removal and authorized-uninstall cases

| ID | Actions | Expected result |
| --- | --- | --- |
| DES-UNINSTALL-001 | Attempt `apt remove` or `apt purge` on Ubuntu without a valid GUI removal handoff. | The package refuses direct removal and enforcement remains operational. |
| DES-UNINSTALL-002 | Attempt `dnf remove` on Fedora without a valid GUI removal handoff. | The package refuses direct removal and enforcement remains operational. |
| DES-UNINSTALL-003 | Attempt `pacman -R` on CachyOS without a valid GUI removal handoff. | The package refuses direct removal and enforcement remains operational. |
| DES-UNINSTALL-004 | Enter an incorrect recovery credential in the GUI. | Uninstall is rejected without creating a package-removal lease. |
| DES-UNINSTALL-005 | Use the valid uninstall recovery credential while protected access and the operator-window policy permit removal. | The browser extension first receives the uninstalling state, the native package removal succeeds, and managed BlocKuntu state is cleaned up. |
| DES-UNINSTALL-006 | Inspect the system after authorized uninstall. | Units, daemon/native-host binaries, managed browser policies, Native Messaging manifests, and the BlocKuntu hosts block are removed; unrelated hosts and browser policy content remain. |
| DES-UNINSTALL-007 | Keep a browser open during authorized GUI uninstall. | Ordinary browser-management pages remain reachable and the extension does not permanently fail-close after the native host disappears. |

## Hosts-file hardening cases

Preserve the VM's original `/etc/hosts` on the host as evidence before these
tests.

| ID | Actions | Expected result |
| --- | --- | --- |
| DES-HOSTS-001 | Clear the immutable flag with `chattr -i`, remove or modify BlocKuntu-managed rows, and leave unrelated rows untouched. | BlocKuntu repairs its exact managed block and restores its required immutable state without changing unrelated rows. |
| DES-HOSTS-002 | Add unrelated valid hosts entries outside the managed markers. | Repair preserves those entries. |
| DES-HOSTS-003 | Delete `/etc/hosts` in the disposable clone. | BlocKuntu recreates a hosts file containing the currently required managed block. Record loss or recovery of non-BlocKuntu baseline entries separately; deleted external content cannot be inferred from policy. |
| DES-HOSTS-004 | Activate and end a Tier 2 domain schedule after a repair. | The scheduled domain appears and disappears at the correct boundaries while the Tier 1 block remains. |
| DES-HOSTS-005 | Activate website allowlists only and inspect the managed block. | No catch-all hosts representation is created for allowlists. |

## Database and policy-recovery cases

The default database is `/var/lib/blockuntu/blockuntu.sqlite3`; the recovery
policy is `/etc/blockuntu/policy-recovery.toml`.

| ID | Actions | Expected result |
| --- | --- | --- |
| DES-DATA-001 | Create blocklists, website allowlists, and application allowlists, then verify that the recovery TOML is current. | The snapshot contains the supported policy objects and modes. |
| DES-DATA-002 | Remove the SQLite database through the controlled destructive harness and restart enforcement. | Policy is restored from recovery TOML and restrictions fail closed during recovery. Runtime history, counters, and statistics are not required to reappear. |
| DES-DATA-003 | Corrupt SQLite while retaining a valid recovery snapshot. | The daemon does not silently start with an empty permissive policy; recovery or an explicit fatal state is recorded. |
| DES-DATA-004 | Remove both SQLite and the recovery snapshot. | BlocKuntu does not claim that the previous policy was recovered; the result is an explicit failure/recovery state rather than fabricated configuration. |
| DES-DATA-005 | Modify the protected recovery TOML and trigger verification. | Unauthorized modification is detected, repaired, rejected, or reported according to the current hardening contract. |
| DES-DATA-006 | Recover a policy containing allowlists. | Website/application modes, tiers, schedules, allowed entries, and allowances survive; Tier 1 allowlists are not introduced. |

## Clock-tamper cases

Use a dedicated snapshot because changing the guest clock can invalidate TLS,
package-manager, browser, and journal behavior.

| ID | Actions | Expected result |
| --- | --- | --- |
| DES-CLOCK-001 | Move the wall clock backwards after establishing the integrity baseline. | Clock tampering is reported and time-sensitive restrictions fail closed. |
| DES-CLOCK-002 | Move the wall clock forward across schedule, allowance, or Detox boundaries, then restore it. | The user cannot gain permissive access merely by changing the wall clock. |
| DES-CLOCK-003 | Reboot after detected clock tampering. | Integrity state and safe enforcement recover according to the documented baseline rules. |
| DES-CLOCK-004 | Try policy import, protected-setting changes, or Tier 1 edit unlock while the clock is considered tampered. | Protected operations are rejected. |

## Protected-setting and Tier 1 edit cases

| ID | Actions | Expected result |
| --- | --- | --- |
| DES-PROTECT-001 | Configure protected access to require no active schedule or Detox, then activate a schedule. | Protected settings and GUI uninstall remain closed. |
| DES-PROTECT-002 | Repeat with an active Detox session. | Protected settings and GUI uninstall remain closed. |
| DES-PROTECT-003 | End all schedules and Detox sessions without clock tampering. | Protected access becomes available again when all configured conditions are satisfied. |
| DES-PROTECT-004 | Enter an invalid Tier 1 edit key. | Editing remains locked. |
| DES-PROTECT-005 | Enter the valid key inside the permitted operator window and edit a Tier 1 list. | The authorized edit succeeds. |
| DES-PROTECT-006 | Wait beyond the five-minute Tier 1 edit window and attempt another protected edit. | The later edit is rejected. |
| DES-PROTECT-007 | Try to use Tier 1 edit authorization to weaken an active Tier 2 or Tier 3 list. | Authorization does not cross rule-tier or active-edit boundaries. |

## Browser-policy and Native Messaging recovery cases

| ID | Actions | Expected result |
| --- | --- | --- |
| DES-BROWSER-001 | Delete or modify a managed browser-policy file after a verified heartbeat. | The daemon repairs the policy for that supported browser. |
| DES-BROWSER-002 | Remove or break a Native Messaging manifest. | Health degrades visibly and the supported repair path restores communication. |
| DES-BROWSER-003 | Stop or make the native host unreachable while an allowed HTTP(S) page is open. | New navigation fails closed; existing-tab failure follows the configured consecutive-failure tolerance. |
| DES-BROWSER-004 | Restore the daemon/native-host path without reinstalling the extension. | Heartbeats resume and ordinary allowed pages recover. |
| DES-BROWSER-005 | Kill and restart the daemon while browsers remain open. | The extension reports the outage, reconnects, and returns to current-session healthy state. |
| DES-BROWSER-006 | Refresh or update a supported confined browser package. | Per-user integration and policy are repaired for the new active revision. |

## Application-allowlist safety cases

These cases deliberately test the failure mode of an incomplete process list.
Keep host-side access and evidence collection available throughout.

| ID | Actions | Expected result |
| --- | --- | --- |
| DES-AAL-001 | Activate a Tier 2 application allowlist that omits selected regular-user session helpers. | Each omitted eligible process is independently terminated and recorded; no parent or application grouping grants permission. |
| DES-AAL-002 | Observe the session after omitted D-Bus, portal, authentication, audio, or clipboard processes are terminated. | Desktop degradation is captured as an expected configuration risk, not mistaken for a daemon crash. |
| DES-AAL-003 | Use a complete explicitly curated allowlist for the same desktop session. | Required services remain healthy because each process has its own matching identity. |
| DES-AAL-004 | Activate a restrictive allowlist that rejects ordinary GUI processes. | BlocKuntu daemon, GUI/recovery path where applicable, Native Messaging components, and protected descendants remain available through built-in exemptions. |
| DES-AAL-005 | Start a new helper after using Add all detected. | The new identity is rejected until explicitly covered; the saved snapshot does not expand automatically. |
| DES-AAL-006 | Exhaust a Tier 3 application-allowlist allowance, unlock the rejecting rule, and let the unlock expire. | Relaunched rejected processes work only during the fixed unlock and are terminated again after expiry; allowance stays exhausted. |
| DES-AAL-007 | Recover SQLite from a policy snapshot containing an active-capable application allowlist. | Per-process semantics and recovery exemptions remain intact after recovery. |

## Package-upgrade cases

| ID | Actions | Expected result |
| --- | --- | --- |
| DES-UPGRADE-001 | Install the previous accepted Debian package, create policy and usage state, then install the candidate `.deb`. | `apt` performs an upgrade rather than removal; policy, modes, installation identity, intended credentials, and services survive. |
| DES-UPGRADE-002 | Repeat for the previous accepted RPM. | `dnf` performs the upgrade and preserves the same contract. |
| DES-UPGRADE-003 | Repeat for the previous accepted Arch/CachyOS package. | `pacman` performs the upgrade and preserves the same contract. |
| DES-UPGRADE-004 | Keep supported browser profiles and extensions installed during upgrade. | Native Messaging and current-session heartbeats recover without reinstalling the extensions. |
| DES-UPGRADE-005 | Include website and application allowlists in the old policy. | Allowlist mode and entries survive without becoming Tier 1 or changing activation semantics. |

## Soak and repeated-lifecycle cases

| ID | Duration/actions | Expected result |
| --- | --- | --- |
| SOAK-001 | Keep a deterministic allowed local page open for one hour, including periods where the tab is backgrounded. | No false missing-heartbeat page, browser termination, or unexplained Native Messaging disconnect occurs. |
| SOAK-002 | Play a YouTube video for one hour with stable network access. | No BlocKuntu missed-heartbeat error or browser kill occurs. Network or YouTube failures are recorded separately from BlocKuntu failures. |
| SOAK-003 | Repeat browser close/open cycles and profile restarts for one hour. | Every new session receives startup grace and transitions to a current heartbeat without accepting stale prior-session state. |
| SOAK-004 | Keep an active schedule across its start and end while a browser and test application remain open. | Enforcement activates and releases at the correct boundaries without restarting BlocKuntu. |
| SOAK-005 | Run a Tier 3 website allowance long enough to cross warning and exhaustion thresholds. | Notifications, accounting, block transition, and local-midnight behavior occur once at the correct boundaries. |
| SOAK-006 | Run allowed and rejected processes under a Tier 3 application allowlist, including background and minimized processes. | Only rejected-process presence consumes time, multiple rejected processes do not multiply charge, and exhaustion terminates the rejected processes. |
| SOAK-007 | Leave the GUI closed while daemon, browsers, schedules, and allowances continue for one hour. | Enforcement and heartbeat remain operational without the GUI window. |

## Completion criteria

- Each destructive case starts from a clean disposable clone.
- The base VM is never modified by a test.
- Evidence remains accessible even after GUI or session failure.
- A one-hour run passes only when both the final state and the intervening logs
  are free of false heartbeat failures.
- Recovery claims distinguish restored policy from non-restored historical
  usage or statistics.
