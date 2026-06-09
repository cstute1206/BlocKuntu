<script lang="ts">
  import { AlertTriangle, ListChecks, Plus, Save, Shield, Trash2 } from "@lucide/svelte";
  import {
    defaultAllowanceForRule,
    patternKinds,
    ruleIsActive
  } from "../../lib/ui";
  import type { Allowance, ConfigSnapshot, Rule } from "../../lib/types";

  interface Props {
    config: ConfigSnapshot | null;
    ruleDraft?: Rule | null;
    ruleAllowanceDraft?: Allowance | null;
    ruleSaving: boolean;
    ruleMessage: string | null;
    tier1EditUnlocked: boolean;
    onSelectRule: (rule: Rule) => void;
    onStartNewRule: () => void;
    onSaveRuleDraft: () => void | Promise<void>;
    onRemoveRuleDraft: () => void | Promise<void>;
  }

  let {
    config,
    ruleDraft = $bindable<Rule | null>(null),
    ruleAllowanceDraft = $bindable<Allowance | null>(null),
    ruleSaving,
    ruleMessage,
    tier1EditUnlocked,
    onSelectRule,
    onStartNewRule,
    onSaveRuleDraft,
    onRemoveRuleDraft
  }: Props = $props();

  let savedRule = $derived(
    ruleDraft ? (config?.rules.find((rule) => rule.id === ruleDraft?.id) ?? null) : null
  );
  let ruleDraftIsExisting = $derived(Boolean(savedRule));
  let ruleDraftLocked = $derived(
    Boolean(
      savedRule &&
        ruleIsActive(savedRule, config?.schedules ?? []) &&
        !(savedRule.tier === "hard" && tier1EditUnlocked)
    )
  );

  function addPattern(): void {
    if (!ruleDraft) return;
    ruleDraft.patterns = [
      ...ruleDraft.patterns,
      { kind: "domain", value: "", match_subdomains: true }
    ];
  }

  function removePattern(index: number): void {
    if (!ruleDraft || ruleDraft.patterns.length <= 1) return;
    ruleDraft.patterns = ruleDraft.patterns.filter((_, patternIndex) => patternIndex !== index);
  }

  function setRuleTier(tier: Rule["tier"]): void {
    if (!ruleDraft) return;
    ruleDraft.tier = tier;
    if (tier === "hard") {
      ruleDraft.allowance_id = null;
      ruleAllowanceDraft = null;
      ruleDraft.unlock_policy = null;
    } else if (!ruleAllowanceDraft) {
      ruleAllowanceDraft = defaultAllowanceForRule(ruleDraft);
    }
  }

  function toggleDraftSchedule(scheduleId: string): void {
    if (!ruleDraft) return;
    if (ruleDraft.schedule_ids.includes(scheduleId)) {
      ruleDraft.schedule_ids = ruleDraft.schedule_ids.filter((id) => id !== scheduleId);
    } else {
      ruleDraft.schedule_ids = [...ruleDraft.schedule_ids, scheduleId];
    }
  }
</script>

<section class="split-view">
  <article class="panel list-panel">
    <div class="panel-title">
      <ListChecks size={18} aria-hidden="true" />
      <h2>Site Lists</h2>
    </div>
    <button class="secondary wide-button" onclick={onStartNewRule}>
      <Plus size={17} aria-hidden="true" />
      <span>New list</span>
    </button>
    <div class="rule-list">
      {#each config?.rules ?? [] as rule (rule.id)}
        <button class:active={ruleDraft?.id === rule.id} onclick={() => onSelectRule(rule)}>
          <span class:hard={rule.tier === "hard"} class="tier-dot"></span>
          <span>{rule.name}</span>
          <em>{rule.tier === "hard" ? "Tier 1" : "Tier 2"}</em>
        </button>
      {:else}
        <p class="empty-state">No lists reported by the daemon.</p>
      {/each}
    </div>
  </article>

  <article class="panel detail-panel">
    <div class="panel-title">
      <Shield size={18} aria-hidden="true" />
      <h2>{ruleDraft?.name || "Site list"}</h2>
    </div>
    {#if ruleDraft}
      {#if ruleDraftLocked}
        <section class="inline-warning">
          <AlertTriangle size={17} aria-hidden="true" />
          <span>This list is active right now.</span>
        </section>
      {/if}
      <div class="form-grid">
        <label>
          <span>Name</span>
          <input bind:value={ruleDraft.name} disabled={ruleDraftLocked} />
        </label>
        <label>
          <span>Tier</span>
          <select
            value={ruleDraft.tier}
            disabled={ruleDraftLocked}
            onchange={(event) => setRuleTier(event.currentTarget.value as Rule["tier"])}
          >
            <option value="controlled_access">Tier 2</option>
            <option value="hard">Tier 1</option>
          </select>
        </label>
      </div>

      <label class="check-row">
        <input type="checkbox" bind:checked={ruleDraft.enabled} disabled={ruleDraftLocked} />
        <span>Enabled</span>
      </label>

      {#if ruleDraft.tier === "controlled_access"}
        <div class="section-label">Daily allowance</div>
        <div class="allowance-editor">
          {#if ruleAllowanceDraft}
            <label>
              <span>Daily minutes</span>
              <input
                type="number"
                min="1"
                max="1440"
                bind:value={ruleAllowanceDraft.daily_minutes}
                disabled={ruleDraftLocked}
              />
            </label>
          {/if}
        </div>
      {/if}

      <div class="section-label">Schedules</div>
      <div class="chip-grid">
        {#each config?.schedules ?? [] as schedule (schedule.id)}
          <label class="chip-check">
            <input
              type="checkbox"
              checked={ruleDraft.schedule_ids.includes(schedule.id)}
              disabled={ruleDraftLocked}
              onchange={() => toggleDraftSchedule(schedule.id)}
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
              <th>Type</th>
              <th>Pattern</th>
              <th>Subdomains</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each ruleDraft.patterns as pattern, index (pattern)}
              <tr>
                <td>
                  <select bind:value={pattern.kind} disabled={ruleDraftLocked}>
                    {#each patternKinds as kind (kind.id)}
                      <option value={kind.id}>{kind.label}</option>
                    {/each}
                  </select>
                </td>
                <td>
                  <input bind:value={pattern.value} disabled={ruleDraftLocked} />
                </td>
                <td>
                  <input
                    type="checkbox"
                    bind:checked={pattern.match_subdomains}
                    disabled={ruleDraftLocked || pattern.kind !== "domain"}
                  />
                </td>
                <td>
                  <button
                    class="icon-button"
                    title="Remove pattern"
                    onclick={() => removePattern(index)}
                    disabled={ruleDraftLocked || ruleDraft.patterns.length <= 1}
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
        <button class="secondary" onclick={addPattern} disabled={ruleDraftLocked}>
          <Plus size={17} aria-hidden="true" />
          <span>Pattern</span>
        </button>
        <button
          class="secondary"
          onclick={onRemoveRuleDraft}
          disabled={ruleSaving || ruleDraftLocked || !ruleDraftIsExisting}
        >
          <Trash2 size={17} aria-hidden="true" />
          <span>Delete</span>
        </button>
        <button class="primary" onclick={onSaveRuleDraft} disabled={ruleSaving || ruleDraftLocked}>
          <Save size={17} aria-hidden="true" />
          <span>Save</span>
        </button>
      </div>
      {#if ruleMessage}
        <p class="result-text">{ruleMessage}</p>
      {/if}
    {/if}
  </article>
</section>
