<script lang="ts">
  import {
    Activity,
    AlertTriangle,
    CheckCircle2,
    Gauge,
    KeyRound,
    Play,
    Power,
    PowerOff,
    Shield,
    Terminal,
    Trash2,
    XCircle
  } from "@lucide/svelte";
  import type {
    DaemonStatus,
    EnforcementStatus,
    HealthCheck,
    SystemHealth,
    UninstallResult
  } from "../../lib/types";

  type Icon = typeof Activity;

  interface Props {
    status: DaemonStatus | null;
    enforcement: EnforcementStatus | null;
    health: SystemHealth | null;
    enforcementChanging: boolean;
    enforcementMessage: string | null;
    rawMethod?: string;
    rawParams?: string;
    rawResult: string;
    rawRunning: boolean;
    uninstallPhrase: string | null;
    uninstallPhraseInput?: string;
    uninstallRunning: boolean;
    uninstallResult: UninstallResult | null;
    uninstallPhraseError: string | null;
    tier1EditPhraseInput?: string;
    tier1EditUnlocking: boolean;
    tier1EditUnlocked: boolean;
    tier1EditUnlockedUntil: string | null;
    tier1EditRemainingSeconds: number;
    tier1EditMessage: string | null;
    tier1EditKeyError: string | null;
    onStartEnforcement: () => void | Promise<void>;
    onStopEnforcement: () => void | Promise<void>;
    onRunRawRpc: () => void | Promise<void>;
    onRunUninstallBlockuntu: () => void | Promise<void>;
    onUnlockTier1Edit: () => void | Promise<void>;
  }

  let {
    status,
    enforcement,
    health,
    enforcementChanging,
    enforcementMessage,
    rawMethod = $bindable(""),
    rawParams = $bindable(""),
    rawResult,
    rawRunning,
    uninstallPhrase,
    uninstallPhraseInput = $bindable(""),
    uninstallRunning,
    uninstallResult,
    uninstallPhraseError,
    tier1EditPhraseInput = $bindable(""),
    tier1EditUnlocking,
    tier1EditUnlocked,
    tier1EditUnlockedUntil,
    tier1EditRemainingSeconds,
    tier1EditMessage,
    tier1EditKeyError,
    onStartEnforcement,
    onStopEnforcement,
    onRunRawRpc,
    onRunUninstallBlockuntu,
    onUnlockTier1Edit
  }: Props = $props();

  let currentEnforcementState = $derived(
    enforcement?.enforcement_state ?? status?.enforcement_state ?? "unknown"
  );
  let enforcementActive = $derived(currentEnforcementState === "active");
  let canRunUninstall = $derived(Boolean(uninstallPhrase && uninstallPhraseInput.trim()));
  let canUnlockTier1Edit = $derived(Boolean(tier1EditPhraseInput.trim()));

  function checkIcon(check: HealthCheck): Icon {
    if (check.state === "ok") return CheckCircle2;
    if (check.state === "error") return XCircle;
    if (check.state === "warn") return AlertTriangle;
    return Activity;
  }
</script>

<section class="content-grid admin-grid">
  <article class="panel">
    <div class="panel-title">
      <Gauge size={18} aria-hidden="true" />
      <h2>Health</h2>
    </div>
    <div class="health-list">
      {#each health?.checks ?? [] as check (check.key)}
        {@const HealthIcon = checkIcon(check)}
        <div class="health-row" data-state={check.state}>
          <HealthIcon size={18} aria-hidden="true" />
          <span>{check.label}</span>
          <strong>{check.state}</strong>
          <small>{check.detail}</small>
        </div>
      {:else}
        <p class="empty-state">No health checks available.</p>
      {/each}
    </div>
  </article>

  <article class="panel">
    <div class="panel-title">
      <Shield size={18} aria-hidden="true" />
      <h2>Enforcement</h2>
    </div>
    <div class="status-list">
      <div class="status-row">
        <span>Mode</span>
        <strong data-state={currentEnforcementState}>{currentEnforcementState}</strong>
      </div>
      <div class="status-row">
        <span>Firefox policy</span>
        <small>{enforcement?.firefox_policy.path ?? "unknown"}</small>
      </div>
      <div class="status-row">
        <span>Chrome policy</span>
        <small>{enforcement?.chrome_policy.path ?? "unknown"}</small>
      </div>
      <div class="status-row">
        <span>Hosts file</span>
        <small>{enforcement?.hosts_file.path ?? "unknown"}</small>
      </div>
    </div>
    <div class="button-row enforcement-actions">
      <button
        class="primary"
        onclick={onStartEnforcement}
        disabled={enforcementChanging || enforcementActive}
      >
        <Power size={17} aria-hidden="true" />
        <span>Start</span>
      </button>
      <button
        class="secondary danger-action"
        onclick={onStopEnforcement}
        disabled={enforcementChanging || !enforcementActive}
      >
        <PowerOff size={17} aria-hidden="true" />
        <span>Stop</span>
      </button>
    </div>
    {#if enforcementMessage}
      <p class="result-text">{enforcementMessage}</p>
    {/if}
  </article>

  <article class="panel">
    <div class="panel-title">
      <Terminal size={18} aria-hidden="true" />
      <h2>JSON-RPC</h2>
    </div>
    <div class="rpc-form">
      <label>
        <span>Method</span>
        <input bind:value={rawMethod} />
      </label>
      <label>
        <span>Params</span>
        <textarea bind:value={rawParams} spellcheck="false"></textarea>
      </label>
      <button class="primary" onclick={onRunRawRpc} disabled={rawRunning}>
        <Play size={17} aria-hidden="true" />
        <span>Run</span>
      </button>
    </div>
    {#if rawResult}
      <pre>{rawResult}</pre>
    {/if}
  </article>

  <article class="panel">
    <div class="panel-title">
      <KeyRound size={18} aria-hidden="true" />
      <h2>Tier 1 Edits</h2>
    </div>
    <div class="status-list">
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
    <div class="tier1-edit-form">
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
      <Trash2 size={18} aria-hidden="true" />
      <h2>Uninstall</h2>
    </div>
    <div class="uninstall-form">
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
