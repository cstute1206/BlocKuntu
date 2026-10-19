<script lang="ts">
  import SiteListsView from '../src/components/views/SiteListsView.svelte';
  import type { ConfigSnapshot, Rule } from '../src/lib/types';
  let { rule = { id: 'work', name: 'Work', tier: 'hard', mode: 'blocklist', enabled: true, allowance_id: null, schedule_ids: [], patterns: [{ kind: 'domain', value: 'work.test', match_subdomains: false }] }, config = null, unlocked = false, detox = [], remove = () => {} }: {
    rule?: Rule;
    config?: ConfigSnapshot | null; unlocked?: boolean; detox?: string[]; remove?: () => void;
  } = $props();
  let draft = $state<Rule>(structuredClone((() => rule)()));
</script>
<SiteListsView config={config} activeDetoxSiteRuleIds={detox} bind:ruleDraft={draft} ruleSaving={false} ruleMessage={null}
  tier1EditUnlocked={unlocked} onSelectRule={() => {}} onStartNewRule={() => {}}
  onSaveRuleDraft={() => {}} onRemoveRuleDraft={remove} />
