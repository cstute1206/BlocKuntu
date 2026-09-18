<script lang="ts">
  import {
    AlertTriangle,
    ChevronDown,
    ChevronRight,
    Gamepad2,
    Plus,
    RefreshCw,
    Save,
    Trash2,
    XCircle
  } from "@lucide/svelte";
  import { tick } from "svelte";
  import { appMatcherKinds, appRuleIsActive, defaultAllowanceForRule } from "../../lib/ui";
  import type { Allowance, AppRule, ConfigSnapshot, RunningApp } from "../../lib/types";

  interface Props {
    config: ConfigSnapshot | null;
    runningApps: RunningApp[];
    runningAppsError: string | null;
    runningAppsLoading: boolean;
    appRuleDraft?: AppRule | null;
    appRuleAllowanceDraft?: Allowance | null;
    appRuleSaving: boolean;
    appRuleMessage: string | null;
    activeDetoxAppRuleIds?: string[];
    onSelectAppRule: (rule: AppRule) => void;
    onStartNewAppRule: () => void;
    onAddDetectedMatchers: (app: RunningApp) => void;
    onAddAllDetectedMatchers: () => void;
    onRefreshRunningApps: () => void | Promise<void>;
    onSaveAppRuleDraft: () => void | Promise<void>;
    onRemoveAppRuleDraft: () => void | Promise<void>;
  }

  let {
    config,
    runningApps,
    runningAppsError,
    runningAppsLoading,
    appRuleDraft = $bindable<AppRule | null>(null),
    appRuleAllowanceDraft = $bindable<Allowance | null>(null),
    appRuleSaving,
    appRuleMessage,
    activeDetoxAppRuleIds = [],
    onSelectAppRule,
    onStartNewAppRule,
    onAddDetectedMatchers,
    onAddAllDetectedMatchers,
    onRefreshRunningApps,
    onSaveAppRuleDraft,
    onRemoveAppRuleDraft
  }: Props = $props();

  let savedAppRule = $derived(
    appRuleDraft ? (config?.app_rules.find((rule) => rule.id === appRuleDraft?.id) ?? null) : null
  );
  let appRuleDraftIsExisting = $derived(Boolean(savedAppRule));
  let appRuleDraftDetoxLocked = $derived(
    Boolean(savedAppRule && activeDetoxAppRuleIds.includes(savedAppRule.id))
  );
  let appRuleDraftActive = $derived(
    Boolean(savedAppRule && appRuleIsActive(savedAppRule, config?.schedules ?? []))
  );
  let appRuleDraftEditLocked = $derived(
    Boolean(savedAppRule && (appRuleDraftDetoxLocked || appRuleDraftActive))
  );
  let savedMatcherCount = $derived(savedAppRule?.matchers.length ?? 0);
  let detectedAppSearch = $state("");
  let filteredRunningApps = $derived(
    runningApps.filter((app) => detectedAppMatchesSearch(app, detectedAppSearch))
  );
  let matcherValueInputs: HTMLInputElement[] = [];
  let matcherListExpanded = $state(false);
  let detectedAppsExpanded = $state(false);
  let appRuleSaveWarningOpen = $state(false);

  function detectedAppMatchesSearch(app: RunningApp, search: string): boolean {
    const query = search.trim().toLocaleLowerCase();
    if (!query) return true;

    return [
      app.display_name,
      app.command_name,
      app.executable_basename,
      app.executable_path,
      app.desktop_id,
      app.window_titles.join(" "),
      app.blocking_rule_name
    ].some((value) => value?.toLocaleLowerCase().includes(query));
  }

  function matcherIsSaved(index: number): boolean {
    return Boolean(savedAppRule && index < savedMatcherCount);
  }

  function matcherEditLocked(index: number): boolean {
    return matcherIsSaved(index) && appRuleDraftEditLocked;
  }

  function matcherRemoveLocked(index: number): boolean {
    if (savedAppRule?.mode === "allowlist") return false;
    return (appRuleDraftDetoxLocked || appRuleDraftActive) && matcherIsSaved(index);
  }

  function matcherAddLocked(): boolean {
    return Boolean(
      savedAppRule?.mode === "allowlist" &&
        (appRuleDraftDetoxLocked || appRuleDraftActive)
    );
  }

  function addAppMatcher(): void {
    if (!appRuleDraft || matcherAddLocked()) return;
    appRuleDraft.matchers = [...appRuleDraft.matchers, { kind: "command_name", value: "" }];
    if (appRuleDraft.mode === "allowlist") matcherListExpanded = true;
  }

  async function addAppMatcherOnEnter(event: KeyboardEvent, index: number): Promise<void> {
    if (
      event.key !== "Enter" ||
      event.isComposing ||
      !appRuleDraft ||
      index !== appRuleDraft.matchers.length - 1 ||
      matcherAddLocked() ||
      matcherEditLocked(index) ||
      !appRuleDraft.matchers[index].value.trim()
    ) {
      return;
    }

    event.preventDefault();
    addAppMatcher();
    await tick();
    matcherValueInputs[appRuleDraft.matchers.length - 1]?.focus();
  }

  function removeAppMatcher(index: number): void {
    if (!appRuleDraft || appRuleDraft.matchers.length <= 1) return;
    appRuleDraft.matchers = appRuleDraft.matchers.filter(
      (_, matcherIndex) => matcherIndex !== index
    );
  }

  function setAppRuleTier(tier: AppRule["tier"]): void {
    if (!appRuleDraft) return;
    if (appRuleDraft.mode === "allowlist" && tier === "hard") return;
    appRuleDraft.tier = tier;
    if (tier !== "controlled_access") {
      appRuleDraft.allowance_id = null;
      appRuleAllowanceDraft = null;
    } else if (!appRuleAllowanceDraft) {
      appRuleAllowanceDraft = defaultAllowanceForRule(appRuleDraft);
    }
  }

  function setAppRuleMode(mode: AppRule["mode"]): void {
    if (!appRuleDraft) return;
    appRuleDraft.mode = mode;
    appRuleDraft.enabled = true;
    if (mode === "allowlist" && appRuleDraft.tier === "hard") {
      setAppRuleTier("scheduled_block");
    }
    if (mode === "allowlist") {
      appRuleDraft.matchers = appRuleDraft.matchers.filter(
        (matcher) => matcher.value.trim().length > 0
      );
    }
    matcherListExpanded = mode === "blocklist";
  }

  function requestAppRuleSave(): void {
    if (appRuleDraft?.mode === "allowlist") {
      appRuleSaveWarningOpen = true;
      return;
    }
    void onSaveAppRuleDraft();
  }

  function confirmAppRuleSave(): void {
    appRuleSaveWarningOpen = false;
    void onSaveAppRuleDraft();
  }

  function closeAppRuleSaveWarning(): void {
    appRuleSaveWarningOpen = false;
  }

  function closeAppRuleSaveWarningOnEscape(event: KeyboardEvent): void {
    if (event.key === "Escape") closeAppRuleSaveWarning();
  }

  function closeAppRuleSaveWarningFromBackdrop(event: MouseEvent): void {
    if (event.currentTarget === event.target) closeAppRuleSaveWarning();
  }

  function toggleAppRuleSchedule(scheduleId: string): void {
    if (!appRuleDraft) return;
    if (appRuleDraft.schedule_ids.includes(scheduleId)) {
      appRuleDraft.schedule_ids = appRuleDraft.schedule_ids.filter((id) => id !== scheduleId);
    } else {
      appRuleDraft.schedule_ids = [...appRuleDraft.schedule_ids, scheduleId];
    }
  }
