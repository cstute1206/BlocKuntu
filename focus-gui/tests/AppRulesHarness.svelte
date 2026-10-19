<script lang="ts">
  import AppRulesView from '../src/components/views/AppRulesView.svelte';
  import { mergeAppMatchers, detectedMatchersForRunningApp } from '../src/lib/ui';
  import type { ConfigSnapshot, RunningApp, AppRule } from '../src/lib/types';
  let { rule, save, config = null, unlocked = false, detox = [], remove = () => {}, runningApps = [] }: {
    rule: AppRule; save: () => void; config?: ConfigSnapshot | null; unlocked?: boolean;
    detox?: string[]; remove?: () => void; runningApps?: RunningApp[];
  } = $props();
  let draft = $state(structuredClone((() => rule)()));
</script>
<AppRulesView config={config} runningApps={runningApps} tier1EditUnlocked={unlocked} activeDetoxAppRuleIds={detox} runningAppsError={null} runningAppsLoading={false}
  bind:appRuleDraft={draft} appRuleSaving={false} appRuleMessage={null}
  onSelectAppRule={() => {}} onStartNewAppRule={() => {}} onAddDetectedMatchers={(app) => { draft.matchers = mergeAppMatchers(draft.matchers, detectedMatchersForRunningApp(app)); }}
  onAddAllDetectedMatchers={() => {}} onRefreshRunningApps={() => {}}
  onSaveAppRuleDraft={save} onRemoveAppRuleDraft={remove} />
