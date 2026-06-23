<script lang="ts">
  import {
    Activity,
    AlertTriangle,
    CheckCircle2,
    Download,
    Gauge,
    KeyRound,
    Trash2,
    Upload,
    XCircle
  } from "@lucide/svelte";
  import type { HealthCheck, SystemHealth, UninstallResult } from "../../lib/types";

  type Icon = typeof Activity;

  interface Props {
    health: SystemHealth | null;
    uninstallPhrase: string | null;
    uninstallPhraseLoading: boolean;
    uninstallPhraseInput?: string;
    uninstallRunning: boolean;
    uninstallResult: UninstallResult | null;
    uninstallPhraseError: string | null;
    tier1EditPhraseInput?: string;
    tier1EditUnlocking: boolean;
    tier1EditUnlocked: boolean;
    tier1EditUnlockedUntil: string | null;
    tier1EditRemainingSeconds: number;
    operatorWindowOpen: boolean;
    operatorWindowLabel: string;
    tier1EditMessage: string | null;
    tier1EditKeyError: string | null;
    policyExportRunning: boolean;
    policyImportRunning: boolean;
    policyTransferMessage: string | null;
    policyTransferError: string | null;
    onRunUninstallBlockuntu: () => void | Promise<void>;
    onUnlockTier1Edit: () => void | Promise<void>;
    onExportPolicyToml: () => void | Promise<void>;
    onImportPolicyToml: () => void | Promise<void>;
  }

  let {
    health,
    uninstallPhrase,
    uninstallPhraseLoading,
    uninstallPhraseInput = $bindable(""),
    uninstallRunning,
    uninstallResult,
    uninstallPhraseError,
    tier1EditPhraseInput = $bindable(""),
    tier1EditUnlocking,
    tier1EditUnlocked,
    tier1EditUnlockedUntil,
    tier1EditRemainingSeconds,
    operatorWindowOpen,
    operatorWindowLabel,
    tier1EditMessage,
    tier1EditKeyError,
    policyExportRunning,
    policyImportRunning,
    policyTransferMessage,
    policyTransferError,
    onRunUninstallBlockuntu,
    onUnlockTier1Edit,
    onExportPolicyToml,
    onImportPolicyToml
  }: Props = $props();

  let healthChecks = $derived(health?.checks ?? []);
  let okHealthCount = $derived(healthChecks.filter((check) => check.state === "ok").length);
  let warnHealthCount = $derived(healthChecks.filter((check) => check.state === "warn").length);
  let errorHealthCount = $derived(healthChecks.filter((check) => check.state === "error").length);
  let canRunUninstall = $derived(
    Boolean(operatorWindowOpen && uninstallPhrase && uninstallPhraseInput.trim() && !uninstallPhraseLoading)
  );
  let canUnlockTier1Edit = $derived(Boolean(operatorWindowOpen && tier1EditPhraseInput.trim()));
  let policyActionRunning = $derived(policyExportRunning || policyImportRunning);

  function checkIcon(check: HealthCheck): Icon {
    if (check.state === "ok") return CheckCircle2;
    if (check.state === "error") return XCircle;
    if (check.state === "warn") return AlertTriangle;
    return Activity;
  }
</script>

