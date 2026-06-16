<script lang="ts">
  import { Ban, CheckCircle2, Play, Timer, XCircle } from "@lucide/svelte";
  import type { AppRule, ConfigSnapshot, DetoxSession, Rule } from "../../lib/types";

  interface Props {
    config: ConfigSnapshot | null;
    detoxSessions: DetoxSession[];
    detoxName?: string;
    detoxDurationMinutes?: number;
    selectedSiteRuleIds?: string[];
    selectedAppRuleIds?: string[];
    detoxStarting: boolean;
    detoxCancellingId: string | null;
    detoxMessage: string | null;
    tier1EditUnlocked: boolean;
    nowMs: number;
    onStartDetox: () => void | Promise<void>;
    onCancelDetox: (id: string) => void | Promise<void>;
  }

  let {
    config,
    detoxSessions,
    detoxName = $bindable("Deep work"),
    detoxDurationMinutes = $bindable(60),
    selectedSiteRuleIds = $bindable<string[]>([]),
    selectedAppRuleIds = $bindable<string[]>([]),
    detoxStarting,
    detoxCancellingId,
    detoxMessage,
    tier1EditUnlocked,
    nowMs,
    onStartDetox,
    onCancelDetox
  }: Props = $props();

  const durationPresets = [
    { label: "30m", minutes: 30 },
    { label: "1h", minutes: 60 },
    { label: "2h", minutes: 120 },
    { label: "4h", minutes: 240 }
  ];

  let enabledSiteRules = $derived(config?.rules.filter((rule) => rule.enabled) ?? []);
  let enabledAppRules = $derived(config?.app_rules.filter((rule) => rule.enabled) ?? []);
  let activeSessions = $derived(
    detoxSessions.filter(
      (session) =>
        session.status === "active" &&
        !session.cancelled_at &&
        Date.parse(session.ends_at) > nowMs
    )
  );
  let inactiveSessions = $derived(
    detoxSessions.filter((session) => !activeSessions.some((active) => active.id === session.id))
  );
  let selectedTargetCount = $derived(selectedSiteRuleIds.length + selectedAppRuleIds.length);
  let canStartDetox = $derived(
    selectedTargetCount > 0 && Number.isFinite(detoxDurationMinutes) && detoxDurationMinutes > 0
  );

  function toggleSiteRule(ruleId: string): void {
    selectedSiteRuleIds = toggleId(selectedSiteRuleIds, ruleId);
  }

  function toggleAppRule(ruleId: string): void {
    selectedAppRuleIds = toggleId(selectedAppRuleIds, ruleId);
  }

  function toggleId(values: string[], id: string): string[] {
    return values.includes(id) ? values.filter((value) => value !== id) : [...values, id];
  }

  function ruleLabel(rule: Rule | AppRule): string {
    return rule.name || rule.id;
  }

  function sessionTitle(session: DetoxSession): string {
    return session.name?.trim() || session.id;
  }

  function formatTime(value: string): string {
    return new Date(value).toLocaleTimeString();
  }

  function formatDateTime(value: string): string {
    return new Date(value).toLocaleString();
  }

  function formatRemaining(session: DetoxSession): string {
    const remainingSeconds = Math.max(0, Math.ceil((Date.parse(session.ends_at) - nowMs) / 1000));
    const hours = Math.floor(remainingSeconds / 3600);
    const minutes = Math.floor((remainingSeconds % 3600) / 60);
    const seconds = remainingSeconds % 60;
    if (hours > 0) return `${hours}h ${minutes}m`;
    if (minutes > 0) return `${minutes}m ${seconds}s`;
    return `${seconds}s`;
  }

  function targetSummary(session: DetoxSession): string {
    const siteCount = session.site_rule_ids.length;
    const appCount = session.app_rule_ids.length;
    const parts = [];
    if (siteCount > 0) parts.push(`${siteCount} site ${siteCount === 1 ? "list" : "lists"}`);
    if (appCount > 0) parts.push(`${appCount} app ${appCount === 1 ? "rule" : "rules"}`);
    return parts.join(", ");
  }
</script>

