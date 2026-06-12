<script lang="ts">
  import { AlertTriangle, CalendarDays, Plus, Save, Trash2 } from "@lucide/svelte";
  import { scheduleDayChoices, scheduleIsActive, weekdays, windowsFor } from "../../lib/ui";
  import type { ConfigSnapshot, Schedule } from "../../lib/types";

  interface Props {
    config: ConfigSnapshot | null;
    scheduleDraft?: Schedule | null;
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

  function removeScheduleWindow(index: number): void {
    if (!scheduleDraft) return;
    scheduleDraft.windows = scheduleDraft.windows.filter((_, windowIndex) => windowIndex !== index);
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
          <span>This schedule is active right now.</span>
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
                    disabled={scheduleDraftLocked}
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
                    disabled={scheduleDraftLocked}
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
                    disabled={scheduleDraftLocked}
                    aria-invalid={!isTwentyFourHourTime(window.end)}
                  />
                </td>
                <td>
                  <button
                    class="icon-button"
                    title="Remove window"
                    onclick={() => removeScheduleWindow(index)}
                    disabled={scheduleDraftLocked}
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
        <button class="secondary" onclick={addScheduleWindow} disabled={scheduleDraftLocked}>
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
          disabled={scheduleSaving || scheduleDraftLocked || !scheduleDraftHasValidTimes}
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
