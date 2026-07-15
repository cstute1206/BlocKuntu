<script lang="ts">
  import { BarChart3, FileText } from "@lucide/svelte";
  import type { LogSummary } from "../../lib/types";

  interface Props {
    logSummary: LogSummary | null;
  }

  let { logSummary }: Props = $props();

  let eventCounts = $derived.by(() =>
    Object.entries(logSummary?.event_counts ?? {})
      .map(([kind, count]) => ({ kind, count }))
      .sort((a, b) => b.count - a.count)
  );

  function formatEventKind(kind: string): string {
    return kind.replace(/_/g, " ");
  }
</script>

<section class="content-grid">
  <article class="panel">
    <div class="panel-title">
      <BarChart3 size={18} aria-hidden="true" />
      <h2>Recorded events</h2>
    </div>
    <p class="metric-value">{logSummary?.total_events ?? 0}</p>
    <p class="muted">Counted from the BlocKuntu log file.</p>
  </article>

  <article class="panel">
    <div class="panel-title">
      <FileText size={18} aria-hidden="true" />
      <h2>Log file</h2>
    </div>
    <code class="log-path">{logSummary?.path ?? "/etc/blockuntu/blockuntu.log"}</code>
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
</section>
