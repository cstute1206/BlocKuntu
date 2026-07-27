<script lang="ts">
  import { AlertTriangle, CalendarDays, Plus, Save, Trash2 } from "@lucide/svelte";
  import { scheduleDayChoices, scheduleIsActive, weekdays, windowsFor } from "../../lib/ui";
  import type { AppRule, ConfigSnapshot, Rule, Schedule } from "../../lib/types";

  interface Props {
    config: ConfigSnapshot | null;
    scheduleDraft?: Schedule | null;
    scheduleSiteRuleIds?: string[];
    scheduleAppRuleIds?: string[];
    scheduleSaving: boolean;
    scheduleMessage: string | null;
    onSelectSchedule: (schedule: Schedule) => void;
    onStartNewSchedule: () => void;
    onSaveScheduleDraft: () => void | Promise<void>;
    onRemoveScheduleDraft: () => void | Promise<void>;
  }

  let {
    config,
    scheduleDraft = $bindable<Schedule | null>(null),
    scheduleSiteRuleIds = $bindable<string[]>([]),
    scheduleAppRuleIds = $bindable<string[]>([]),
    scheduleSaving,
    scheduleMessage,
    onSelectSchedule,
    onStartNewSchedule,
    onSaveScheduleDraft,
    onRemoveScheduleDraft
  }: Props = $props();

  let savedSchedule = $derived(
    scheduleDraft
      ? (config?.schedules.find((schedule) => schedule.id === scheduleDraft?.id) ?? null)
      : null
  );
  let scheduleDraftIsExisting = $derived(Boolean(savedSchedule));
  let scheduleDraftLocked = $derived(Boolean(savedSchedule && scheduleIsActive(savedSchedule)));
  let savedWindowCount = $derived(savedSchedule?.windows.length ?? 0);
  let schedulableSiteRules = $derived(config?.rules.filter((rule) => rule.tier !== "hard") ?? []);
  let schedulableAppRules = $derived(
    config?.app_rules.filter((rule) => rule.tier !== "hard") ?? []
  );
  const timeInputPattern = "([01][0-9]|2[0-3]):[0-5][0-9]";
  const timeInputTitle = "24-hour time, for example 09:00";
  let scheduleDraftHasValidTimes = $derived(
    Boolean(
      scheduleDraft?.windows.every(
        (window) => isTwentyFourHourTime(window.start) && isTwentyFourHourTime(window.end)
      )
    )
  );

  function isTwentyFourHourTime(value: string): boolean {
    return /^([01][0-9]|2[0-3]):[0-5][0-9]$/.test(value);
  }

  function addScheduleWindow(): void {
    if (!scheduleDraft) return;
    scheduleDraft.windows = [
      ...scheduleDraft.windows,
      { weekday: "workdays", start: "09:00", end: "17:00" }
    ];
  }

  function windowIsSaved(index: number): boolean {
    return Boolean(savedSchedule && index < savedWindowCount);
  }

  function windowEditLocked(index: number): boolean {
    return scheduleDraftLocked && windowIsSaved(index);
  }

  function windowRemoveLocked(index: number): boolean {
    return scheduleDraftLocked && windowIsSaved(index);
  }

  function removeScheduleWindow(index: number): void {
    if (!scheduleDraft) return;
    scheduleDraft.windows = scheduleDraft.windows.filter((_, windowIndex) => windowIndex !== index);
  }

  function toggleScheduleSiteRule(rule: Rule): void {
    scheduleSiteRuleIds = toggleId(scheduleSiteRuleIds, rule.id);
  }

  function toggleScheduleAppRule(rule: AppRule): void {
    scheduleAppRuleIds = toggleId(scheduleAppRuleIds, rule.id);
  }

  function toggleId(values: string[], id: string): string[] {
    return values.includes(id) ? values.filter((value) => value !== id) : [...values, id];
  }
</script>

