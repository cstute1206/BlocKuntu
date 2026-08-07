<script lang="ts">
  import {
    CheckCircle2,
    Play,
    Search,
    Server,
    Shield,
    Unlock,
    Wrench,
    XCircle
  } from "@lucide/svelte";
  import { openExtensionStore } from "../../lib/api";
  import type {
    ConfigSnapshot,
    DaemonStatus,
    DecisionResult,
    SystemHealth,
    UnlockResult
  } from "../../lib/types";

  interface Props {
    status: DaemonStatus | null;
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
    tier1EditKey: string | null;
    onDismissFirstRunOverview: () => void;
    onRunUrlCheck: () => void | Promise<void>;
    onRunUnlock: () => void | Promise<void>;
  }

  let {
    status,
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
    tier1EditKey,
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
  let extensionStoreError = $state<string | null>(null);

  let hardRules = $derived(config?.rules.filter((rule) => rule.tier === "hard") ?? []);
  let controlledRules = $derived(
    config?.rules.filter((rule) => rule.tier === "controlled_access") ?? []
  );
  let scheduledRules = $derived(
    config?.rules.filter((rule) => rule.tier === "scheduled_block") ?? []
  );
  let hardAppRules = $derived(config?.app_rules.filter((rule) => rule.tier === "hard") ?? []);
  let controlledAppRules = $derived(
    config?.app_rules.filter((rule) => rule.tier === "controlled_access") ?? []
  );
  let scheduledAppRules = $derived(
    config?.app_rules.filter((rule) => rule.tier === "scheduled_block") ?? []
  );
  let hardBlockCount = $derived(hardRules.length + hardAppRules.length);
  let scheduledBlockCount = $derived(scheduledRules.length + scheduledAppRules.length);
  let controlledBlockCount = $derived(controlledRules.length + controlledAppRules.length);
  let failingChecks = $derived(
    health?.checks.filter((check) => check.state === "error" || check.state === "warn") ?? []
  );
  function submitUrlCheckOnEnter(event: KeyboardEvent): void {
    if (event.key !== "Enter" || event.isComposing || urlChecking) return;
    event.preventDefault();
    void onRunUrlCheck();
  }

  function submitUnlockOnEnter(event: KeyboardEvent): void {
    if (event.key !== "Enter" || event.isComposing || !canUnlock) return;
    event.preventDefault();
    void onRunUnlock();
  }

  async function openExtensionStoreLink(event: MouseEvent, url: string): Promise<void> {
    event.preventDefault();
    extensionStoreError = null;

    try {
      await openExtensionStore(url);
    } catch {
      extensionStoreError = "Unable to open the extension store in your default browser.";
    }
  }
</script>

{#if showFirstRunOverview}
  <div class="onboarding-backdrop">
    <div
      class="onboarding-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="onboarding-title"
      tabindex="-1"
    >
      <div class="onboarding-modal-header">
        <div class="panel-title">
          <Shield size={18} aria-hidden="true" />
          <h2 id="onboarding-title">Welcome to BlocKuntu</h2>
        </div>
        <button class="icon-button" title="Close introduction" onclick={onDismissFirstRunOverview}>
          <XCircle size={17} aria-hidden="true" />
        </button>
      </div>
      <div class="onboarding-copy">
        <p>BlocKuntu applies the rules you configure. Before adding your first list, keep these rules in mind:</p>
        <ul>
          <li><strong>Tier 1</strong> is always blocked. Edit it only with the Tier 1 unlock.</li>
          <li><strong>Tier 2</strong> blocks strictly during an attached schedule or Detox and cannot be manually unlocked.</li>
          <li><strong>Tier 3</strong> is also active during a schedule or Detox, but retains its daily allowance and manual unlock.</li>
          <li>Tier 2 and Tier 3 lists need a schedule or a Detox session to become active.</li>
          <li>
            Install the browser extension for browser blocking:
            <a
              href="https://addons.mozilla.org/en-US/firefox/addon/blockuntu/"
              target="_blank"
              rel="noreferrer"
              onclick={(event) => void openExtensionStoreLink(event, "https://addons.mozilla.org/en-US/firefox/addon/blockuntu/")}
            >Firefox Add-ons</a>
            or
            <a
              href="https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc"
              target="_blank"
              rel="noreferrer"
              onclick={(event) => void openExtensionStoreLink(event, "https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc")}
            >Chrome Web Store</a> for Chrome, Chromium, Brave, Opera, Microsoft Edge, or Vivaldi. In Opera and Edge, first turn on “Allow extensions from other stores”; in Vivaldi, enable Web Store in Settings → Privacy and Security → Google Extensions. The Firefox Add-ons extension also supports LibreWolf and Waterfox.
          </li>
        </ul>
        {#if extensionStoreError}
          <p class="danger-text">{extensionStoreError}</p>
        {/if}
        <div class="onboarding-credentials">
          {#if uninstallPhrase && tier1EditKey}
            <p><strong>Recovery uninstall phrase</strong> — store this somewhere secure.</p>
            <code class="phrase-code">{uninstallPhrase}</code>
            <p><strong>Tier 1 edit key</strong> — required to unlock Tier 1 edits; store it somewhere secure.</p>
            <code class="phrase-code">{tier1EditKey}</code>
          {:else}
            <p>Recovery credentials have been hidden and removed from this device.</p>
          {/if}
        </div>
        <div class="button-row onboarding-actions">
          <button class="primary" onclick={onDismissFirstRunOverview}>Get started</button>
        </div>
      </div>
    </div>
  </div>
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
      <span class="metric-value warn">{scheduledBlockCount}</span>
      <span>scheduled strict</span>
    </div>
    <div class="metric-line">
      <span class="metric-value accent">{controlledBlockCount}</span>
      <span>controlled access</span>
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
      <input
        bind:value={testUrl}
        placeholder="example.com or https://example.com/"
        onkeydown={submitUrlCheckOnEnter}
      />
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
      Manual unlocks only apply to active Tier 3 rules, including Tier 3 activated by Detox. Tier 1
      and Tier 2 blocks cannot be bypassed.
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
          onkeydown={submitUnlockOnEnter}
        />
      </label>
      <button class="primary" onclick={onRunUnlock} disabled={!canUnlock}>
        <Unlock size={17} aria-hidden="true" />
        <span>Unlock</span>
      </button>
    </div>
    {#if unlockResult}
      <p class="result-text">
        Manual unlock granted for {unlockResult.target} for {unlockResult.minutes} minutes, until
        {new Date(unlockResult.expires_at).toLocaleString()}. Reason: {unlockResult.reason}
      </p>
    {/if}
  </article>
</section>
