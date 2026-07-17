<script lang="ts">
  import { Ban, CheckCircle2, Play, Timer, XCircle } from "@lucide/svelte";
  import type {
    AppRule,
    ConfigSnapshot,
    DetoxDurationUnit,
    DetoxSession,
    Rule
  } from "../../lib/types";

  const MAX_DETOX_DURATION_MINUTES = 12 * 7 * 24 * 60;
  const durationMultipliers: Record<DetoxDurationUnit, number> = {
    minutes: 1,
    hours: 60,
    days: 24 * 60,
    weeks: 7 * 24 * 60
  };

  interface Props {
    config: ConfigSnapshot | null;
    detoxSessions: DetoxSession[];
    detoxName?: string;
    detoxDurationValue?: number;
    detoxDurationUnit?: DetoxDurationUnit;
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
    detoxDurationValue = $bindable(1),
    detoxDurationUnit = $bindable<DetoxDurationUnit>("hours"),
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
    { label: "1 hour", value: 1, unit: "hours" },
    { label: "4 hours", value: 4, unit: "hours" },
    { label: "1 day", value: 1, unit: "days" },
    { label: "3 days", value: 3, unit: "days" },
    { label: "1 week", value: 1, unit: "weeks" },
    { label: "2 weeks", value: 2, unit: "weeks" },
    { label: "4 weeks", value: 4, unit: "weeks" }
  ] satisfies Array<{ label: string; value: number; unit: DetoxDurationUnit }>;

  const durationUnitOptions: Array<{ label: string; value: DetoxDurationUnit }> = [
    { label: "Minutes", value: "minutes" },
    { label: "Hours", value: "hours" },
    { label: "Days", value: "days" },
    { label: "Weeks", value: "weeks" }
  ];

  let detoxSiteRules = $derived(
    config?.rules.filter((rule) => rule.tier !== "hard") ?? []
  );
  let detoxAppRules = $derived(
    config?.app_rules.filter((rule) => rule.tier !== "hard") ?? []
  );
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
  let detoxDurationMinutes = $derived(
    Number(detoxDurationValue ?? 0) * durationMultipliers[detoxDurationUnit]
  );
  let maximumDurationValue = $derived(
    Math.floor(MAX_DETOX_DURATION_MINUTES / durationMultipliers[detoxDurationUnit])
  );
  let plannedEnd = $derived(
    Number.isFinite(detoxDurationMinutes) && detoxDurationMinutes > 0
      ? new Date(nowMs + detoxDurationMinutes * 60_000)
      : null
  );
  let canStartDetox = $derived(
    selectedTargetCount > 0 &&
      Number.isFinite(detoxDurationMinutes) &&
      detoxDurationMinutes >= 1 &&
      detoxDurationMinutes <= MAX_DETOX_DURATION_MINUTES
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

  function selectDurationPreset(value: number, unit: DetoxDurationUnit): void {
    detoxDurationValue = value;
    detoxDurationUnit = unit;
  }

  function ruleLabel(rule: Rule | AppRule): string {
    const tier = rule.tier === "scheduled_block" ? "Tier 2" : "Tier 3";
    return `${rule.name || rule.id} (${tier})`;
  }

  function sessionTitle(session: DetoxSession): string {
    return session.name?.trim() || session.id;
  }

  function formatDateTime(value: string): string {
    return new Date(value).toLocaleString();
  }

  function formatRemaining(session: DetoxSession): string {
    const remainingSeconds = Math.max(0, Math.ceil((Date.parse(session.ends_at) - nowMs) / 1000));
    const days = Math.floor(remainingSeconds / 86_400);
    const hours = Math.floor(remainingSeconds / 3600);
    const minutes = Math.floor((remainingSeconds % 3600) / 60);
    const seconds = remainingSeconds % 60;
    if (days >= 7) return `${Math.floor(days / 7)}w ${days % 7}d`;
    if (days > 0) return `${days}d ${hours % 24}h`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    if (minutes > 0) return `${minutes}m ${seconds}s`;
    return `${seconds}s`;
  }

  function targetSummary(session: DetoxSession): string {
    const siteCount = session.site_rule_ids.length;
    const appCount = session.app_rule_ids.length;
    const parts = [];
    if (siteCount > 0) parts.push(`${siteCount} ${siteCount === 1 ? "website" : "websites"}`);
    if (appCount > 0)
      parts.push(`${appCount} ${appCount === 1 ? "application" : "applications"}`);
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
      <fieldset class="duration-field">
        <legend>Duration</legend>
        <div class="duration-input-row">
          <input
            aria-label="Detox duration"
            type="number"
            min="1"
            max={maximumDurationValue}
            step="1"
            bind:value={detoxDurationValue}
          />
          <select aria-label="Detox duration unit" bind:value={detoxDurationUnit}>
            {#each durationUnitOptions as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </div>
      </fieldset>
    </div>

    <div class="preset-row" aria-label="Duration presets">
      {#each durationPresets as preset (`${preset.value}-${preset.unit}`)}
        <button
          class:active={detoxDurationValue === preset.value && detoxDurationUnit === preset.unit}
          class="secondary"
          onclick={() => selectDurationPreset(preset.value, preset.unit)}
        >
          <span>{preset.label}</span>
        </button>
      {/each}
    </div>
    <p class:danger-text={detoxDurationMinutes > MAX_DETOX_DURATION_MINUTES} class="policy-note">
      {#if detoxDurationMinutes > MAX_DETOX_DURATION_MINUTES}
        Detox can run for at most 12 weeks.
      {:else if plannedEnd}
        Ends {plannedEnd.toLocaleString()}. Tier 2 stays strict; Tier 3 keeps its allowance and
        manual unlock.
      {:else}
        Choose a duration from one minute to 12 weeks.
      {/if}
    </p>

    <div class="section-label">Websites</div>
    <div class="chip-grid detox-chip-grid">
      {#each detoxSiteRules as rule (rule.id)}
        <label class="chip-check">
          <input
            type="checkbox"
            checked={selectedSiteRuleIds.includes(rule.id)}
            onchange={() => toggleSiteRule(rule.id)}
          />
          <span>{ruleLabel(rule)}</span>
        </label>
      {:else}
        <p class="empty-state">No Tier 2 or Tier 3 websites.</p>
      {/each}
    </div>

    <div class="section-label">Applications</div>
    <div class="chip-grid detox-chip-grid">
      {#each detoxAppRules as rule (rule.id)}
        <label class="chip-check">
          <input
            type="checkbox"
            checked={selectedAppRuleIds.includes(rule.id)}
            onchange={() => toggleAppRule(rule.id)}
          />
          <span>{ruleLabel(rule)}</span>
        </label>
      {:else}
        <p class="empty-state">No Tier 2 or Tier 3 applications.</p>
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
            <small>until {formatDateTime(session.ends_at)}</small>
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
