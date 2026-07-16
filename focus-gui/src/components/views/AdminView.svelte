<script lang="ts">
  import {
    Activity,
    AlertTriangle,
    CircleMinus,
    CheckCircle2,
    Clipboard,
    Clock3,
    Download,
    FileText,
    Gauge,
    KeyRound,
    Monitor,
    RefreshCw,
    Settings,
    ShieldCheck,
    Trash2,
    Upload,
    Wrench,
    X,
    XCircle
  } from "@lucide/svelte";
  import type { ApplicationUiPreferences } from "../../lib/ui";
  import type {
    EnforcementStatus,
    HealthCheck,
    SystemHealth,
    UninstallResult,
    WindowDetectionStatus
  } from "../../lib/types";

  type Icon = typeof Activity;
  type SettingsSection =
    | "health"
    | "enforcement"
    | "browser"
    | "policy"
    | "protected"
    | "application"
    | "logging"
    | "maintenance";

  interface Props {
    health: SystemHealth | null;
    enforcement: EnforcementStatus | null;
    runningAppsWindowDetection: WindowDetectionStatus | null;
    applicationUiPreferences: ApplicationUiPreferences;
    uninstallPhrase: string | null;
    uninstallPhraseLoading: boolean;
    uninstallPhraseInput?: string;
    uninstallRunning: boolean;
    uninstallResult: UninstallResult | null;
    uninstallPhraseError: string | null;
    tier1EditPhraseInput?: string;
    tier1EditUnlocking: boolean;
    tier1EditUnlockedUntil: string | null;
    operatorWindowOpen: boolean;
    operatorWindowLabel: string;
    tier1EditMessage: string | null;
    tier1EditKeyError: string | null;
    policyExportRunning: boolean;
    policyImportRunning: boolean;
    policyTransferMessage: string | null;
    policyTransferError: string | null;
    onRefreshHealth: () => void | Promise<void>;
    onCopyDiagnostics: () => void | Promise<void>;
    onRunUninstallBlockuntu: () => void | Promise<void>;
    onUnlockTier1Edit: () => void | Promise<void>;
    onExportPolicyToml: () => void | Promise<void>;
    onImportPolicyToml: () => void | Promise<void>;
    onUpdateApplicationUiPreferences: (preferences: ApplicationUiPreferences) => void;
    onShowFirstRunOverview: () => void;
    onClose: () => void;
  }

  const settingsSections: Array<{ id: SettingsSection; label: string; icon: Icon }> = [
    { id: "health", label: "Health", icon: Gauge },
    { id: "enforcement", label: "Enforcement", icon: ShieldCheck },
    { id: "browser", label: "Browser integration", icon: Monitor },
    { id: "policy", label: "Policy and recovery", icon: Download },
    { id: "protected", label: "Protected changes", icon: KeyRound },
    { id: "application", label: "Application UI", icon: Settings },
    { id: "logging", label: "Logging", icon: FileText },
    { id: "maintenance", label: "Maintenance", icon: Trash2 }
  ];

  let {
    health,
    enforcement,
    runningAppsWindowDetection,
    applicationUiPreferences,
    uninstallPhrase,
    uninstallPhraseLoading,
    uninstallPhraseInput = $bindable(""),
    uninstallRunning,
    uninstallResult,
    uninstallPhraseError,
    tier1EditPhraseInput = $bindable(""),
    tier1EditUnlocking,
    tier1EditUnlockedUntil,
    operatorWindowOpen,
    operatorWindowLabel,
    tier1EditMessage,
    tier1EditKeyError,
    policyExportRunning,
    policyImportRunning,
    policyTransferMessage,
    policyTransferError,
    onRefreshHealth,
    onCopyDiagnostics,
    onRunUninstallBlockuntu,
    onUnlockTier1Edit,
    onExportPolicyToml,
    onImportPolicyToml,
    onUpdateApplicationUiPreferences,
    onShowFirstRunOverview,
    onClose
  }: Props = $props();

  let activeSection: SettingsSection = $state("health");
  let healthChecks = $derived(health?.checks ?? []);
  let browserIntegrationChecks = $derived(
    healthChecks.filter((check) =>
      /firefox|chrome|chromium|native_host|extension/i.test(check.key)
    )
  );
  let okHealthCount = $derived(healthChecks.filter((check) => check.state === "ok").length);
  let warnHealthCount = $derived(healthChecks.filter((check) => check.state === "warn").length);
  let errorHealthCount = $derived(healthChecks.filter((check) => check.state === "error").length);
  let canRunUninstall = $derived(
    Boolean(operatorWindowOpen && uninstallPhrase && uninstallPhraseInput.trim() && !uninstallPhraseLoading)
  );
  let canUnlockTier1Edit = $derived(Boolean(operatorWindowOpen && tier1EditPhraseInput.trim()));
  let policyActionRunning = $derived(policyExportRunning || policyImportRunning);
  let protectionState = $derived(
    tier1EditUnlockedUntil && Date.parse(tier1EditUnlockedUntil) > Date.now()
      ? `Unlocked until ${new Date(tier1EditUnlockedUntil).toLocaleString()}`
      : "Locked"
  );

  function checkIcon(check: HealthCheck): Icon {
    if (check.state === "ok") return CheckCircle2;
    if (check.state === "inactive") return CircleMinus;
    if (check.state === "pending") return Clock3;
    if (check.state === "error") return XCircle;
    if (check.state === "warn") return AlertTriangle;
    return Activity;
  }

  function healthStateLabel(state: HealthCheck["state"]): string {
    if (state === "inactive") return "not running";
    if (state === "pending") return "starting";
    return state;
  }

  function formatStatus(value: boolean | undefined, enabled = "active", disabled = "not active"): string {
    if (value === undefined) return "unavailable";
    return value ? enabled : disabled;
  }

  function updateRestoreLastSelectedPage(event: Event): void {
    onUpdateApplicationUiPreferences({
      ...applicationUiPreferences,
      restoreLastSelectedPage: (event.currentTarget as HTMLInputElement).checked
    });
  }

  function updateRefreshInterval(event: Event): void {
    const value = Number((event.currentTarget as HTMLSelectElement).value);
    if (![5, 15, 30, 60].includes(value)) return;
    onUpdateApplicationUiPreferences({
      ...applicationUiPreferences,
      refreshIntervalSeconds: value as ApplicationUiPreferences["refreshIntervalSeconds"]
    });
  }

  function closeOnEscape(event: KeyboardEvent): void {
    if (event.key === "Escape") onClose();
  }

  function closeFromBackdrop(event: MouseEvent): void {
    if (event.currentTarget === event.target) onClose();
  }