<section class="content-grid detox-grid">
  <article class="panel detox-start-panel">
    <div class="panel-title">
      <Timer size={18} aria-hidden="true" />
      <h2>Start Detox</h2>
    </div>

    <div class="form-grid detox-form">
      <label>
        <span>Name</span>
        <input bind:value={detoxName} />
      </label>
      <label>
        <span>Minutes</span>
        <input type="number" min="1" max="10080" step="5" bind:value={detoxDurationMinutes} />
      </label>
    </div>

    <div class="preset-row" aria-label="Duration presets">
      {#each durationPresets as preset (preset.minutes)}
        <button
          class:active={detoxDurationMinutes === preset.minutes}
          class="secondary"
          onclick={() => (detoxDurationMinutes = preset.minutes)}
        >
          <span>{preset.label}</span>
        </button>
      {/each}
    </div>

    <div class="section-label">Site lists</div>
    <div class="chip-grid detox-chip-grid">
      {#each enabledSiteRules as rule (rule.id)}
        <label class="chip-check">
          <input
            type="checkbox"
            checked={selectedSiteRuleIds.includes(rule.id)}
            onchange={() => toggleSiteRule(rule.id)}
          />
          <span>{ruleLabel(rule)}</span>
        </label>
      {:else}
        <p class="empty-state">No enabled site lists.</p>
      {/each}
    </div>

    <div class="section-label">App rules</div>
    <div class="chip-grid detox-chip-grid">
      {#each enabledAppRules as rule (rule.id)}
        <label class="chip-check">
          <input
            type="checkbox"
            checked={selectedAppRuleIds.includes(rule.id)}
            onchange={() => toggleAppRule(rule.id)}
          />
          <span>{ruleLabel(rule)}</span>
        </label>
      {:else}
        <p class="empty-state">No enabled app rules.</p>
      {/each}
    </div>

    <div class="button-row detox-action-row">
      <button class="primary" onclick={onStartDetox} disabled={detoxStarting || !canStartDetox}>
        <Play size={17} aria-hidden="true" />
        <span>{detoxStarting ? "Starting" : "Start"}</span>
      </button>
      <span>{selectedTargetCount} selected</span>
    </div>
    {#if detoxMessage}
      <p class="result-text">{detoxMessage}</p>
    {/if}
  </article>

  <article class="panel detox-active-panel">
    <div class="panel-title">
      <Ban size={18} aria-hidden="true" />
      <h2>Active Detox</h2>
    </div>

    <div class="detox-session-list">
      {#each activeSessions as session (session.id)}
        <div class="detox-session-row" data-state="active">
          <CheckCircle2 size={18} aria-hidden="true" />
          <div class="detox-session-copy">
            <strong>{sessionTitle(session)}</strong>
            <span>{targetSummary(session)}</span>
          </div>
          <div class="detox-session-meta">
            <strong>{formatRemaining(session)}</strong>
            <small>until {formatTime(session.ends_at)}</small>
          </div>
          <button
            class="secondary danger-action"
            onclick={() => onCancelDetox(session.id)}
            disabled={!tier1EditUnlocked || detoxCancellingId === session.id}
          >
            <XCircle size={17} aria-hidden="true" />
            <span>{detoxCancellingId === session.id ? "Cancelling" : "Cancel"}</span>
          </button>
        </div>
      {:else}
        <p class="empty-state">No active detox sessions.</p>
      {/each}
    </div>
  </article>

  <article class="panel wide-panel">
    <div class="panel-title">
      <Timer size={18} aria-hidden="true" />
      <h2>Recent Detox</h2>
    </div>

    <div class="detox-history-list">
      {#each inactiveSessions.slice(0, 8) as session (session.id)}
        <div class="detox-history-row" data-state={session.status}>
          <span>{sessionTitle(session)}</span>
          <small>{targetSummary(session)}</small>
          <strong>{session.status}</strong>
          <time datetime={session.ends_at}>{formatDateTime(session.ends_at)}</time>
        </div>
      {:else}
        <p class="empty-state">No detox history.</p>
      {/each}
    </div>
  </article>
</section>
