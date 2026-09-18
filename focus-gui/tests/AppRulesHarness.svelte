<script lang="ts">
  import AppRulesView from '../src/components/views/AppRulesView.svelte';
  import type { AppRule } from '../src/lib/types';
  let { rule, save }: { rule: AppRule; save: () => void } = $props();
  let draft = $state(structuredClone((() => rule)()));
</script>
<AppRulesView config={null} runningApps={[]} runningAppsError={null} runningAppsLoading={false}
  bind:appRuleDraft={draft} appRuleSaving={false} appRuleMessage={null}
  onSelectAppRule={() => {}} onStartNewAppRule={() => {}} onAddDetectedMatchers={() => {}}
  onAddAllDetectedMatchers={() => {}} onRefreshRunningApps={() => {}}
  onSaveAppRuleDraft={save} onRemoveAppRuleDraft={() => {}} />
