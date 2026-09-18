<script lang="ts">
  import {
    AlertTriangle,
    ListChecks,
    Plus,
    Save,
    Shield,
    Trash2,
    XCircle
  } from "@lucide/svelte";
  import { tick } from "svelte";
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
    activeDetoxSiteRuleIds?: string[];
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
    activeDetoxSiteRuleIds = [],
    onSelectRule,
    onStartNewRule,
    onSaveRuleDraft,
    onRemoveRuleDraft
  }: Props = $props();

  let savedRule = $derived(
    ruleDraft ? (config?.rules.find((rule) => rule.id === ruleDraft?.id) ?? null) : null
  );
  let ruleDraftIsExisting = $derived(Boolean(savedRule));
  let ruleDraftDetoxLocked = $derived(
    Boolean(savedRule && activeDetoxSiteRuleIds.includes(savedRule.id))
  );
  let ruleDraftActive = $derived(
    Boolean(savedRule && ruleIsActive(savedRule, config?.schedules ?? []))
  );
  let ruleDraftEditLocked = $derived(
    Boolean(
      savedRule &&
        (ruleDraftDetoxLocked ||
          (ruleDraftActive && !(savedRule.tier === "hard" && tier1EditUnlocked)))
    )
  );
  let savedPatternCount = $derived(savedRule?.patterns.length ?? 0);
  let patternValueInputs: HTMLInputElement[] = [];
  let ruleSaveWarningOpen = $state(false);

  function ruleSaveNeedsWarning(): boolean {
    return Boolean(
      ruleDraft &&
        (ruleDraft.mode === "allowlist" ||
          (ruleDraft.mode === "blocklist" && ruleDraft.tier === "hard"))
    );
  }

  function requestRuleSave(): void {
    if (ruleSaveNeedsWarning()) {
      ruleSaveWarningOpen = true;
      return;
    }
    void onSaveRuleDraft();
  }

  function confirmRuleSave(): void {
    ruleSaveWarningOpen = false;
    void onSaveRuleDraft();
  }

  function closeRuleSaveWarning(): void {
    ruleSaveWarningOpen = false;
  }

  function closeRuleSaveWarningOnEscape(event: KeyboardEvent): void {
    if (event.key === "Escape") closeRuleSaveWarning();
  }

  function closeRuleSaveWarningFromBackdrop(event: MouseEvent): void {
    if (event.currentTarget === event.target) closeRuleSaveWarning();
  }

  function patternIsSaved(index: number): boolean {
    return Boolean(savedRule && index < savedPatternCount);
  }

  function patternEditLocked(index: number): boolean {
    return patternIsSaved(index) && ruleDraftEditLocked;
  }

  function patternRemoveLocked(index: number): boolean {
    if (!patternIsSaved(index)) return false;
    if (savedRule?.mode === "allowlist") return false;
    if (ruleDraftDetoxLocked) return true;
    return ruleDraftActive && !(savedRule?.tier === "hard" && tier1EditUnlocked);
  }

  function patternAddLocked(): boolean {
    return Boolean(
      savedRule &&
        savedRule.mode === "allowlist" &&
        (ruleDraftDetoxLocked ||
          (ruleDraftActive && !(savedRule.tier === "hard" && tier1EditUnlocked)))
    );
  }

  function addPattern(): void {
    if (!ruleDraft || patternAddLocked()) return;
    ruleDraft.patterns = [
      ...ruleDraft.patterns,
      { kind: "domain", value: "", match_subdomains: true }
    ];
  }

  async function addPatternOnEnter(event: KeyboardEvent, index: number): Promise<void> {
    if (
      event.key !== "Enter" ||
      event.isComposing ||
      !ruleDraft ||
      index !== ruleDraft.patterns.length - 1 ||
      patternAddLocked() ||
      patternEditLocked(index) ||
      !ruleDraft.patterns[index].value.trim()
    ) {
      return;
    }

    event.preventDefault();
    addPattern();
    await tick();
    patternValueInputs[ruleDraft.patterns.length - 1]?.focus();
  }

  function removePattern(index: number): void {
    if (!ruleDraft || ruleDraft.patterns.length <= 1) return;
    ruleDraft.patterns = ruleDraft.patterns.filter((_, patternIndex) => patternIndex !== index);
  }

  function setRuleTier(tier: Rule["tier"]): void {
    if (!ruleDraft) return;
    if (ruleDraft.mode === "allowlist" && tier === "hard") return;
    ruleDraft.tier = tier;
    if (tier !== "controlled_access") {
      ruleDraft.allowance_id = null;
      ruleAllowanceDraft = null;
    } else if (!ruleAllowanceDraft) {
      ruleAllowanceDraft = defaultAllowanceForRule(ruleDraft);
    }
  }

  function setRuleMode(mode: Rule["mode"]): void {
    if (!ruleDraft) return;
    ruleDraft.mode = mode;
    if (mode === "allowlist") {
      ruleDraft.enabled = true;
      if (ruleDraft.tier === "hard") setRuleTier("scheduled_block");
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
      <h2>Websites</h2>
    </div>
    <button class="secondary wide-button" onclick={onStartNewRule}>
      <Plus size={17} aria-hidden="true" />
      <span>New website</span>
    </button>
    <div class="rule-list">
      {#each config?.rules ?? [] as rule (rule.id)}
        <button class:active={ruleDraft?.id === rule.id} onclick={() => onSelectRule(rule)}>
          <span class:hard={rule.tier === "hard"} class="tier-dot"></span>
          <span>{rule.name}</span>
          <em>
            {rule.tier === "hard" ? "Tier 1" : rule.tier === "scheduled_block" ? "Tier 2" : "Tier 3"}
            {rule.mode === "allowlist" ? " · Allowlist" : ""}
          </em>
        </button>
      {:else}
        <p class="empty-state">No websites reported by the daemon.</p>
      {/each}
    </div>
  </article>

  <article class="panel detail-panel">
    <div class="panel-title">
      <Shield size={18} aria-hidden="true" />
      <h2>{ruleDraft?.name || "Website"}</h2>
    </div>
    {#if ruleDraft}
      {#if ruleDraftDetoxLocked || ruleDraftActive}
        <section class="inline-warning">
          <AlertTriangle size={17} aria-hidden="true" />
          <span>
            {ruleDraftDetoxLocked
              ? ruleDraft.mode === "allowlist"
                ? "This allowlist is covered by an active detox session. Existing patterns can be removed to tighten access, but allowed websites cannot be added."
                : "This website list is covered by an active detox session. Existing settings are locked, but you can still append patterns."
              : ruleDraft.mode === "allowlist"
                ? "This allowlist is active right now. Existing patterns can be removed to tighten access, but allowed websites cannot be added."
                : "This website list is active right now. Existing settings are locked, but you can still append patterns."}
          </span>
        </section>
      {/if}
      <div class="form-grid">
        <label>
          <span>Name</span>
          <input bind:value={ruleDraft.name} disabled={ruleDraftEditLocked} />
        </label>
        <label>
          <span>Tier</span>
          <select
            value={ruleDraft.tier}
            disabled={ruleDraftEditLocked}
            onchange={(event) => setRuleTier(event.currentTarget.value as Rule["tier"])}
          >
            <option value="hard" disabled={ruleDraft.mode === "allowlist"}>Tier 1</option>
            <option value="scheduled_block">Tier 2</option>
            <option value="controlled_access">Tier 3</option>
          </select>
        </label>
        <label>
          <span>List behavior</span>
          <select
            value={ruleDraft.mode}
            disabled={ruleDraftEditLocked}
            onchange={(event) => setRuleMode(event.currentTarget.value as Rule["mode"])}
          >
            <option value="blocklist">Block listed websites</option>
            <option value="allowlist">Allow only listed websites</option>
          </select>
        </label>
      </div>

      {#if ruleDraft.tier === "controlled_access"}
        <div class="section-label">Daily allowance</div>
        <div class="allowance-editor">
          {#if ruleAllowanceDraft}
            <label>
              <span>Daily minutes</span>
              <input
                type="number"
                min="0"
                max="1440"
                bind:value={ruleAllowanceDraft.daily_minutes}
                disabled={ruleDraftEditLocked}
              />
            </label>
          {/if}
        </div>
      {/if}

      <div class="section-label">Schedules</div>
      {#if ruleDraft.tier !== "hard"}
        <p class="tier2-schedule-note">
          {ruleDraft.tier === "scheduled_block"
            ? ruleDraft.mode === "allowlist"
              ? "Tier 2 allows only these websites during an attached schedule or Detox. It cannot be unlocked and requires the browser extension."
              : "Tier 2 websites block strictly during an attached schedule or Detox, cannot be unlocked and domain patterns enter the hosts file while active."
            : ruleDraft.mode === "allowlist"
              ? "Tier 3 counts time outside these websites against the daily allowance. After it is used, only these websites remain available. Manual unlocks still work."
              : "Tier 3 websites use allowances and manual unlocks during an attached schedule or Detox and never enter the hosts file."}
        </p>
        {#if ruleDraft.schedule_ids.length === 0 && !ruleDraftDetoxLocked}
          <section class="inline-warning">
            <AlertTriangle size={17} aria-hidden="true" />
            <span>
              No schedule is attached. This {ruleDraft.tier === "scheduled_block" ? "Tier 2" : "Tier 3"} list of websites stays inactive unless you select it for Detox or add it to a schedule.
            </span>
          </section>
        {/if}
      {/if}
      <div class="chip-grid">
        {#each config?.schedules ?? [] as schedule (schedule.id)}
          <label class="chip-check">
            <input
              type="checkbox"
              checked={ruleDraft.schedule_ids.includes(schedule.id)}
              disabled={ruleDraftEditLocked}
              onchange={() => toggleDraftSchedule(schedule.id)}
            />
            <span>{schedule.name ?? schedule.id}</span>
          </label>
        {:else}
          <p class="empty-state">No schedules available.</p>
        {/each}
      </div>

      <div class="section-label">
        {ruleDraft.mode === "allowlist" ? "Allowed websites" : "Blocked websites"}
      </div>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th title="Hover a type selector for an example.">Type</th>
              <th>Pattern</th>
              <th title="For Domain patterns only: also match hosts such as www.example.com and app.example.com.">
                Subdomains
              </th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each ruleDraft.patterns as pattern, index (pattern)}
              <tr>
                <td>
                  <select
                    bind:value={pattern.kind}
                    disabled={patternEditLocked(index)}
                    title={patternKinds.find((kind) => kind.id === pattern.kind)?.help ?? ""}
                  >
                    {#each patternKinds as kind (kind.id)}
                      <option value={kind.id}>{kind.label}</option>
                    {/each}
                  </select>
                </td>
                <td>
                  <input
                    bind:value={pattern.value}
                    bind:this={patternValueInputs[index]}
                    disabled={patternEditLocked(index)}
                    aria-keyshortcuts="Enter"
                    title="Press Enter in the last pattern to add another"
                    onkeydown={(event) => addPatternOnEnter(event, index)}
                  />
                </td>
                <td>
                  <input
                    type="checkbox"
                    bind:checked={pattern.match_subdomains}
                    disabled={patternEditLocked(index) || pattern.kind !== "domain"}
                  />
                </td>
                <td>
                  <button
                    class="icon-button"
                    title="Remove pattern"
                    onclick={() => removePattern(index)}
                    disabled={
                      patternRemoveLocked(index) || ruleDraft.patterns.length <= 1
                    }
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
        <button class="secondary" onclick={addPattern} disabled={patternAddLocked()}>
          <Plus size={17} aria-hidden="true" />
          <span>Pattern</span>
        </button>
        <button
          class="secondary"
          onclick={onRemoveRuleDraft}
          disabled={ruleSaving || ruleDraftDetoxLocked || ruleDraftActive || !ruleDraftIsExisting}
        >
          <Trash2 size={17} aria-hidden="true" />
          <span>Delete</span>
        </button>
        <button class="primary" onclick={requestRuleSave} disabled={ruleSaving}>
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

{#if ruleSaveWarningOpen && ruleDraft}
  <div
    class="onboarding-backdrop"
    role="presentation"
    onclick={closeRuleSaveWarningFromBackdrop}
  >
    <div
      class="onboarding-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="rule-save-warning-title"
      aria-describedby="rule-save-warning-description"
      tabindex="-1"
      onkeydown={closeRuleSaveWarningOnEscape}
    >
      <div class="onboarding-modal-header">
        <div class="panel-title">
          <AlertTriangle size={18} aria-hidden="true" />
          <h2 id="rule-save-warning-title">
            {ruleDraft.mode === "allowlist" ? "Save this allowlist?" : "Save this Tier 1 blocklist?"}
          </h2>
        </div>
        <button
          class="icon-button"
          title="Cancel save"
          aria-label="Cancel save"
          onclick={closeRuleSaveWarning}
        >
          <XCircle size={17} aria-hidden="true" />
        </button>
      </div>
      <div class="onboarding-copy" id="rule-save-warning-description">
        {#if ruleDraft.mode === "allowlist"}
          {#if ruleDraft.tier === "controlled_access"}
            <p>
              Saving does not activate this list. During an attached schedule or Detox, time on
              non-matching websites consumes its daily allowance. After the allowance is used,
              every non-matching top-level HTTP/HTTPS website is blocked unless manually unlocked.
            </p>
          {:else}
            <p>
              Saving does not activate this list. Once it is active through an attached schedule or
              Detox, every top-level HTTP/HTTPS website that does not match will be blocked.
            </p>
          {/if}
          <ul>
            <li>Redirect and sign-in domains may need their own entries.</li>
            <li>A website must match every allowlist that is active at the same time.</li>
            <li>A matching blocklist still wins.</li>
            <li>Allowlist enforcement requires the browser extension. Private windows also need extension access unless private browsing is disabled.</li>
          </ul>
        {:else}
          <p>
            Tier 1 blocklists are always active. Saving this list can immediately block every
            matching website. Broader changes later require a Tier 1 edit unlock.
          </p>
        {/if}
        <div class="button-row onboarding-actions">
          <button class="secondary" onclick={closeRuleSaveWarning}>Cancel</button>
          <button class="primary" onclick={confirmRuleSave} disabled={ruleSaving}>
            <Save size={17} aria-hidden="true" />
            <span>{ruleDraft.mode === "allowlist" ? "Save allowlist" : "Save Tier 1 list"}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
