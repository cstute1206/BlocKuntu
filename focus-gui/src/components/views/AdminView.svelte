<script lang="ts">
  import {
    Activity,
    AlertTriangle,
    Bell,
    CircleMinus,
    CheckCircle2,
    Clock3,
    Download,
    FileText,
    Gauge,
    KeyRound,
    RefreshCw,
    ShieldCheck,
    Trash2,
    Upload,
    X,
    XCircle
  } from "@lucide/svelte";
  import type {
    EnforcementStatus,
    HealthCheck,
    NotificationPreferences,
    SystemHealth,
    UninstallResult
  } from "../../lib/types";

  type Icon = typeof Activity;
  type SettingsSection =
    | "health"
    | "enforcement"
    | "policy"
    | "protected"
    | "notifications"
    | "logging";

  interface Props {
    health: SystemHealth | null;
    enforcement: EnforcementStatus | null;
    notificationPreferences: NotificationPreferences | null;
    notificationPreferencesSaving: boolean;
    notificationPreferencesError: string | null;
    installationSerial: string | null;
    buildNumber: string | null;
    uninstallPhraseLoading: boolean;
    uninstallPhraseConfiguredState: boolean;
    uninstallPhraseSetupInput?: string;
    uninstallPhraseSetupConfirmation?: string;
    uninstallPhraseConfiguring: boolean;
    uninstallPhraseInput?: string;
    uninstallRunning: boolean;
    uninstallResult: UninstallResult | null;
    uninstallPhraseError: string | null;
    tier1EditPhraseInput?: string;
    tier1EditUnlocking: boolean;
    tier1EditUnlocked: boolean;
    tier1EditUnlockedUntil: string | null;
    operatorWindowOpen: boolean;
    operatorWindowLabel: string;
    operatorWindowRestrictionEnabled: boolean;
    tier1EditCredentialConfigured: boolean;
    tier1EditMessage: string | null;
    tier1EditCredentialSetupInput?: string;
    tier1EditCredentialSetupConfirmation?: string;
    policyExportRunning: boolean;
    policyImportRunning: boolean;
    policyTransferMessage: string | null;
    policyTransferError: string | null;
    onRefreshHealth: () => void | Promise<void>;
    onRunUninstallBlockuntu: () => void | Promise<void>;
    onConfigureUninstallPhrase: () => void | Promise<void>;
    onUnlockTier1Edit: () => void | Promise<void>;
    onConfigureTier1Credential: () => void | Promise<void>;
    onUpdateOperatorWindowRestriction: (enabled: boolean) => void | Promise<void>;
    onExportPolicyToml: () => void | Promise<void>;
    onImportPolicyToml: () => void | Promise<void>;
    onUpdateNotificationPreferences: (
      preferences: NotificationPreferences
    ) => void | Promise<void>;
    onShowFirstRunOverview: () => void;
    onClose: () => void;
  }

  const settingsSections: Array<{ id: SettingsSection; label: string; icon: Icon }> = [
    { id: "health", label: "Health", icon: Gauge },
    { id: "enforcement", label: "Enforcement", icon: ShieldCheck },
    { id: "policy", label: "Export and import rules", icon: Download },
    { id: "protected", label: "Protected changes and uninstall", icon: KeyRound },
    { id: "notifications", label: "Notifications", icon: Bell },
    { id: "logging", label: "Logging", icon: FileText }
  ];

  let {
    health,
    enforcement,
    notificationPreferences,
    notificationPreferencesSaving,
    notificationPreferencesError,
    installationSerial,
    buildNumber,
    uninstallPhraseLoading,
    uninstallPhraseConfiguredState,
    uninstallPhraseSetupInput = $bindable(""),
    uninstallPhraseSetupConfirmation = $bindable(""),
    uninstallPhraseConfiguring,
    uninstallPhraseInput = $bindable(""),
    uninstallRunning,
    uninstallResult,
    uninstallPhraseError,
    tier1EditPhraseInput = $bindable(""),
    tier1EditUnlocking,
    tier1EditUnlocked,
    tier1EditUnlockedUntil,
    operatorWindowOpen,
    operatorWindowLabel,
    operatorWindowRestrictionEnabled,
    tier1EditCredentialConfigured,
    tier1EditMessage,
    tier1EditCredentialSetupInput = $bindable(""),
    tier1EditCredentialSetupConfirmation = $bindable(""),
    policyExportRunning,
    policyImportRunning,
    policyTransferMessage,
    policyTransferError,
    onRefreshHealth,
    onRunUninstallBlockuntu,
    onConfigureUninstallPhrase,
    onUnlockTier1Edit,
    onConfigureTier1Credential,
    onUpdateOperatorWindowRestriction,
    onExportPolicyToml,
    onImportPolicyToml,
    onUpdateNotificationPreferences,
    onShowFirstRunOverview,
    onClose
  }: Props = $props();

  let activeSection: SettingsSection = $state("health");
  let customAllowanceThreshold = $state("");
  $effect(() => {
    customAllowanceThreshold = String(
      notificationPreferences?.allowance_warning_minutes.find(
        (minutes) => minutes !== 5 && minutes !== 1
      ) ?? ""
    );
  });
  let healthChecks = $derived(health?.checks ?? []);
  let okHealthCount = $derived(healthChecks.filter((check) => check.state === "ok").length);
  let warnHealthCount = $derived(healthChecks.filter((check) => check.state === "warn").length);
  let errorHealthCount = $derived(healthChecks.filter((check) => check.state === "error").length);
  let canRunUninstall = $derived(
    Boolean(uninstallPhraseConfiguredState && uninstallPhraseInput.trim() && !uninstallPhraseLoading)
  );
  let canConfigureUninstallPhrase = $derived(
    Boolean(
      !uninstallPhraseConfiguredState &&
        uninstallPhraseSetupInput.trim().length >= 16 &&
        uninstallPhraseSetupInput === uninstallPhraseSetupConfirmation
    )
  );
  let canUnlockTier1Edit = $derived(
    Boolean(tier1EditCredentialConfigured && operatorWindowOpen && tier1EditPhraseInput.trim())
  );
  let canConfigureTier1Edit = $derived(
    Boolean(
      !tier1EditCredentialConfigured &&
        tier1EditCredentialSetupInput.trim().length >= 16 &&
        tier1EditCredentialSetupInput === tier1EditCredentialSetupConfirmation
    )
  );
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

  function updateNotificationPreferences(
    patch: Partial<NotificationPreferences>
  ): void {
    if (!notificationPreferences || notificationPreferencesSaving) return;
    void onUpdateNotificationPreferences({
      ...notificationPreferences,
      ...patch
    });
  }

  function updateNotificationBoolean(
    key: keyof NotificationPreferences,
    event: Event
  ): void {
    updateNotificationPreferences({
      [key]: (event.currentTarget as HTMLInputElement).checked
    });
  }

  function updateAllowanceThreshold(minutes: number, event: Event): void {
    if (!notificationPreferences) return;
    const thresholds = new Set(notificationPreferences.allowance_warning_minutes);
    if ((event.currentTarget as HTMLInputElement).checked) {
      thresholds.add(minutes);
    } else {
      thresholds.delete(minutes);
    }
    updateNotificationPreferences({
      allowance_warning_minutes: [...thresholds]
    });
  }

  function updateCustomAllowanceThreshold(): void {
    if (!notificationPreferences) return;
    const thresholds: number[] = notificationPreferences.allowance_warning_minutes.filter(
      (minutes) => minutes === 5 || minutes === 1
    );
    const custom = Number(customAllowanceThreshold);
    if (Number.isInteger(custom) && custom >= 1 && custom <= 1440) {
      thresholds.push(custom);
    } else if (customAllowanceThreshold.trim()) {
      return;
    }
    updateNotificationPreferences({
      allowance_warning_minutes: [...new Set(thresholds)]
    });
  }

  function selectAllNotificationOptions(enabled: boolean): void {
    if (!notificationPreferences) return;
    updateNotificationPreferences({
      enabled,
      website_blocked: enabled,
      application_blocked: enabled,
      allowance_warnings: enabled,
      allowance_warning_minutes: enabled
        ? [...new Set([...notificationPreferences.allowance_warning_minutes, 5, 1])]
        : [],
      schedule_started: enabled,
      schedule_ended: enabled,
      detox_started: enabled,
      detox_ended: enabled
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
            <div class="status-list installation-info">
              <div class="status-row"><span>Build</span><small>{buildNumber ?? "Unavailable"}</small></div>
              <div class="status-row"><span>Installation serial</span><small>{installationSerial ?? "Unavailable"}</small></div>
            </div>
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
              <div class="status-row"><span>Hosts immutability</span><small>{enforcement?.hosts_file?.immutable_required ? enforcement.hosts_file.immutable_state : "not required"}</small></div>
              <div class="status-row"><span>Unsupported browsers</span><small>{healthChecks.find((check) => check.key === "unsupported_browser_hard_block")?.detail ?? "unavailable"}</small></div>
            </div>
            <p class="settings-note">Browser integration health is included in the Health checks above.</p>
          </section>
        {:else if activeSection === "policy"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Export and import rules</h3><p>Move policy through the daemon using a portable TOML file.</p></div></div>
            <div class="policy-file-actions">
              <button class="secondary" onclick={onExportPolicyToml} disabled={policyActionRunning}><Download size={17} aria-hidden="true" /><span>{policyExportRunning ? "Exporting" : "Export TOML"}</span></button>
              <button class="secondary" onclick={onImportPolicyToml} disabled={policyActionRunning} title="Append TOML"><Upload size={17} aria-hidden="true" /><span>{policyImportRunning ? "Appending" : "Append TOML"}</span></button>
            </div>
            {#if policyTransferMessage}<p class="result-text">{policyTransferMessage}</p>{/if}
            {#if policyTransferError}<p class="result-text danger-text">{policyTransferError}</p>{/if}
            <p class="settings-note"><strong>Append keeps your current rules.</strong> Imported rules are added; when an imported rule has the same ID as an existing rule, the imported version replaces that rule. No rules are removed simply because they are absent from the imported file.</p>
          </section>
        {:else if activeSection === "protected"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Protected changes and uninstall</h3><p>Manage Tier 1 editing, maintenance, and the protected uninstall flow.</p></div></div>
            <div class="status-list">
              <div class="status-row"><span>Operator window</span><strong data-state={operatorWindowOpen ? "active" : "stopped"}>{operatorWindowOpen ? "open" : "closed"}</strong></div>
              <div class="status-row"><span>Allowed time</span><small>{operatorWindowLabel}</small></div>
              <div class="status-row"><span>Edit unlock</span><small>{protectionState}</small></div>
            </div>
            <label class="preference-row"><span><strong>Sunday restriction</strong><small>Restrict Tier 1 edits and uninstall to Sundays from 20:00 to 23:59.</small></span><input type="checkbox" checked={operatorWindowRestrictionEnabled} disabled={tier1EditCredentialConfigured && !tier1EditUnlocked} onchange={(event) => onUpdateOperatorWindowRestriction((event.currentTarget as HTMLInputElement).checked)} /></label>
            {#if !tier1EditCredentialConfigured}
              <div class="tier1-edit-form admin-action-form">
                <label><span>Set Tier 1 credential</span><input type="password" bind:value={tier1EditCredentialSetupInput} autocomplete="new-password" placeholder="At least 16 characters" spellcheck="false" /></label>
                <label><span>Confirm credential</span><input type="password" bind:value={tier1EditCredentialSetupConfirmation} autocomplete="new-password" placeholder="Repeat the credential" spellcheck="false" /></label>
                <button class="primary" onclick={onConfigureTier1Credential} disabled={tier1EditUnlocking || !canConfigureTier1Edit}><KeyRound size={17} aria-hidden="true" /><span>{tier1EditUnlocking ? "Saving" : "Set credential"}</span></button>
              </div>
              <p class="settings-note">This credential is stored by the daemon and is never displayed again. Save it somewhere secure.</p>
            {:else}
              <div class="tier1-edit-form admin-action-form">
                <label><span>Tier 1 credential</span><input type="password" bind:value={tier1EditPhraseInput} autocomplete="current-password" placeholder="Enter the Tier 1 credential" spellcheck="false" /></label>
                <button class="primary" onclick={onUnlockTier1Edit} disabled={tier1EditUnlocking || !canUnlockTier1Edit}><KeyRound size={17} aria-hidden="true" /><span>{tier1EditUnlocking ? "Unlocking" : "Unlock 5 min"}</span></button>
              </div>
            {/if}
            {#if tier1EditMessage}<p class="result-text">{tier1EditMessage}</p>{/if}
            <div class="button-row compact-row settings-action-row"><button class="secondary" onclick={onShowFirstRunOverview}>Show welcome modal</button></div>
            {#if !uninstallPhraseConfiguredState}
              <div class="uninstall-form admin-action-form">
                <label><span>Set uninstall phrase</span><input type="password" bind:value={uninstallPhraseSetupInput} autocomplete="new-password" placeholder="At least 16 characters" spellcheck="false" /></label>
                <label><span>Confirm uninstall phrase</span><input type="password" bind:value={uninstallPhraseSetupConfirmation} autocomplete="new-password" placeholder="Repeat the phrase" spellcheck="false" /></label>
                <button class="secondary" onclick={onConfigureUninstallPhrase} disabled={uninstallPhraseConfiguring || !canConfigureUninstallPhrase}><KeyRound size={17} aria-hidden="true" /><span>{uninstallPhraseConfiguring ? "Saving" : "Set uninstall phrase"}</span></button>
              </div>
              <p class="settings-note">This phrase is stored only for this desktop user and is never displayed again. Save it somewhere secure.</p>
            {:else}
              <div class="uninstall-form admin-action-form">
                <label><span>Uninstall phrase</span><input type="password" bind:value={uninstallPhraseInput} autocomplete="current-password" placeholder="Enter your uninstall phrase" spellcheck="false" /></label>
                <button class="secondary danger-action" onclick={onRunUninstallBlockuntu} disabled={uninstallRunning || !canRunUninstall}><Trash2 size={17} aria-hidden="true" /><span>{uninstallRunning ? "Removing" : "Uninstall BlocKuntu"}</span></button>
              </div>
            {/if}
            {#if uninstallPhraseError}<p class="result-text danger-text">{uninstallPhraseError}</p>{/if}
            {#if uninstallResult}<p class="result-text">{uninstallResult.detail}</p>{/if}
          </section>
        {:else if activeSection === "notifications"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Notifications</h3><p>Choose which enforcement events BlocKuntu sends to the desktop.</p></div></div>
            {#if notificationPreferences}
              <div class="button-row compact-row settings-action-row"><button class="secondary" onclick={() => selectAllNotificationOptions(true)} disabled={notificationPreferencesSaving}>Select all</button><button class="secondary" onclick={() => selectAllNotificationOptions(false)} disabled={notificationPreferencesSaving}>Unselect all</button></div>
              <div class="preference-list">
                <label class="preference-row"><span><strong>Desktop notifications</strong><small>Master switch for every notification below.</small></span><input type="checkbox" checked={notificationPreferences.enabled} disabled={notificationPreferencesSaving} onchange={(event) => updateNotificationBoolean("enabled", event)} /></label>
                <label class="preference-row"><span><strong>Website blocked</strong><small>Notify when the browser integration blocks a website.</small></span><input type="checkbox" checked={notificationPreferences.website_blocked} disabled={notificationPreferencesSaving || !notificationPreferences.enabled} onchange={(event) => updateNotificationBoolean("website_blocked", event)} /></label>
                <label class="preference-row"><span><strong>Application blocked</strong><small>Notify after BlocKuntu successfully closes a blocked application.</small></span><input type="checkbox" checked={notificationPreferences.application_blocked} disabled={notificationPreferencesSaving || !notificationPreferences.enabled} onchange={(event) => updateNotificationBoolean("application_blocked", event)} /></label>
                <label class="preference-row"><span><strong>Allowance warnings</strong><small>Notify once when an allowance crosses an enabled threshold.</small></span><input type="checkbox" checked={notificationPreferences.allowance_warnings} disabled={notificationPreferencesSaving || !notificationPreferences.enabled} onchange={(event) => updateNotificationBoolean("allowance_warnings", event)} /></label>
                <label class="preference-row"><span><strong>Below 5 minutes</strong><small>Standard early allowance warning.</small></span><input type="checkbox" checked={notificationPreferences.allowance_warning_minutes.includes(5)} disabled={notificationPreferencesSaving || !notificationPreferences.enabled || !notificationPreferences.allowance_warnings} onchange={(event) => updateAllowanceThreshold(5, event)} /></label>
                <label class="preference-row"><span><strong>Below 1 minute</strong><small>Final allowance warning before the limit is exhausted.</small></span><input type="checkbox" checked={notificationPreferences.allowance_warning_minutes.includes(1)} disabled={notificationPreferencesSaving || !notificationPreferences.enabled || !notificationPreferences.allowance_warnings} onchange={(event) => updateAllowanceThreshold(1, event)} /></label>
                <label class="preference-row"><span><strong>Additional threshold</strong><small>Optional custom warning from 1 to 1440 minutes.</small></span><input type="number" min="1" max="1440" step="1" placeholder="Minutes" bind:value={customAllowanceThreshold} disabled={notificationPreferencesSaving || !notificationPreferences.enabled || !notificationPreferences.allowance_warnings} onchange={updateCustomAllowanceThreshold} /></label>
                <label class="preference-row"><span><strong>Schedule started</strong><small>Notify when a configured schedule becomes active.</small></span><input type="checkbox" checked={notificationPreferences.schedule_started} disabled={notificationPreferencesSaving || !notificationPreferences.enabled} onchange={(event) => updateNotificationBoolean("schedule_started", event)} /></label>
                <label class="preference-row"><span><strong>Schedule ended</strong><small>Notify when a configured schedule becomes inactive.</small></span><input type="checkbox" checked={notificationPreferences.schedule_ended} disabled={notificationPreferencesSaving || !notificationPreferences.enabled} onchange={(event) => updateNotificationBoolean("schedule_ended", event)} /></label>
                <label class="preference-row"><span><strong>Detox started</strong><small>Notify when a Detox session starts.</small></span><input type="checkbox" checked={notificationPreferences.detox_started} disabled={notificationPreferencesSaving || !notificationPreferences.enabled} onchange={(event) => updateNotificationBoolean("detox_started", event)} /></label>
                <label class="preference-row"><span><strong>Detox ended</strong><small>Notify when a Detox expires or is ended early.</small></span><input type="checkbox" checked={notificationPreferences.detox_ended} disabled={notificationPreferencesSaving || !notificationPreferences.enabled} onchange={(event) => updateNotificationBoolean("detox_ended", event)} /></label>
              </div>
              <p class="settings-note">Notifications are delivered only while the BlocKuntu GUI or tray process is running. Repeated block events are automatically deduplicated.</p>
            {:else}
              <p class="empty-state">Notification settings are unavailable while the daemon is offline.</p>
            {/if}
            {#if notificationPreferencesError}<p class="result-text danger-text">{notificationPreferencesError}</p>{/if}
          </section>
        {:else if activeSection === "logging"}
          <section class="settings-panel">
            <div class="settings-panel-header"><div><h3>Logging</h3><p>BlocKuntu writes plain local event and notification-delivery entries; there is no GUI log viewer.</p></div></div>
            <div class="log-file-note">
              <FileText size={18} aria-hidden="true" />
              <div><strong>Event log</strong><code>/etc/blockuntu/blockuntu.log</code><small>The daemon appends queued, accepted, and failed notification details here. Use a terminal to inspect it.</small></div>
            </div>
            <div class="log-command-list"><code>sudo tail -f /etc/blockuntu/blockuntu.log</code><code>sudo less /etc/blockuntu/blockuntu.log</code></div>
          </section>
        {/if}
      </div>
    </div>
  </div>
</div>
