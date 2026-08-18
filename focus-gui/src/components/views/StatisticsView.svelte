<script lang="ts">
  import { BarChart3, CalendarClock, FileText } from "@lucide/svelte";
  import type { LogSummary, ScheduleActivitySummary } from "../../lib/types";

  interface Props {
    logSummary: LogSummary | null;
    scheduleActivitySummary: ScheduleActivitySummary | null;
  }

  let { logSummary, scheduleActivitySummary }: Props = $props();

  let eventCounts = $derived.by(() =>
    Object.entries(logSummary?.event_counts ?? {})
      .map(([kind, count]) => ({ kind, count }))
      .sort((a, b) => b.count - a.count)
  );
  let totalScheduleActiveSeconds = $derived(
    (scheduleActivitySummary?.schedules ?? []).reduce(
      (total, schedule) => total + schedule.total_active_seconds,
      0
    )
  );

  function formatEventKind(kind: string): string {
    return kind.replace(/_/g, " ");
  }

  function formatActiveDuration(totalSeconds: number): string {
    const totalMinutes = Math.floor(totalSeconds / 60);
    const days = Math.floor(totalMinutes / (24 * 60));
    const hours = Math.floor((totalMinutes % (24 * 60)) / 60);
    const minutes = totalMinutes % 60;
    if (days > 0) return `${days} d ${hours} h ${minutes} min`;
    if (hours > 0) return `${hours} h ${minutes} min`;
    return `${minutes} min`;
  }
</script>

<section class="content-grid">
  <article class="panel">
    <div class="panel-title">
      <BarChart3 size={18} aria-hidden="true" />
      <h2>Recorded events</h2>
    </div>
    <p class="metric-value">{logSummary?.total_events ?? 0}</p>
    <p class="muted">All-time count retained independently from the diagnostic log.</p>
  </article>

  <article class="panel">
    <div class="panel-title">
      <FileText size={18} aria-hidden="true" />
      <h2>Log file</h2>
    </div>
    <code class="log-path">{logSummary?.path ?? "/etc/blockuntu/blockuntu.log"}</code>
    <p class="statistics-note">Detailed log entries are retained for {logSummary?.detail_retention_days ?? 30} days; the event totals below are all-time.</p>
    <div class="event-mix-list">
      {#each eventCounts as bucket (bucket.kind)}
        <div class="event-mix-row">
          <span title={bucket.kind}>{formatEventKind(bucket.kind)}</span>
          <strong>{bucket.count}</strong>
        </div>
      {:else}
        <p class="empty-state">No log entries recorded yet.</p>
      {/each}
    </div>
  </article>

  <article class="panel wide-panel">
    <div class="panel-title">
      <CalendarClock size={18} aria-hidden="true" />
      <h2>Schedule active time</h2>
    </div>
    <p class="metric-value">{formatActiveDuration(totalScheduleActiveSeconds)}</p>
    <p class="statistics-note">
      Accumulated while schedules are actually active. The timer persists across daemon restarts.
    </p>
    <div class="event-mix-list">
      {#each scheduleActivitySummary?.schedules ?? [] as schedule (schedule.id)}
        <div class="event-mix-row">
          <span title={schedule.id}>{schedule.name ?? schedule.id}</span>
          <strong>{formatActiveDuration(schedule.total_active_seconds)}</strong>
        </div>
      {:else}
        <p class="empty-state">No schedule activity recorded yet.</p>
      {/each}
    </div>
  </article>
</section>