<section class="split-view">
  <article class="panel list-panel">
    <div class="panel-title">
      <CalendarDays size={18} aria-hidden="true" />
      <h2>Schedules</h2>
    </div>
    <button class="secondary wide-button" onclick={onStartNewSchedule}>
      <Plus size={17} aria-hidden="true" />
      <span>New schedule</span>
    </button>
    <div class="rule-list">
      {#each config?.schedules ?? [] as schedule (schedule.id)}
        <button
          class:active={scheduleDraft?.id === schedule.id}
          onclick={() => onSelectSchedule(schedule)}
        >
          <span class:hard={scheduleIsActive(schedule)} class="tier-dot"></span>
          <span>{schedule.name ?? schedule.id}</span>
          <em>{scheduleIsActive(schedule) ? "Active" : "Idle"}</em>
        </button>
      {:else}
        <p class="empty-state">No schedules reported by the daemon.</p>
      {/each}
    </div>
  </article>

  <article class="panel detail-panel">
    <div class="panel-title">
      <CalendarDays size={18} aria-hidden="true" />
      <h2>{scheduleDraft?.name || "Schedule"}</h2>
    </div>
    {#if scheduleDraft}
      {#if scheduleDraftLocked}
        <section class="inline-warning">
          <AlertTriangle size={17} aria-hidden="true" />
          <span>This schedule is active right now. Existing windows and attachments are locked, but you can append new windows.</span>
        </section>
      {/if}
      <div class="form-grid schedule-form">
        <label>
          <span>Name</span>
          <input bind:value={scheduleDraft.name} disabled={scheduleDraftLocked} />
        </label>
      </div>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Days</th>
              <th>Start</th>
              <th>End</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each scheduleDraft.windows as window, index (window)}
              <tr>
                <td>
                  <select
                    bind:value={window.weekday}
                    disabled={windowEditLocked(index)}
                  >
                    {#each scheduleDayChoices as day (day.id)}
                      <option value={day.id}>{day.label}</option>
                    {/each}
                  </select>
                </td>
                <td>
                  <input
                    class="time-input"
                    type="text"
                    inputmode="text"
                    maxlength="5"
                    pattern={timeInputPattern}
                    placeholder="09:00"
                    title={timeInputTitle}
                    bind:value={window.start}
                    disabled={windowEditLocked(index)}
                    aria-invalid={!isTwentyFourHourTime(window.start)}
                  />
                </td>
                <td>
                  <input
                    class="time-input"
                    type="text"
                    inputmode="text"
                    maxlength="5"
                    pattern={timeInputPattern}
                    placeholder="17:00"
                    title={timeInputTitle}
                    bind:value={window.end}
                    disabled={windowEditLocked(index)}
                    aria-invalid={!isTwentyFourHourTime(window.end)}
                  />
                </td>
                <td>
                  <button
                    class="icon-button"
                    title="Remove window"
                    onclick={() => removeScheduleWindow(index)}
                    disabled={windowRemoveLocked(index)}
                  >
                    <Trash2 size={16} aria-hidden="true" />
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="section-label">Attached websites</div>
      <div class="chip-grid">
        {#each schedulableSiteRules as rule (rule.id)}
          <label class="chip-check">
            <input
              type="checkbox"
              checked={scheduleSiteRuleIds.includes(rule.id)}
              disabled={scheduleDraftLocked}
              onchange={() => toggleScheduleSiteRule(rule)}
            />
            <span>{rule.name || rule.id}</span>
          </label>
        {:else}
          <p class="empty-state">No Tier 2 or Tier 3 websites available.</p>
        {/each}
      </div>

      <div class="section-label">Attached applications</div>
      <div class="chip-grid">
        {#each schedulableAppRules as rule (rule.id)}
          <label class="chip-check">
            <input
              type="checkbox"
              checked={scheduleAppRuleIds.includes(rule.id)}
              disabled={scheduleDraftLocked}
              onchange={() => toggleScheduleAppRule(rule)}
            />
            <span>{rule.name || rule.id}</span>
          </label>
        {:else}
          <p class="empty-state">No Tier 2 or Tier 3 applications available.</p>
        {/each}
      </div>

      <div class="button-row schedule-action-row">
        <button class="secondary" onclick={addScheduleWindow}>
          <Plus size={17} aria-hidden="true" />
          <span>Window</span>
        </button>
        <button
          class="secondary"
          onclick={onRemoveScheduleDraft}
          disabled={scheduleSaving || scheduleDraftLocked || !scheduleDraftIsExisting}
        >
          <Trash2 size={17} aria-hidden="true" />
          <span>Delete</span>
        </button>
        <button
          class="primary"
          onclick={onSaveScheduleDraft}
          disabled={scheduleSaving || !scheduleDraftHasValidTimes}
        >
          <Save size={17} aria-hidden="true" />
          <span>Save</span>
        </button>
      </div>
      {#if scheduleMessage}
        <p class="result-text">{scheduleMessage}</p>
      {/if}
    {/if}
  </article>

  <article class="panel wide-panel">
    <div class="panel-title">
      <CalendarDays size={18} aria-hidden="true" />
      <h2>Weekly Grid</h2>
    </div>
    <div class="schedule-grid">
      <div class="schedule-head">Schedule</div>
      {#each weekdays as day (day.id)}
        <div class="schedule-head">{day.label}</div>
      {/each}
      {#each config?.schedules ?? [] as schedule (schedule.id)}
        <div class="schedule-name">{schedule.name ?? schedule.id}</div>
        {#each weekdays as day (day.id)}
          <div class:filled={windowsFor(schedule, day.id)} class="schedule-cell">
            {windowsFor(schedule, day.id) || "-"}
          </div>
        {/each}
      {:else}
        <p class="empty-state">No schedules reported by the daemon.</p>
      {/each}
    </div>
  </article>
</section>