<section class="content-grid admin-grid">
  <article class="panel admin-health-panel">
    <div class="panel-title admin-panel-title">
      <Gauge size={18} aria-hidden="true" />
      <h2>Health</h2>
      <div class="health-summary" aria-label="Health summary">
        <span class="health-count" data-state="ok">
          <CheckCircle2 size={15} aria-hidden="true" />
          {okHealthCount}
        </span>
        <span class="health-count" data-state="warn">
          <AlertTriangle size={15} aria-hidden="true" />
          {warnHealthCount}
        </span>
        <span class="health-count" data-state="error">
          <XCircle size={15} aria-hidden="true" />
          {errorHealthCount}
        </span>
      </div>
    </div>
    <div class="health-grid">
      {#each healthChecks as check (check.key)}
        {@const HealthIcon = checkIcon(check)}
        <div class="health-row" data-state={check.state}>
          <HealthIcon size={18} aria-hidden="true" />
          <div class="health-copy">
            <span>{check.label}</span>
            <small>{check.detail}</small>
          </div>
          <strong>{check.state}</strong>
        </div>
      {:else}
        <p class="empty-state">No health checks available.</p>
      {/each}
    </div>
  </article>

  <article class="panel">
    <div class="panel-title">
      <KeyRound size={18} aria-hidden="true" />
      <h2>Tier 1 Edits</h2>
    </div>
    <div class="status-list">
      <div class="status-row">
        <span>Allowed</span>
        <strong data-state={operatorWindowOpen ? "active" : "stopped"}>
          {operatorWindowOpen ? "open" : "closed"}
        </strong>
      </div>
      <div class="status-row">
        <span>Time</span>
        <small>{operatorWindowLabel}</small>
      </div>
      <div class="status-row">
        <span>Window</span>
        <strong data-state={tier1EditUnlocked ? "active" : "stopped"}>
          {tier1EditUnlocked ? "active" : "locked"}
        </strong>
      </div>
      <div class="status-row">
        <span>Expires</span>
        <small>
          {#if tier1EditUnlocked && tier1EditUnlockedUntil}
            {new Date(tier1EditUnlockedUntil).toLocaleTimeString()} ({tier1EditRemainingSeconds}s)
          {:else}
            -
          {/if}
        </small>
      </div>
    </div>
    <div class="tier1-edit-form admin-action-form">
      <label>
        <span>Edit key</span>
        <input
          bind:value={tier1EditPhraseInput}
          autocomplete="off"
          placeholder="Type the Tier 1 edit key"
          spellcheck="false"
        />
      </label>
      <button
        class="primary"
        onclick={onUnlockTier1Edit}
        disabled={tier1EditUnlocking || !canUnlockTier1Edit}
      >
        <KeyRound size={17} aria-hidden="true" />
        <span>{tier1EditUnlocking ? "Unlocking" : "Unlock 5 min"}</span>
      </button>
    </div>
    {#if tier1EditMessage}
      <p class="result-text">{tier1EditMessage}</p>
    {/if}
    {#if tier1EditKeyError}
      <p class="result-text danger-text">{tier1EditKeyError}</p>
    {/if}
  </article>

  <article class="panel">
    <div class="panel-title">
      <Download size={18} aria-hidden="true" />
      <h2>Policy Files</h2>
    </div>
    <div class="policy-file-actions">
      <button class="secondary" onclick={onExportPolicyToml} disabled={policyActionRunning}>
        <Download size={17} aria-hidden="true" />
        <span>{policyExportRunning ? "Exporting" : "Export TOML"}</span>
      </button>
      <button
        class="secondary"
        onclick={onImportPolicyToml}
        disabled={policyActionRunning}
        title="Append TOML"
      >
        <Upload size={17} aria-hidden="true" />
        <span>{policyImportRunning ? "Appending" : "Append TOML"}</span>
      </button>
    </div>
    {#if policyTransferMessage}
      <p class="result-text">{policyTransferMessage}</p>
    {/if}
    {#if policyTransferError}
      <p class="result-text danger-text">{policyTransferError}</p>
    {/if}
  </article>

  <article class="panel">
    <div class="panel-title">
      <Trash2 size={18} aria-hidden="true" />
      <h2>Uninstall</h2>
    </div>
    <div class="uninstall-form admin-action-form">
      <div class="status-list">
        <div class="status-row">
          <span>Allowed</span>
          <strong data-state={operatorWindowOpen ? "active" : "stopped"}>
            {operatorWindowOpen ? "open" : "closed"}
          </strong>
        </div>
        <div class="status-row">
          <span>Time</span>
          <small>{operatorWindowLabel}</small>
        </div>
        <div class="status-row">
          <span>Phrase</span>
          <strong data-state={uninstallPhrase ? "active" : "stopped"}>
            {#if uninstallPhrase}
              ready
            {:else if uninstallPhraseLoading}
              loading
            {:else}
              unavailable
            {/if}
          </strong>
        </div>
        {#if uninstallPhrase}
          <div class="status-row status-row-wide">
            <span>Confirm</span>
            <code class="phrase-code">{uninstallPhrase}</code>
          </div>
        {/if}
      </div>
      <label>
        <span>Confirmation phrase</span>
        <input
          bind:value={uninstallPhraseInput}
          autocomplete="off"
          placeholder="Type an uninstall phrase"
          spellcheck="false"
        />
      </label>
      <button
        class="secondary danger-action"
        onclick={onRunUninstallBlockuntu}
        disabled={uninstallRunning || !canRunUninstall}
      >
        <Trash2 size={17} aria-hidden="true" />
        <span>{uninstallRunning ? "Removing" : "Uninstall"}</span>
      </button>
    </div>
    {#if uninstallPhraseError}
      <p class="result-text danger-text">{uninstallPhraseError}</p>
    {/if}
    {#if uninstallResult}
      <p class="result-text">{uninstallResult.detail}</p>
    {/if}
  </article>
</section>
