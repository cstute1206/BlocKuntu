<script lang="ts">
  import {
    AlertTriangle,
    CircleMinus,
    CheckCircle2,
    Clock3,
    Play,
    Search,
    Server,
    Shield,
    Unlock,
    Wrench,
    XCircle
  } from "@lucide/svelte";
  import type {
    ConfigSnapshot,
    DaemonStatus,
    DecisionResult,
    EnforcementStatus,
    HealthCheck,
    SystemHealth,
    UnlockResult
  } from "../../lib/types";

  type BrowserSetupState = "ok" | "inactive" | "pending" | "warn" | "error" | "unknown";

  interface Props {
    status: DaemonStatus | null;
    enforcement: EnforcementStatus | null;
    health: SystemHealth | null;
    config: ConfigSnapshot | null;
    showFirstRunOverview: boolean;
    testUrl?: string;
    urlDecision: DecisionResult | null;
    urlChecking: boolean;
    unlockTarget?: string;
    unlockReason?: string;
    unlockResult: UnlockResult | null;
    unlocking: boolean;
    uninstallPhrase: string | null;
    uninstallPhraseLoading: boolean;
    uninstallPhraseError: string | null;
    tier1EditKey: string | null;
    tier1EditKeyLoading: boolean;
    tier1EditKeyError: string | null;
    onDismissFirstRunOverview: () => void;
    onRunUrlCheck: () => void | Promise<void>;
    onRunUnlock: () => void | Promise<void>;
  }

  let {
    status,
    enforcement,
    health,
    config,
    showFirstRunOverview,
    testUrl = $bindable(""),
    urlDecision,
    urlChecking,
    unlockTarget = $bindable(""),
    unlockReason = $bindable(""),
    unlockResult,
    unlocking,
    uninstallPhrase,
    uninstallPhraseLoading,
    uninstallPhraseError,
    tier1EditKey,
    tier1EditKeyLoading,
    tier1EditKeyError,
    onDismissFirstRunOverview,
    onRunUrlCheck,
    onRunUnlock
  }: Props = $props();

  let unlockReasonLetterCount = $derived(
    [...unlockReason].filter((character) => /\p{L}/u.test(character)).length
  );
  let canUnlock = $derived(
    Boolean(unlockTarget.trim() && unlockReasonLetterCount >= 20 && !unlocking)
  );

  let hardRules = $derived(config?.rules.filter((rule) => rule.tier === "hard") ?? []);
  let controlledRules = $derived(
    config?.rules.filter((rule) => rule.tier === "controlled_access") ?? []
  );
  let hardAppRules = $derived(config?.app_rules.filter((rule) => rule.tier === "hard") ?? []);
  let controlledAppRules = $derived(
    config?.app_rules.filter((rule) => rule.tier === "controlled_access") ?? []
  );
  let hardBlockCount = $derived(hardRules.length + hardAppRules.length);
  let controlledBlockCount = $derived(controlledRules.length + controlledAppRules.length);
  let failingChecks = $derived(
    health?.checks.filter((check) => check.state === "error" || check.state === "warn") ?? []
  );
  let firefoxExtensionCheck = $derived(
    health?.checks.find((check) => check.key === "firefox_extension") ?? null
  );
  let chromeExtensionCheck = $derived(
    health?.checks.find((check) => check.key === "chrome_extension") ?? null
  );
  let showFirefoxSetup = $derived(Boolean(firefoxExtensionCheck));
  let showChromeSetup = $derived(Boolean(chromeExtensionCheck));
  let firefoxSetupState = $derived(browserSetupState(firefoxExtensionCheck));
  let chromeSetupState = $derived(browserSetupState(chromeExtensionCheck));
  let firefoxPolicyPending = $derived(
    Boolean(
      enforcement?.firefox_policy.deferred_until_heartbeat &&
        !enforcement.firefox_policy.active_after_heartbeat
    )
  );
  let chromePolicyPending = $derived(
    Boolean(
      enforcement?.chrome_policy.deferred_until_heartbeat &&
        !enforcement.chrome_policy.active_after_heartbeat
    )
  );
  let showFirefoxPolicy = $derived(
    Boolean(health?.checks.some((check) => check.key === "firefox_policy"))
  );
  let showChromePolicy = $derived(
    Boolean(health?.checks.some((check) => check.key === "chrome_policy"))
  );

  function browserSetupState(check: HealthCheck | null): BrowserSetupState {
    if (!check) return "unknown";
    return check.state;
  }

  function setupStateLabel(state: BrowserSetupState): string {
    if (state === "ok") return "Connected";
    if (state === "inactive") return "Browser closed";
    if (state === "pending") return "Starting";
    if (state === "error") return "Needs attention";
    if (state === "warn") return "Install extension";
    return "Checking";
  }
