<script lang="ts">
  import SiteListsView from '../src/components/views/SiteListsView.svelte';
  import type { Rule } from '../src/lib/types';
  let draft = $state<Rule>({ id: 'work', name: 'Work', tier: 'hard', mode: 'blocklist', enabled: true, allowance_id: null, schedule_ids: [], patterns: [{ kind: 'domain', value: 'work.test', match_subdomains: false }] });
</script>
<SiteListsView config={null} bind:ruleDraft={draft} ruleSaving={false} ruleMessage={null}
  tier1EditUnlocked={false} onSelectRule={() => {}} onStartNewRule={() => {}}
  onSaveRuleDraft={() => {}} onRemoveRuleDraft={() => {}} />