</script>

<section class="split-view">
  <article class="panel list-panel">
    <div class="panel-title">
      <Gamepad2 size={18} aria-hidden="true" />
      <h2>Applications</h2>
    </div>
    <button class="secondary wide-button" onclick={onStartNewAppRule}>
      <Plus size={17} aria-hidden="true" />
      <span>New application</span>
    </button>
    <div class="rule-list">
      {#each config?.app_rules ?? [] as rule (rule.id)}
        <button
          class:active={appRuleDraft?.id === rule.id}
          onclick={() => onSelectAppRule(rule)}
        >
          <span class:hard={rule.tier === "hard"} class="tier-dot"></span>
          <span>{rule.name}</span>
          <em>
            {rule.tier === "hard" ? "Tier 1" : rule.tier === "scheduled_block" ? "Tier 2" : "Tier 3"}
            {rule.mode === "allowlist" ? " · Allowlist" : ""}
          </em>
        </button>
      {:else}
        <p class="empty-state">No application lists yet.</p>
      {/each}
    </div>
  </article>

  <article class="panel detail-panel">
    <div class="panel-title">
      <Gamepad2 size={18} aria-hidden="true" />
      <h2>{appRuleDraft?.name || "Application"}</h2>
    </div>
    {#if appRuleDraft}
      {#if appRuleDraftDetoxLocked || appRuleDraftActive}
        <section class="inline-warning">
          <AlertTriangle size={17} aria-hidden="true" />
          <span>
            {appRuleDraftDetoxLocked
              ? appRuleDraft.mode === "allowlist"
                ? "This application allowlist is covered by an active Detox. Allowed identities can be removed, but new applications cannot be added."
                : "This application is covered by an active Detox. Existing settings are locked, but you can append matchers."
              : appRuleDraft.mode === "allowlist"
                ? "This application allowlist is active right now. Allowed identities can be removed, but new applications cannot be added."
                : "This application is active right now. Existing settings are locked, but you can append matchers."}
          </span>
        </section>
      {/if}
      <div class="form-grid">
        <label>
          <span>Name</span>
          <input bind:value={appRuleDraft.name} disabled={appRuleDraftEditLocked} />
        </label>
        <label>
          <span>Tier</span>
          <select
            value={appRuleDraft.tier}
            disabled={appRuleDraftEditLocked}
            onchange={(event) => setAppRuleTier(event.currentTarget.value as AppRule["tier"])}
          >
            <option value="hard" disabled={appRuleDraft.mode === "allowlist"}>Tier 1</option>
            <option value="scheduled_block">Tier 2</option>
            <option value="controlled_access">Tier 3</option>
          </select>
        </label>
        <label>
          <span>List behavior</span>
          <select
            value={appRuleDraft.mode}
            disabled={appRuleDraftEditLocked}
            onchange={(event) => setAppRuleMode(event.currentTarget.value as AppRule["mode"])}
          >
            <option value="blocklist">Block listed applications</option>
            <option value="allowlist">Allow only listed applications</option>
          </select>
        </label>
      </div>

      {#if appRuleDraft.tier === "controlled_access"}
        <div class="section-label">Daily allowance</div>
        <div class="allowance-editor">
          {#if appRuleAllowanceDraft}
            <label>
              <span>Daily minutes</span>
              <input
                type="number"
                min="0"
                max="1440"
                bind:value={appRuleAllowanceDraft.daily_minutes}
                disabled={appRuleDraftEditLocked}
              />
            </label>
          {/if}
        </div>
      {/if}

      <div class="section-label">Schedules</div>
      {#if appRuleDraft.tier !== "hard"}
        <p class="tier2-schedule-note">
          {appRuleDraft.tier === "scheduled_block"
            ? appRuleDraft.mode === "allowlist"
              ? "Tier 2 closes non-allowlisted regular-user processes during an attached schedule or Detox and cannot be unlocked."
              : "Tier 2 applications block strictly during an attached schedule or Detox and cannot be unlocked."
            : appRuleDraft.mode === "allowlist"
              ? "Tier 3 counts time while a non-allowlisted regular-user process is running. After the allowance is used, it is closed unless manually unlocked."
              : "Tier 3 applications use allowances and manual unlocks during an attached schedule or Detox."}
        </p>
        {#if appRuleDraft.schedule_ids.length === 0 && !appRuleDraftDetoxLocked}
          <section class="inline-warning">
            <AlertTriangle size={17} aria-hidden="true" />
            <span>
              No schedule is attached. This {appRuleDraft.tier === "scheduled_block" ? "Tier 2" : "Tier 3"} application stays inactive unless you select it for Detox or add it to a schedule.
            </span>
          </section>
        {/if}
      {/if}
      <div class="chip-grid">
        {#each config?.schedules ?? [] as schedule (schedule.id)}
          <label class="chip-check">
            <input
              type="checkbox"
              checked={appRuleDraft.schedule_ids.includes(schedule.id)}
              disabled={appRuleDraftEditLocked}
              onchange={() => toggleAppRuleSchedule(schedule.id)}
            />
            <span>{schedule.name ?? schedule.id}</span>
          </label>
        {:else}
          <p class="empty-state">No schedules available.</p>
        {/each}
      </div>

      <div class="collapsible-section-header">
        <div>
          <strong>{appRuleDraft.mode === "allowlist" ? "Allowed application identities" : "Application matchers"}</strong>
          <small>{appRuleDraft.matchers.length} {appRuleDraft.matchers.length === 1 ? "entry" : "entries"}</small>
        </div>
        {#if appRuleDraft.mode === "allowlist"}
          <button class="secondary" onclick={() => (matcherListExpanded = !matcherListExpanded)}>
            {#if matcherListExpanded}<ChevronDown size={16} aria-hidden="true" />{:else}<ChevronRight size={16} aria-hidden="true" />{/if}
            <span>{matcherListExpanded ? "Hide identities" : "Show identities"}</span>
          </button>
        {/if}
      </div>
      {#if appRuleDraft.mode === "blocklist" || matcherListExpanded}
        <div class="table-wrap compact-table-wrap">
          <table>
            <thead>
              <tr>
                <th>Matcher</th>
                <th>Value</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {#each appRuleDraft.matchers as matcher, index (index)}
                <tr>
                  <td>
                    <select
                      bind:value={matcher.kind}
                      disabled={matcherEditLocked(index)}
                    >
                      {#each appMatcherKinds as kind (kind.id)}
                        <option value={kind.id}>{kind.label}</option>
                      {/each}
                    </select>
                  </td>
                  <td>
                    <input
                      bind:value={matcher.value}
                      bind:this={matcherValueInputs[index]}
                      disabled={matcherEditLocked(index)}
                      aria-keyshortcuts="Enter"
                      title="Press Enter in the last matcher to add another"
                      onkeydown={(event) => addAppMatcherOnEnter(event, index)}
                    />
                  </td>
                  <td>
                    <button
                      class="icon-button"
                      title="Remove matcher"
                      onclick={() => removeAppMatcher(index)}
                      disabled={
                        matcherRemoveLocked(index) || appRuleDraft.matchers.length <= 1
                      }
                    >
                      <Trash2 size={16} aria-hidden="true" />
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}

      <div class="button-row">
        <button class="secondary" onclick={addAppMatcher} disabled={matcherAddLocked()}>
          <Plus size={17} aria-hidden="true" />
          <span>{appRuleDraft.mode === "allowlist" ? "Identity" : "Matcher"}</span>
        </button>
        <button
          class="secondary"
          onclick={onRemoveAppRuleDraft}
          disabled={
            appRuleSaving ||
            appRuleDraftDetoxLocked ||
            appRuleDraftActive ||
            !appRuleDraftIsExisting
          }
        >
          <Trash2 size={17} aria-hidden="true" />
          <span>Delete</span>
        </button>
        <button
          class="primary"
          onclick={requestAppRuleSave}
          disabled={appRuleSaving}
        >
          <Save size={17} aria-hidden="true" />
          <span>Save</span>
        </button>
      </div>
      {#if appRuleMessage}
        <p class="result-text">{appRuleMessage}</p>
      {/if}
    {/if}
    <div class="collapsible-section-header detected-app-section-header">
      <div>
        <strong>Detected processes</strong>
        <small>{runningApps.length} eligible {runningApps.length === 1 ? "process" : "processes"}</small>
      </div>
      <div class="panel-actions">
        {#if appRuleDraft?.mode === "allowlist"}
          <button
            class="secondary"
            onclick={onAddAllDetectedMatchers}
            disabled={runningApps.length === 0 || matcherAddLocked()}
          >
            <Plus size={16} aria-hidden="true" />
            <span>Add all detected</span>
          </button>
        {/if}
        <button class="secondary" onclick={() => (detectedAppsExpanded = !detectedAppsExpanded)}>
          {#if detectedAppsExpanded}<ChevronDown size={16} aria-hidden="true" />{:else}<ChevronRight size={16} aria-hidden="true" />{/if}
          <span>{detectedAppsExpanded ? "Hide" : "Expand"}</span>
        </button>
      </div>
    </div>
    {#if detectedAppsExpanded}
      <div class="detected-app-controls">
        <label class="detected-app-search">
          <span>Search</span>
          <input
            type="search"
            bind:value={detectedAppSearch}
            placeholder="Name, command, path, or desktop ID"
          />
        </label>
        <button class="secondary" onclick={onRefreshRunningApps} disabled={runningAppsLoading}>
          <RefreshCw size={16} aria-hidden="true" />
          <span>{runningAppsLoading ? "Refreshing" : "Refresh"}</span>
        </button>
      </div>
      {#if runningAppsError}
        <p class="danger-text">{runningAppsError}</p>
      {/if}
      <div class="rule-list detected-app-list">
        {#if filteredRunningApps.length > 0}
          {#each filteredRunningApps as app (`${app.pid}-${app.display_name}`)}
            <article class="detected-app-item">
              <div class="detected-app-copy">
                <div class="detected-app-header"><strong>{app.display_name}</strong></div>
                <p class="detected-app-meta">PID {app.pid}</p>
                {#if app.command_name}<p class="detected-app-meta">Command: {app.command_name}</p>{/if}
                {#if app.executable_basename}<p class="detected-app-meta">Binary: {app.executable_basename}</p>{/if}
                {#if app.executable_path}<p class="detected-app-meta">Path: {app.executable_path}</p>{/if}
                {#if app.desktop_id}<p class="detected-app-meta">Desktop ID: {app.desktop_id}</p>{/if}
                {#if app.window_titles.length > 0}<p class="detected-app-meta">Title: {app.window_titles[0]}</p>{/if}
                {#if app.blocking_rule_name}<p class="detected-app-meta">Rule: {app.blocking_rule_name}</p>{/if}
              </div>
              <button class="secondary detected-app-append" onclick={() => onAddDetectedMatchers(app)} disabled={matcherAddLocked()}>
                <Plus size={16} aria-hidden="true" />
                <span>{appRuleDraft?.mode === "allowlist" ? "Add" : "Append"}</span>
              </button>
            </article>
          {/each}
        {:else if runningApps.length > 0}
          <p class="empty-state">No detected processes match your search.</p>
        {:else}
          <p class="empty-state">No eligible regular-user processes are currently detected.</p>
        {/if}
      </div>
    {/if}
  </article>
</section>

{#if appRuleSaveWarningOpen && appRuleDraft}
  <div
    class="onboarding-backdrop"
    role="presentation"
    onclick={closeAppRuleSaveWarningFromBackdrop}
  >
    <div
      class="onboarding-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="app-allowlist-warning-title"
      aria-describedby="app-allowlist-warning-description"
      tabindex="-1"
      onkeydown={closeAppRuleSaveWarningOnEscape}
    >
      <div class="onboarding-modal-header">
        <div class="panel-title">
          <AlertTriangle size={18} aria-hidden="true" />
          <h2 id="app-allowlist-warning-title">Save this application allowlist?</h2>
        </div>
        <button class="icon-button" title="Cancel save" aria-label="Cancel save" onclick={closeAppRuleSaveWarning}><XCircle size={17} aria-hidden="true" /></button>
      </div>
      <div class="onboarding-copy" id="app-allowlist-warning-description">
        <p>
          Saving does not activate this list. During an attached schedule or Detox, BlocKuntu evaluates every eligible regular-user process independently. Allowing one process does not automatically allow its children, helpers or related processes.
        </p>
        <p>This can terminate essential parts of the desktop session, including:</p>
        <ul>
          <li><code>systemd --user</code> and D-Bus services.</li>
          <li>Desktop portals and authentication agents.</li>
          <li>Audio and clipboard infrastructure.</li>
          <li>Browser renderers, GPU processes and crash handlers.</li>
          <li>IDE language servers and terminal child processes.</li>
          <li>Tier 2 terminates non-allowlisted processes without manual unlock.</li>
          <li>Tier 3 consumes allowance while a non-allowlisted process is running and retains manual unlock.</li>
          <li>Existing application blocklists still win.</li>
        </ul>
        <p>BlocKuntu's own processes and non-desktop system accounts remain protected so recovery and uninstallation stay available.</p>
        <div class="button-row onboarding-actions">
          <button class="secondary" onclick={closeAppRuleSaveWarning}>Cancel</button>
          <button class="primary" onclick={confirmAppRuleSave} disabled={appRuleSaving}><Save size={17} aria-hidden="true" /><span>Save allowlist</span></button>
        </div>
      </div>
    </div>
  </div>
{/if}