</script>

{#if showFirstRunOverview}
  <section class="setup-panel" aria-label="First run setup">
    <div class="setup-panel-header">
      <div class="panel-title">
        <Shield size={18} aria-hidden="true" />
        <h2>First Run</h2>
      </div>
      <button
        class="icon-button"
        title="Dismiss"
        onclick={onDismissFirstRunOverview}
        disabled={!uninstallPhrase}
      >
        <XCircle size={17} aria-hidden="true" />
      </button>
    </div>
    <div class="setup-grid">
      {#if showFirefoxSetup}
        <div class="setup-row" data-state={firefoxSetupState}>
          {#if firefoxSetupState === "ok"}
            <CheckCircle2 size={18} aria-hidden="true" />
          {:else if firefoxSetupState === "inactive"}
            <CircleMinus size={18} aria-hidden="true" />
          {:else if firefoxSetupState === "pending"}
            <Clock3 size={18} aria-hidden="true" />
          {:else if firefoxSetupState === "error"}
            <XCircle size={18} aria-hidden="true" />
          {:else}
            <AlertTriangle size={18} aria-hidden="true" />
          {/if}
          <span>Firefox extension</span>
          <strong>{setupStateLabel(firefoxSetupState)}</strong>
          <small>
            {firefoxExtensionCheck?.detail ??
              "No heartbeat yet. Install and enable the BlocKuntu Firefox extension."}
          </small>
        </div>
      {/if}

      {#if showChromeSetup}
        <div class="setup-row" data-state={chromeSetupState}>
          {#if chromeSetupState === "ok"}
            <CheckCircle2 size={18} aria-hidden="true" />
          {:else if chromeSetupState === "inactive"}
            <CircleMinus size={18} aria-hidden="true" />
          {:else if chromeSetupState === "pending"}
            <Clock3 size={18} aria-hidden="true" />
          {:else if chromeSetupState === "error"}
            <XCircle size={18} aria-hidden="true" />
          {:else}
            <AlertTriangle size={18} aria-hidden="true" />
          {/if}
          <span>Chrome extension</span>
          <strong>{setupStateLabel(chromeSetupState)}</strong>
          <small>
            {chromeExtensionCheck?.detail ??
              "No heartbeat yet. Install and enable the BlocKuntu Chrome extension."}
          </small>
        </div>
      {/if}

      {#if showFirefoxPolicy}
        <div class="setup-row" data-state={firefoxPolicyPending ? "warn" : "ok"}>
          {#if firefoxPolicyPending}
            <AlertTriangle size={18} aria-hidden="true" />
          {:else}
            <CheckCircle2 size={18} aria-hidden="true" />
          {/if}
          <span>Firefox policy</span>
          <strong>{firefoxPolicyPending ? "Deferred" : "Ready"}</strong>
          <small>
            {firefoxPolicyPending
              ? "Managed policy will be written after the first Firefox extension heartbeat."
              : (enforcement?.firefox_policy.detail ?? "Waiting for daemon status.")}
          </small>
        </div>
      {/if}

      {#if showChromePolicy}
        <div class="setup-row" data-state={chromePolicyPending ? "warn" : "ok"}>
          {#if chromePolicyPending}
            <AlertTriangle size={18} aria-hidden="true" />
          {:else}
            <CheckCircle2 size={18} aria-hidden="true" />
          {/if}
          <span>Chrome policy</span>
          <strong>{chromePolicyPending ? "Deferred" : "Ready"}</strong>
          <small>
            {chromePolicyPending
              ? "Managed policy will be written after the first Chrome extension heartbeat."
              : (enforcement?.chrome_policy.detail ?? "Waiting for daemon status.")}
          </small>
        </div>
      {/if}

      <div class="setup-row setup-row-wide" data-state={uninstallPhrase ? "ok" : "warn"}>
        {#if uninstallPhrase}
          <CheckCircle2 size={18} aria-hidden="true" />
        {:else}
          <AlertTriangle size={18} aria-hidden="true" />
        {/if}
        <span>Uninstall phrase</span>
        <strong>{uninstallPhrase ? "Created" : "Unavailable"}</strong>
        <small>
          {#if uninstallPhrase}
            <code class="phrase-code">{uninstallPhrase}</code>
          {:else if uninstallPhraseLoading}
            Creating confirmation phrase.
          {:else}
            {uninstallPhraseError ?? "Confirmation phrase could not be created."}
          {/if}
        </small>
      </div>

      <div class="setup-row setup-row-wide" data-state={tier1EditKey ? "ok" : "warn"}>
        {#if tier1EditKey}
          <CheckCircle2 size={18} aria-hidden="true" />
        {:else}
          <AlertTriangle size={18} aria-hidden="true" />
        {/if}
        <span>Tier 1 edit key</span>
        <strong>{tier1EditKey ? "Created" : "Unavailable"}</strong>
        <small>
          {#if tier1EditKey}
            <code class="phrase-code">{tier1EditKey}</code>
          {:else if tier1EditKeyLoading}
            Loading edit key.
          {:else}
            {tier1EditKeyError ?? "Tier 1 edit key could not be loaded."}
          {/if}
        </small>
      </div>
    </div>
  </section>
{/if}

<section class="dashboard-grid">
  <article class="panel metric-panel">
    <div class="panel-title">
      <Server size={18} aria-hidden="true" />
      <h2>Daemon</h2>
    </div>
    <div class="metric-line">
      <span class="metric-value">{status?.rules ?? "?"}</span>
      <span>websites</span>
    </div>
    <div class="metric-line">
      <span class="metric-value">{status?.app_rules ?? "?"}</span>
      <span>applications</span>
    </div>
    <div class="metric-line">
      <span class="metric-value">{status?.schedules ?? "?"}</span>
      <span>schedules</span>
    </div>
    <div class="metric-line">
      <span class="metric-value">{status?.allowances ?? "?"}</span>
      <span>allowances</span>
    </div>
  </article>

  <article class="panel metric-panel">
    <div class="panel-title">
      <Shield size={18} aria-hidden="true" />
      <h2>Block Tiers</h2>
    </div>
    <div class="metric-line">
      <span class="metric-value danger">{hardBlockCount}</span>
      <span>hard blocks</span>
    </div>
    <div class="metric-line">
      <span class="metric-value accent">{controlledBlockCount}</span>
      <span>controlled</span>
    </div>
  </article>

  <article class="panel metric-panel">
    <div class="panel-title">
      <Wrench size={18} aria-hidden="true" />
      <h2>System</h2>
    </div>
    <div class="metric-line">
      <span class="metric-value">{health?.checks.length ?? "?"}</span>
      <span>checks</span>
    </div>
    <div class="metric-line">
      <span class="metric-value warn">{failingChecks.length}</span>
      <span>warnings</span>
    </div>
  </article>
</section>

<section class="content-grid">
  <article class="panel">
    <div class="panel-title">
      <Search size={18} aria-hidden="true" />
      <h2>URL Probe</h2>
    </div>
    <div class="input-row">
      <input bind:value={testUrl} placeholder="https://example.com/" />
      <button class="primary" onclick={onRunUrlCheck} disabled={urlChecking}>
        <Play size={17} aria-hidden="true" />
        <span>Check</span>
      </button>
    </div>
    {#if urlDecision}
      <div class:blocked={urlDecision.decision === "block"} class="decision-row">
        {#if urlDecision.decision === "block"}
          <XCircle size={18} aria-hidden="true" />
          <span>{urlDecision.reason?.kind ?? "blocked"}</span>
        {:else}
          <CheckCircle2 size={18} aria-hidden="true" />
          <span>allowed</span>
        {/if}
      </div>
    {/if}
  </article>

  <article class="panel">
    <div class="panel-title">
      <Unlock size={18} aria-hidden="true" />
      <h2>Manual Unlock</h2>
    </div>
    <p class="policy-note">
      Manual unlocks only apply to Tier 2 rules. Active Detox sessions and Tier 1 blocks cannot be
      bypassed.
    </p>
    <div class="unlock-grid">
      <label>
        <span>Target</span>
        <input bind:value={unlockTarget} />
      </label>
      <label class="reason-field">
        <span>Reason ({unlockReasonLetterCount}/20 letters)</span>
        <input
          bind:value={unlockReason}
          placeholder="Describe why this access is necessary"
          autocomplete="off"
        />
      </label>
      <button class="primary" onclick={onRunUnlock} disabled={!canUnlock}>
        <Unlock size={17} aria-hidden="true" />
        <span>Unlock</span>
      </button>
    </div>
    {#if unlockResult}
      <p class="result-text">
        Active until {new Date(unlockResult.expires_at).toLocaleTimeString()} for
        {unlockResult.target}
      </p>
    {/if}
  </article>
</section>