</script>

<div class="settings-modal-backdrop" role="presentation" onclick={closeFromBackdrop}>
  <div
    class="settings-modal"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settings-modal-title"
    tabindex="-1"
    onkeydown={closeOnEscape}
  >
    <header class="settings-modal-header">
      <div>
        <p class="eyebrow">Local enforcement</p>
        <h2 id="settings-modal-title">Settings</h2>
      </div>
      <button class="icon-button" title="Close Settings" aria-label="Close Settings" onclick={onClose}>
        <X size={19} aria-hidden="true" />
      </button>
    </header>

    <div class="settings-modal-layout">
      <nav class="settings-section-nav" aria-label="Settings sections">
        {#each settingsSections as section (section.id)}
          {@const SectionIcon = section.icon}
          <button
            class:active={activeSection === section.id}
            onclick={() => (activeSection = section.id)}
          >
            <SectionIcon size={17} aria-hidden="true" />
            <span>{section.label}</span>
          </button>
        {/each}
      </nav>

      <div class="settings-modal-content">
        {#if activeSection === "health"}
          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3>Health</h3>
                <p>Live daemon, browser, and enforcement checks.</p>
              </div>
              <div class="panel-actions">
                <button class="secondary" onclick={onCopyDiagnostics}>
                  <Clipboard size={17} aria-hidden="true" />
                  <span>Copy diagnostics</span>
                </button>
                <button class="secondary" onclick={onRefreshHealth}>
                  <RefreshCw size={17} aria-hidden="true" />
                  <span>Refresh</span>
                </button>
              </div>
            </div>
            <div class="health-summary" aria-label="Health summary">
              <span class="health-count" data-state="ok"><CheckCircle2 size={15} aria-hidden="true" />{okHealthCount}</span>
              <span class="health-count" data-state="warn"><AlertTriangle size={15} aria-hidden="true" />{warnHealthCount}</span>
              <span class="health-count" data-state="error"><XCircle size={15} aria-hidden="true" />{errorHealthCount}</span>
            </div>
            {#if health}
              <p class="settings-meta">Last checked {new Date(health.checked_at).toLocaleString()}</p>
            {/if}
            <div class="health-grid">
              {#each healthChecks as check (check.key)}
                {@const HealthIcon = checkIcon(check)}
                <div class="health-row" data-state={check.state}>
                  <HealthIcon size={18} aria-hidden="true" />
                  <div class="health-copy"><span>{check.label}</span><small>{check.detail}</small></div>
                  <strong>{healthStateLabel(check.state)}</strong>
                </div>
              {:else}
                <p class="empty-state">No health checks available. Refresh after the daemon starts.</p>
              {/each}
            </div>
          </section>
        {:else if activeSection === "enforcement"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Enforcement</h3><p>Live state only. Policy changes stay daemon-controlled.</p></div></div>
            <div class="status-list">
              <div class="status-row"><span>Enforcement</span><strong data-state={enforcement?.enforcement_state === "active" ? "active" : "stopped"}>{enforcement?.enforcement_state ?? "unavailable"}</strong></div>
              <div class="status-row"><span>Firefox requirement</span><small>{formatStatus(enforcement?.firefox_policy?.compliant, "managed and compliant", "needs repair")}</small></div>
              <div class="status-row"><span>Chrome/Chromium requirement</span><small>{formatStatus(enforcement?.chrome_policy?.compliant, "managed and compliant", "needs repair")}</small></div>
              <div class="status-row"><span>Hosts immutability</span><small>{enforcement?.hosts_file?.immutable_required ? enforcement.hosts_file.immutable_state : "not required"}</small></div>
              <div class="status-row"><span>Unsupported browsers</span><small>{healthChecks.find((check) => check.key === "unsupported_browser_hard_block")?.detail ?? "unavailable"}</small></div>
              <div class="status-row"><span>Window-title matching</span><small>{runningAppsWindowDetection?.detail ?? "Check Applications after the daemon is online."}</small></div>
            </div>
            <p class="settings-note">Browser heartbeat grace periods and application scan intervals are not configurable through the current daemon API.</p>
          </section>
        {:else if activeSection === "browser"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Browser integration</h3><p>Use these checks to identify the supported repair path.</p></div></div>
            <div class="settings-check-list">
              {#each browserIntegrationChecks as check (check.key)}
                {@const BrowserIcon = checkIcon(check)}
                <div class="settings-check" data-state={check.state}>
                  <BrowserIcon size={17} aria-hidden="true" />
                  <div><strong>{check.label}</strong><small>{check.detail}</small></div>
                </div>
              {:else}
                <p class="empty-state">Browser integration checks appear when a supported browser is present.</p>
              {/each}
            </div>
            <div class="status-list browser-identifiers">
              <div class="status-row"><span>Firefox extension ID</span><small>{enforcement?.firefox_policy?.extension_id ?? "unavailable"}</small></div>
              <div class="status-row"><span>Chrome extension ID</span><small>{enforcement?.chrome_policy?.extension_id ?? "unavailable"}</small></div>
            </div>
            <div class="repair-note"><Wrench size={17} aria-hidden="true" /><p>For Firefox Snap or Flatpak, open a terminal and run <code>blockuntu-setup-confined-firefox</code>, then restart Firefox.</p></div>
          </section>
        {:else if activeSection === "policy"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Policy and recovery</h3><p>Move policy through the daemon using a portable TOML file.</p></div></div>
            <div class="policy-file-actions">
              <button class="secondary" onclick={onExportPolicyToml} disabled={policyActionRunning}><Download size={17} aria-hidden="true" /><span>{policyExportRunning ? "Exporting" : "Export TOML"}</span></button>
              <button class="secondary" onclick={onImportPolicyToml} disabled={policyActionRunning} title="Append TOML"><Upload size={17} aria-hidden="true" /><span>{policyImportRunning ? "Appending" : "Append TOML"}</span></button>
            </div>
            {#if policyTransferMessage}<p class="result-text">{policyTransferMessage}</p>{/if}
            {#if policyTransferError}<p class="result-text danger-text">{policyTransferError}</p>{/if}
            <p class="settings-note">Policy database paths and recovery snapshot creation or restore are not exposed by the current daemon API.</p>
          </section>
        {:else if activeSection === "protected"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Protected changes</h3><p>The only place to open the short protected-edit window.</p></div></div>
            <div class="status-list">
              <div class="status-row"><span>Operator window</span><strong data-state={operatorWindowOpen ? "active" : "stopped"}>{operatorWindowOpen ? "open" : "closed"}</strong></div>
              <div class="status-row"><span>Allowed time</span><small>{operatorWindowLabel}</small></div>
              <div class="status-row"><span>Edit unlock</span><small>{protectionState}</small></div>
            </div>
            <div class="tier1-edit-form admin-action-form">
              <label><span>Edit key</span><input bind:value={tier1EditPhraseInput} autocomplete="off" placeholder="Type the Tier 1 edit key" spellcheck="false" /></label>
              <button class="primary" onclick={onUnlockTier1Edit} disabled={tier1EditUnlocking || !canUnlockTier1Edit}><KeyRound size={17} aria-hidden="true" /><span>{tier1EditUnlocking ? "Unlocking" : "Unlock 5 min"}</span></button>
            </div>
            {#if tier1EditMessage}<p class="result-text">{tier1EditMessage}</p>{/if}
            {#if tier1EditKeyError}<p class="result-text danger-text">{tier1EditKeyError}</p>{/if}
          </section>
        {:else if activeSection === "application"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Application UI</h3><p>These local preferences do not change enforcement.</p></div></div>
            <div class="preference-list">
              <label class="preference-row"><span><strong>Restore last selected page</strong><small>Open the page you were using when the GUI was last closed.</small></span><input type="checkbox" checked={applicationUiPreferences.restoreLastSelectedPage} onchange={updateRestoreLastSelectedPage} /></label>
              <label class="preference-row"><span><strong>Dashboard and status refresh</strong><small>How often the GUI reloads live daemon status.</small></span><select value={applicationUiPreferences.refreshIntervalSeconds} onchange={updateRefreshInterval}><option value="5">Every 5 seconds</option><option value="15">Every 15 seconds</option><option value="30">Every 30 seconds</option><option value="60">Every minute</option></select></label>
            </div>
            <div class="button-row compact-row settings-action-row"><button class="secondary" onclick={onShowFirstRunOverview}>Show first-run overview</button></div>
            <p class="settings-note">Starting on login, tray-close behaviour, and desktop notifications need reviewed native-runtime support and are not configurable yet.</p>
          </section>
        {:else if activeSection === "logging"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Logging</h3><p>BlocKuntu writes plain local event entries; there is no GUI log viewer.</p></div></div>
            <div class="log-file-note">
              <FileText size={18} aria-hidden="true" />
              <div><strong>Event log</strong><code>/etc/blockuntu/blockuntu.log</code><small>The daemon appends every recorded event here. Use a terminal to inspect it.</small></div>
            </div>
            <div class="log-command-list"><code>sudo tail -f /etc/blockuntu/blockuntu.log</code><code>sudo less /etc/blockuntu/blockuntu.log</code></div>
          </section>
        {:else if activeSection === "maintenance"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Maintenance</h3><p>Disruptive actions are kept separate from everyday settings.</p></div></div>
            <div class="button-row compact-row settings-action-row"><button class="secondary" onclick={onShowFirstRunOverview}>Reset first-run overview</button></div>
            <div class="uninstall-form admin-action-form">
              <div class="status-list"><div class="status-row"><span>Uninstall allowed</span><strong data-state={operatorWindowOpen ? "active" : "stopped"}>{operatorWindowOpen ? "open" : "closed"}</strong></div><div class="status-row"><span>Allowed time</span><small>{operatorWindowLabel}</small></div></div>
              <label><span>Confirmation phrase</span><input bind:value={uninstallPhraseInput} autocomplete="off" placeholder="Type an uninstall phrase" spellcheck="false" /></label>
              <button class="secondary danger-action" onclick={onRunUninstallBlockuntu} disabled={uninstallRunning || !canRunUninstall}><Trash2 size={17} aria-hidden="true" /><span>{uninstallRunning ? "Removing" : "Uninstall BlocKuntu"}</span></button>
            </div>
            {#if uninstallPhraseError}<p class="result-text danger-text">{uninstallPhraseError}</p>{/if}
            {#if uninstallResult}<p class="result-text">{uninstallResult.detail}</p>{/if}
          </section>
        {/if}
      </div>
    </div>
  </div>
</div>
