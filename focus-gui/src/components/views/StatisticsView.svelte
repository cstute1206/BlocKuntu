<script lang="ts">
  import { Activity, BarChart3 } from "@lucide/svelte";
  import type { RecentEvent } from "../../lib/types";

  interface Props {
    events: RecentEvent[];
  }

  let { events }: Props = $props();

  let eventBuckets = $derived.by(() => {
    const counts: Record<string, number> = {};
    for (const event of events) {
      counts[event.kind] = (counts[event.kind] ?? 0) + 1;
    }
    return Object.entries(counts)
      .map(([kind, count]) => ({ kind, count }))
      .sort((a, b) => b.count - a.count);
  });

  function formatEventKind(kind: string): string {
    return kind.replace(/_/g, " ");
  }
</script>

<section class="content-grid">
  <article class="panel">
    <div class="panel-title">
      <BarChart3 size={18} aria-hidden="true" />
      <h2>Event Mix</h2>
    </div>
    <div class="event-mix-list">
      {#each eventBuckets as bucket (bucket.kind)}
        <div class="event-mix-row">
          <span title={bucket.kind}>{formatEventKind(bucket.kind)}</span>
          <strong>{bucket.count}</strong>
        </div>
      {:else}
        <p class="empty-state">No events recorded yet.</p>
      {/each}
    </div>
  </article>

  <article class="panel">
    <div class="panel-title">
      <Activity size={18} aria-hidden="true" />
      <h2>Recent Events</h2>
    </div>
    <div class="event-list">
      {#each events.slice(0, 12) as event (event.id)}
        <div class="event-row">
          <span title={event.kind}>{formatEventKind(event.kind)}</span>
          <strong title={event.target ?? "system"}>{event.target ?? "system"}</strong>
          <time>{new Date(event.created_at).toLocaleTimeString()}</time>
        </div>
      {:else}
        <p class="empty-state">No events recorded yet.</p>
      {/each}
    </div>
  </article>
</section>
