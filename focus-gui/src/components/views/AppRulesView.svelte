<script lang="ts">
  import { AlertTriangle, Gamepad2, Plus, Save, Trash2 } from "@lucide/svelte";
  import { appMatcherKinds, appRuleIsActive } from "../../lib/ui";
  import type { AppRule, ConfigSnapshot } from "../../lib/types";

  interface Props {
    config: ConfigSnapshot | null;
    appRuleDraft?: AppRule | null;
    appRuleSaving: boolean;
    appRuleMessage: string | null;
    activeDetoxAppRuleIds?: string[];
    onSelectAppRule: (rule: AppRule) => void;
    onStartNewAppRule: () => void;
    onSaveAppRuleDraft: () => void | Promise<void>;
    onRemoveAppRuleDraft: () => void | Promise<void>;
  }

  let {
    config,
    appRuleDraft = $bindable<AppRule | null>(null),
    appRuleSaving,
    appRuleMessage,
    activeDetoxAppRuleIds = [],
    onSelectAppRule,
    onStartNewAppRule,
    onSaveAppRuleDraft,
    onRemoveAppRuleDraft
  }: Props = $props();

  let savedAppRule = $derived(
    appRuleDraft ? (config?.app_rules.find((rule) => rule.id === appRuleDraft?.id) ?? null) : null
  );
  let appRuleDraftIsExisting = $derived(Boolean(savedAppRule));
  let appRuleDraftDetoxLocked = $derived(
    Boolean(savedAppRule && activeDetoxAppRuleIds.includes(savedAppRule.id))
  );
  let appRuleDraftLocked = $derived(
    Boolean(
      savedAppRule &&
        (appRuleDraftDetoxLocked || appRuleIsActive(savedAppRule, config?.schedules ?? []))
    )
  );

  function addAppMatcher(): void {
    if (!appRuleDraft) return;
    appRuleDraft.matchers = [...appRuleDraft.matchers, { kind: "command_name", value: "" }];
  }

  function removeAppMatcher(index: number): void {
    if (!appRuleDraft || appRuleDraft.matchers.length <= 1) return;
    appRuleDraft.matchers = appRuleDraft.matchers.filter(
      (_, matcherIndex) => matcherIndex !== index
    );
  }

  function setAppRuleTier(tier: AppRule["tier"]): void {
    if (!appRuleDraft) return;
    appRuleDraft.tier = tier;
    if (tier === "hard") {
      appRuleDraft.allowance_id = null;
      appRuleDraft.unlock_policy = null;
    }
  }

  function toggleAppRuleSchedule(scheduleId: string): void {
    if (!appRuleDraft) return;
    if (appRuleDraft.schedule_ids.includes(scheduleId)) {
      appRuleDraft.schedule_ids = appRuleDraft.schedule_ids.filter((id) => id !== scheduleId);
    } else {
      appRuleDraft.schedule_ids = [...appRuleDraft.schedule_ids, scheduleId];
    }
  }
</script>

<section class="split-view">
  <article class="panel list-panel">
    <div class="panel-title">
      <Gamepad2 size={18} aria-hidden="true" />
      <h2>App Rules</h2>
    </div>
    <button class="secondary wide-button" onclick={onStartNewAppRule}>
      <Plus size={17} aria-hidden="true" />
      <span>New app</span>
    </button>
    <div class="rule-list">
      {#each config?.app_rules ?? [] as rule (rule.id)}
        <button
          class:active={appRuleDraft?.id === rule.id}
          onclick={() => onSelectAppRule(rule)}
        >
          <span class:hard={rule.tier === "hard"} class="tier-dot"></span>
          <span>{rule.name}</span>
          <em>{rule.tier === "hard" ? "Tier 1" : "Tier 2"}</em>
        </button>
      {:else}
        <p class="empty-state">No app rules reported by the daemon.</p>
      {/each}
    </div>
  </article>

  <article class="panel detail-panel">
    <div class="panel-title">
      <Gamepad2 size={18} aria-hidden="true" />
      <h2>{appRuleDraft?.name || "App rule"}</h2>
    </div>
    {#if appRuleDraft}
      {#if appRuleDraftLocked}
        <section class="inline-warning">
          <AlertTriangle size={17} aria-hidden="true" />
          <span>
            {appRuleDraftDetoxLocked
              ? "This app rule is covered by an active detox session."
              : "This app rule is active right now."}
          </span>
        </section>
      {/if}
      <div class="form-grid">
        <label>
          <span>Name</span>
          <input bind:value={appRuleDraft.name} disabled={appRuleDraftLocked} />
        </label>
        <label>
          <span>Tier</span>
          <select
            value={appRuleDraft.tier}
            disabled={appRuleDraftLocked}
            onchange={(event) => setAppRuleTier(event.currentTarget.value as AppRule["tier"])}
          >
            <option value="hard">Tier 1</option>
            <option value="controlled_access">Tier 2</option>
          </select>
        </label>
      </div>

      <label class="check-row">
        <input
          type="checkbox"
          bind:checked={appRuleDraft.enabled}
          disabled={appRuleDraftLocked}
        />
        <span>Enabled</span>
      </label>

      <div class="section-label">Schedules</div>
      <div class="chip-grid">
        {#each config?.schedules ?? [] as schedule (schedule.id)}
          <label class="chip-check">
            <input
              type="checkbox"
              checked={appRuleDraft.schedule_ids.includes(schedule.id)}
              disabled={appRuleDraftLocked}
              onchange={() => toggleAppRuleSchedule(schedule.id)}
            />
            <span>{schedule.name ?? schedule.id}</span>
          </label>
        {:else}
          <p class="empty-state">No schedules available.</p>
        {/each}
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Matcher</th>
              <th>Value</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each appRuleDraft.matchers as matcher, index (matcher)}
              <tr>
                <td>
                  <select bind:value={matcher.kind} disabled={appRuleDraftLocked}>
                    {#each appMatcherKinds as kind (kind.id)}
                      <option value={kind.id}>{kind.label}</option>
                    {/each}
                  </select>
                </td>
                <td>
                  <input bind:value={matcher.value} disabled={appRuleDraftLocked} />
                </td>
                <td>
                  <button
                    class="icon-button"
                    title="Remove matcher"
                    onclick={() => removeAppMatcher(index)}
                    disabled={appRuleDraftLocked || appRuleDraft.matchers.length <= 1}
                  >
                    <Trash2 size={16} aria-hidden="true" />
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="button-row">
        <button class="secondary" onclick={addAppMatcher} disabled={appRuleDraftLocked}>
          <Plus size={17} aria-hidden="true" />
          <span>Matcher</span>
        </button>
        <button
          class="secondary"
          onclick={onRemoveAppRuleDraft}
          disabled={appRuleSaving || appRuleDraftLocked || !appRuleDraftIsExisting}
        >
          <Trash2 size={17} aria-hidden="true" />
          <span>Delete</span>
        </button>
        <button
          class="primary"
          onclick={onSaveAppRuleDraft}
          disabled={appRuleSaving || appRuleDraftLocked}
        >
          <Save size={17} aria-hidden="true" />
          <span>Save</span>
        </button>
      </div>
      {#if appRuleMessage}
        <p class="result-text">{appRuleMessage}</p>
      {/if}
    {/if}
  </article>
</section>
