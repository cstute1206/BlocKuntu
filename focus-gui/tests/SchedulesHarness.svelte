<script lang="ts">
  import SchedulesView from '../src/components/views/SchedulesView.svelte';
  import type { ConfigSnapshot, Schedule } from '../src/lib/types';
  let { config, schedule, unlocked = false, detox = [], remove = () => {} }: {
    config: ConfigSnapshot; schedule: Schedule; unlocked?: boolean; detox?: string[]; remove?: () => void;
  } = $props();
  let draft = $state(structuredClone((() => schedule)()));
</script>
<SchedulesView {config} bind:scheduleDraft={draft} tier1EditUnlocked={unlocked}
  activeDetoxSiteRuleIds={detox} activeDetoxAppRuleIds={detox}
  scheduleSaving={false} scheduleMessage={null} timeFormat="24h"
  onSelectSchedule={() => {}} onStartNewSchedule={() => {}}
  onSaveScheduleDraft={() => {}} onRemoveScheduleDraft={remove} />
