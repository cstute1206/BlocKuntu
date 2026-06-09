<script lang="ts">
  import { onMount } from "svelte";
  import {
    AlertTriangle,
    BarChart3,
    CalendarDays,
    CheckCircle2,
    Gamepad2,
    LayoutDashboard,
    ListChecks,
    LockKeyhole,
    RefreshCw,
    Settings,
    XCircle
  } from "@lucide/svelte";
  import AdminView from "./components/views/AdminView.svelte";
  import AppRulesView from "./components/views/AppRulesView.svelte";
  import OverviewView from "./components/views/OverviewView.svelte";
  import SchedulesView from "./components/views/SchedulesView.svelte";
  import SiteListsView from "./components/views/SiteListsView.svelte";
  import StatisticsView from "./components/views/StatisticsView.svelte";
  import {
    configSnapshot,
    daemonRpc,
    daemonStatus,
    deleteAppRule,
    deleteSchedule,
    deleteSiteList,
    enforcementStatus,
    evaluateUrl,
    recentEvents,
    requestUnlock,
    startEnforcement,
    stopEnforcement,
    systemHealth,
    tier1EditKey,
    tier1EditStatus,
    uninstallBlockuntu,
    uninstallConfirmationPhrase,
    unlockTier1Edit,
    upsertAllowance,
    upsertAppRule,
    upsertSchedule,
    upsertSiteList
  } from "./lib/api";
  import {
    cloneAllowance,
    cloneAllowanceForRule,
    cloneAppRule,
    cloneRule,
    cloneSchedule,
    defaultAllowanceForRule,
    firstRunOverviewDismissed,
    formatError,
    markFirstRunOverviewDismissed,
    nextAvailableIndexedId,
    normalizeAllowanceDraft,
    normalizeAppRuleDraft,
    normalizeRuleDraft,
    normalizeScheduleDraft
  } from "./lib/ui";
  import type {
    Allowance,
    AppRule,
    ConfigSnapshot,
    DaemonStatus,
    DecisionResult,
    EnforcementStatus,
    RecentEvent,
    Rule,
    Schedule,
    SystemHealth,
    UninstallResult,
    UnlockResult,
    ViewId
  } from "./lib/types";

  type Icon = typeof LayoutDashboard;

  const navItems: Array<{ id: ViewId; label: string; icon: Icon }> = [
    { id: "overview", label: "Dashboard", icon: LayoutDashboard },
    { id: "blocks", label: "Lists", icon: ListChecks },
    { id: "apps", label: "Apps", icon: Gamepad2 },
    { id: "schedule", label: "Schedule", icon: CalendarDays },
    { id: "statistics", label: "Statistics", icon: BarChart3 },
    { id: "admin", label: "Admin", icon: Settings }
  ];

  let activeView: ViewId = $state("overview");
  let socketPath = $state("");
  let status = $state<DaemonStatus | null>(null);
  let enforcement = $state<EnforcementStatus | null>(null);
  let health = $state<SystemHealth | null>(null);
  let config = $state<ConfigSnapshot | null>(null);
  let events = $state<RecentEvent[]>([]);
  let loading = $state(false);
  let enforcementChanging = $state(false);
  let enforcementMessage: string | null = $state(null);
  let lastError: string | null = $state(null);
  let lastRefresh: string | null = $state(null);
  let showFirstRunOverview = $state(false);

  let testUrl = $state("https://youtube.com/");
  let urlDecision = $state<DecisionResult | null>(null);
  let urlChecking = $state(false);

  let unlockTarget = $state("https://youtube.com/");
  let unlockReason = $state("Task-related access");
  let unlockResult = $state<UnlockResult | null>(null);
  let unlocking = $state(false);

  let selectedRuleId = $state<string | null>(null);
  let ruleDraft = $state<Rule | null>(null);
  let ruleAllowanceDraft = $state<Allowance | null>(null);
  let ruleSaving = $state(false);
  let ruleMessage: string | null = $state(null);

  let selectedAppRuleId = $state<string | null>(null);
  let appRuleDraft = $state<AppRule | null>(null);
  let appRuleSaving = $state(false);
  let appRuleMessage: string | null = $state(null);

  let selectedScheduleId = $state<string | null>(null);
  let scheduleDraft = $state<Schedule | null>(null);
  let scheduleSaving = $state(false);
  let scheduleMessage: string | null = $state(null);

  let rawMethod = $state("status");
  let rawParams = $state("{}");
  let rawResult = $state("");
  let rawRunning = $state(false);

  let uninstallPhrase: string | null = $state(null);
  let uninstallPhraseLoading = $state(false);
  let uninstallPhraseError: string | null = $state(null);
  let uninstallPhraseInput = $state("");
  let uninstallRunning = $state(false);
  let uninstallResult: UninstallResult | null = $state(null);

  let tier1EditKeyValue: string | null = $state(null);
  let tier1EditKeyLoading = $state(false);
  let tier1EditKeyError: string | null = $state(null);
  let tier1EditPhraseInput = $state("");
  let tier1EditUnlocking = $state(false);
  let tier1EditUnlockedUntil: string | null = $state(null);
  let tier1EditMessage: string | null = $state(null);
  let nowMs = $state(Date.now());

  let daemonOnline = $derived(status?.status === "ok");
  let activeViewTitle = $derived(
    navItems.find((item) => item.id === activeView)?.label ?? "Dashboard"
  );
  let tier1EditUnlocked = $derived(
    Boolean(tier1EditUnlockedUntil && Date.parse(tier1EditUnlockedUntil) > nowMs)
  );
  let tier1EditRemainingSeconds = $derived(
    tier1EditUnlockedUntil ? Math.max(0, Math.ceil((Date.parse(tier1EditUnlockedUntil) - nowMs) / 1000)) : 0
  );

  onMount(() => {
    const interval = window.setInterval(() => {
      nowMs = Date.now();
    }, 1000);
    showFirstRunOverview = !firstRunOverviewDismissed();
    void loadUninstallPhrase();
    void loadTier1EditKey();
    void refreshAll();
    return () => window.clearInterval(interval);
  });

  function socketArg(): string | undefined {
    const trimmed = socketPath.trim();
    return trimmed.length > 0 ? trimmed : undefined;
  }

  async function refreshAll(): Promise<void> {
    loading = true;
    lastError = null;

    const [
      statusResult,
      enforcementResult,
      configResult,
      eventsResult,
      healthResult,
      tier1EditStatusResult
    ] =
      await Promise.allSettled([
        daemonStatus(socketArg()),
        enforcementStatus(socketArg()),
        configSnapshot(socketArg()),
        recentEvents(80, socketArg()),
        systemHealth(socketArg()),
        tier1EditStatus(socketArg())
      ]);

    if (statusResult.status === "fulfilled") {
      status = statusResult.value;
    } else {
      status = null;
      lastError = formatError(statusResult.reason);
    }

    if (enforcementResult.status === "fulfilled") {
      enforcement = enforcementResult.value;
    } else {
      enforcement = null;
    }

    if (configResult.status === "fulfilled") {
      config = configResult.value;
      syncConfigSelection(configResult.value);
    }

    if (eventsResult.status === "fulfilled") {
      events = eventsResult.value.events;
    }

    if (healthResult.status === "fulfilled") {
      health = healthResult.value;
    }

    if (tier1EditStatusResult.status === "fulfilled") {
      tier1EditUnlockedUntil = tier1EditStatusResult.value.active
        ? (tier1EditStatusResult.value.expires_at ?? null)
        : null;
    }

    lastRefresh = new Date().toLocaleTimeString();
    loading = false;
  }

  function syncConfigSelection(snapshot: ConfigSnapshot): void {
    const selectedRuleSnapshot =
      snapshot.rules.find((rule) => rule.id === selectedRuleId) ?? null;
    if (!selectedRuleSnapshot) {
      selectedRuleId = snapshot.rules[0]?.id ?? null;
      setRuleDraft(snapshot.rules[0] ?? null, snapshot);
    } else if (!ruleDraft) {
      setRuleDraft(selectedRuleSnapshot, snapshot);
    }

    const selectedAppRuleSnapshot =
      snapshot.app_rules.find((rule) => rule.id === selectedAppRuleId) ?? null;
    if (!selectedAppRuleSnapshot) {
      selectedAppRuleId = snapshot.app_rules[0]?.id ?? null;
      appRuleDraft = snapshot.app_rules[0] ? cloneAppRule(snapshot.app_rules[0]) : null;
    } else if (!appRuleDraft) {
      appRuleDraft = cloneAppRule(selectedAppRuleSnapshot);
    }

    const selectedScheduleSnapshot =
      snapshot.schedules.find((schedule) => schedule.id === selectedScheduleId) ?? null;
    if (!selectedScheduleSnapshot) {
      selectedScheduleId = snapshot.schedules[0]?.id ?? null;
      scheduleDraft = snapshot.schedules[0] ? cloneSchedule(snapshot.schedules[0]) : null;
    } else if (!scheduleDraft) {
      scheduleDraft = cloneSchedule(selectedScheduleSnapshot);
    }
  }

  async function runUrlCheck(): Promise<void> {
    urlChecking = true;
    urlDecision = null;
    lastError = null;
    try {
      urlDecision = await evaluateUrl(testUrl, socketArg());
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      urlChecking = false;
    }
  }

  async function runUnlock(): Promise<void> {
    unlocking = true;
    unlockResult = null;
    lastError = null;
    try {
      unlockResult = await requestUnlock(unlockTarget, 2, unlockReason, socketArg());
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      unlocking = false;
    }
  }

  async function refreshEventsOnly(): Promise<void> {
    try {
      events = (await recentEvents(80, socketArg())).events;
    } catch {
      // Full refresh surfaces connection errors; lightweight event refresh does not interrupt actions.
    }
  }

  async function runRawRpc(): Promise<void> {
    rawRunning = true;
    rawResult = "";
    lastError = null;
    try {
      const parsed = rawParams.trim() ? JSON.parse(rawParams) : {};
      const result = await daemonRpc(rawMethod, parsed, socketArg());
      rawResult = JSON.stringify(result, null, 2);
    } catch (error) {
      lastError = formatError(error);
      rawResult = "";
    } finally {
      rawRunning = false;
    }
  }

  async function runStartEnforcement(): Promise<void> {
    enforcementChanging = true;
    enforcementMessage = null;
    lastError = null;
    try {
      enforcement = await startEnforcement(socketArg());
      enforcementMessage = "Started.";
      await refreshAll();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      enforcementChanging = false;
    }
  }

  async function runStopEnforcement(): Promise<void> {
    enforcementChanging = true;
    enforcementMessage = null;
    lastError = null;
    try {
      enforcement = await stopEnforcement(socketArg());
      enforcementMessage = "Stopped.";
      await refreshAll();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      enforcementChanging = false;
    }
  }

  async function loadUninstallPhrase(): Promise<void> {
    uninstallPhraseLoading = true;
    uninstallPhraseError = null;
    try {
      uninstallPhrase = (await uninstallConfirmationPhrase()).phrase;
    } catch (error) {
      uninstallPhrase = null;
      uninstallPhraseError = formatError(error);
    } finally {
      uninstallPhraseLoading = false;
    }
  }

  async function loadTier1EditKey(): Promise<void> {
    tier1EditKeyLoading = true;
    tier1EditKeyError = null;
    try {
      tier1EditKeyValue = (await tier1EditKey()).key;
    } catch (error) {
      tier1EditKeyValue = null;
      tier1EditKeyError = formatError(error);
    } finally {
      tier1EditKeyLoading = false;
    }
  }

  async function runUnlockTier1Edit(): Promise<void> {
    if (!tier1EditPhraseInput.trim()) return;
    tier1EditUnlocking = true;
    tier1EditMessage = null;
    lastError = null;
    try {
      const result = await unlockTier1Edit(tier1EditPhraseInput, socketArg());
      tier1EditUnlockedUntil = result.active ? (result.expires_at ?? null) : null;
      tier1EditMessage = tier1EditUnlockedUntil
        ? `Unlocked until ${new Date(tier1EditUnlockedUntil).toLocaleTimeString()}.`
        : "Tier 1 edit window is not active.";
      tier1EditPhraseInput = "";
    } catch (error) {
      tier1EditMessage = null;
      lastError = formatError(error);
    } finally {
      tier1EditUnlocking = false;
    }
  }

  async function runUninstallBlockuntu(): Promise<void> {
    if (!uninstallPhrase || !uninstallPhraseInput.trim()) return;
    uninstallRunning = true;
    uninstallResult = null;
    lastError = null;
    try {
      uninstallResult = await uninstallBlockuntu(uninstallPhraseInput);
    } catch (error) {
      lastError = formatError(error);
    } finally {
      uninstallRunning = false;
    }
  }

  function setRuleDraft(rule: Rule | null, snapshot: ConfigSnapshot | null = config): void {
    ruleDraft = rule ? cloneRule(rule) : null;
    ruleAllowanceDraft = rule ? cloneAllowanceForRule(rule, snapshot) : null;
  }

  function selectRule(rule: Rule): void {
    selectedRuleId = rule.id;
    setRuleDraft(rule, config);
    ruleMessage = null;
  }

  function startNewRule(): void {
    const { id, index } = nextAvailableIndexedId(
      config?.rules.map((rule) => rule.id) ?? [],
      "site-list"
    );
    selectedRuleId = null;
    const newRule: Rule = {
      id,
      name: `Site list ${index}`,
      tier: "controlled_access",
      enabled: true,
      patterns: [{ kind: "domain", value: "example.com", match_subdomains: true }],
      schedule_ids: [],
      allowance_id: null,
      unlock_policy: null
    };
    ruleDraft = newRule;
    ruleAllowanceDraft = defaultAllowanceForRule(newRule);
    ruleMessage = null;
  }

  async function saveRuleDraft(): Promise<void> {
    if (!ruleDraft) return;
    ruleSaving = true;
    lastError = null;
    ruleMessage = null;
    try {
      const socket = socketArg();
      if (ruleDraft.tier === "controlled_access") {
        const allowance = normalizeAllowanceDraft(
          ruleAllowanceDraft ?? defaultAllowanceForRule(ruleDraft),
          ruleDraft
        );
        const allowanceResponse = await upsertAllowance(allowance, socket);
        config = allowanceResponse.config;
        ruleAllowanceDraft = cloneAllowance(allowance);
        ruleDraft.allowance_id = allowance.id;
      } else {
        ruleDraft.allowance_id = null;
      }

      const response = await upsertSiteList(normalizeRuleDraft(ruleDraft), socket);
      config = response.config;
      selectedRuleId = ruleDraft.id.trim();
      setRuleDraft(
        response.config.rules.find((rule) => rule.id === selectedRuleId) ??
          response.config.rules[0] ??
          null,
        response.config
      );
      ruleMessage = "Saved.";
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      ruleSaving = false;
    }
  }

  async function removeRuleDraft(): Promise<void> {
    if (!ruleDraft || !config?.rules.some((rule) => rule.id === ruleDraft?.id)) return;
    ruleSaving = true;
    lastError = null;
    ruleMessage = null;
    try {
      const response = await deleteSiteList(ruleDraft.id, socketArg());
      config = response.config;
      selectedRuleId = response.config.rules[0]?.id ?? null;
      setRuleDraft(response.config.rules[0] ?? null, response.config);
      ruleMessage = "Deleted.";
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      ruleSaving = false;
    }
  }

  function selectAppRule(rule: AppRule): void {
    selectedAppRuleId = rule.id;
    appRuleDraft = cloneAppRule(rule);
    appRuleMessage = null;
  }

  function startNewAppRule(): void {
    const { id, index } = nextAvailableIndexedId(
      config?.app_rules.map((rule) => rule.id) ?? [],
      "app-rule"
    );
    selectedAppRuleId = null;
    appRuleDraft = {
      id,
      name: `App rule ${index}`,
      tier: "hard",
      enabled: true,
      matchers: [{ kind: "command_name", value: "kmines" }],
      schedule_ids: [],
      allowance_id: null,
      unlock_policy: null
    };
    appRuleMessage = null;
  }

  async function saveAppRuleDraft(): Promise<void> {
    if (!appRuleDraft) return;
    appRuleSaving = true;
    lastError = null;
    appRuleMessage = null;
    try {
      const response = await upsertAppRule(normalizeAppRuleDraft(appRuleDraft), socketArg());
      config = response.config;
      selectedAppRuleId = appRuleDraft.id.trim();
      appRuleDraft = cloneAppRule(
        response.config.app_rules.find((rule) => rule.id === selectedAppRuleId) ??
          response.config.app_rules[0]
      );
      appRuleMessage = "Saved.";
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      appRuleSaving = false;
    }
  }

  async function removeAppRuleDraft(): Promise<void> {
    if (!appRuleDraft || !config?.app_rules.some((rule) => rule.id === appRuleDraft?.id)) return;
    appRuleSaving = true;
    lastError = null;
    appRuleMessage = null;
    try {
      const response = await deleteAppRule(appRuleDraft.id, socketArg());
      config = response.config;
      selectedAppRuleId = response.config.app_rules[0]?.id ?? null;
      appRuleDraft = response.config.app_rules[0] ? cloneAppRule(response.config.app_rules[0]) : null;
      appRuleMessage = "Deleted.";
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      appRuleSaving = false;
    }
  }

  function selectSchedule(schedule: Schedule): void {
    selectedScheduleId = schedule.id;
    scheduleDraft = cloneSchedule(schedule);
    scheduleMessage = null;
  }

  function startNewSchedule(): void {
    const { id, index } = nextAvailableIndexedId(
      config?.schedules.map((schedule) => schedule.id) ?? [],
      "schedule"
    );
    selectedScheduleId = null;
    scheduleDraft = {
      id,
      name: `Schedule ${index}`,
      windows: [{ weekday: "workdays", start: "09:00", end: "17:00" }]
    };
    scheduleMessage = null;
  }

  async function saveScheduleDraft(): Promise<void> {
    if (!scheduleDraft) return;
    scheduleSaving = true;
    lastError = null;
    scheduleMessage = null;
    try {
      const response = await upsertSchedule(normalizeScheduleDraft(scheduleDraft), socketArg());
      config = response.config;
      selectedScheduleId = scheduleDraft.id.trim();
      scheduleDraft = cloneSchedule(
        response.config.schedules.find((schedule) => schedule.id === selectedScheduleId) ??
          response.config.schedules[0]
      );
      scheduleMessage = "Saved.";
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      scheduleSaving = false;
    }
  }

  async function removeScheduleDraft(): Promise<void> {
    if (!scheduleDraft || !config?.schedules.some((schedule) => schedule.id === scheduleDraft?.id)) {
      return;
    }
    scheduleSaving = true;
    lastError = null;
    scheduleMessage = null;
    try {
      const response = await deleteSchedule(scheduleDraft.id, socketArg());
      config = response.config;
      selectedScheduleId = response.config.schedules[0]?.id ?? null;
      scheduleDraft = response.config.schedules[0] ? cloneSchedule(response.config.schedules[0]) : null;
      scheduleMessage = "Deleted.";
      await refreshEventsOnly();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      scheduleSaving = false;
    }
  }

  function dismissFirstRunOverview(): void {
    showFirstRunOverview = false;
    markFirstRunOverviewDismissed();
  }
</script>

<div class="app-shell">
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark"><LockKeyhole size={22} /></div>
      <div>
        <strong>BlocKuntu</strong>
        <span>Focus control</span>
      </div>
    </div>

    <nav class="nav-list" aria-label="Main">
      {#each navItems as item (item.id)}
        {@const IconComponent = item.icon}
        <button
          class:active={activeView === item.id}
          title={item.label}
          onclick={() => (activeView = item.id)}
        >
          <IconComponent size={18} aria-hidden="true" />
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>

    <div class="daemon-strip" class:offline={!daemonOnline}>
      {#if daemonOnline}
        <CheckCircle2 size={18} aria-hidden="true" />
        <span>Daemon online</span>
      {:else}
        <XCircle size={18} aria-hidden="true" />
        <span>Daemon offline</span>
      {/if}
    </div>
  </aside>

  <main class="workspace">
    <header class="topbar">
      <div>
        <p class="eyebrow">Local enforcement</p>
        <h1>{activeViewTitle}</h1>
      </div>
      <div class="topbar-actions">
        <label class="socket-field">
          <span>Socket</span>
          <input
            bind:value={socketPath}
            placeholder="Auto: /run/blockuntu/blockuntud.sock, then /tmp/blockuntu/blockuntud.sock"
            spellcheck="false"
          />
        </label>
        <button class="icon-button" title="Refresh" onclick={refreshAll} disabled={loading}>
          <span class:spin={loading}>
            <RefreshCw size={18} aria-hidden="true" />
          </span>
        </button>
      </div>
    </header>

    {#if lastError}
      <section class="alert-row" role="status">
        <AlertTriangle size={18} aria-hidden="true" />
        <span>{lastError}</span>
      </section>
    {/if}

    {#if activeView === "overview"}
      <OverviewView
        {status}
        {enforcement}
        {health}
        {config}
        {showFirstRunOverview}
        bind:testUrl
        {urlDecision}
        {urlChecking}
        bind:unlockTarget
        bind:unlockReason
        {unlockResult}
        {unlocking}
        {uninstallPhrase}
        {uninstallPhraseLoading}
        {uninstallPhraseError}
        tier1EditKey={tier1EditKeyValue}
        {tier1EditKeyLoading}
        {tier1EditKeyError}
        onDismissFirstRunOverview={dismissFirstRunOverview}
        onRunUrlCheck={runUrlCheck}
        onRunUnlock={runUnlock}
      />
    {:else if activeView === "blocks"}
      <SiteListsView
        {config}
        bind:ruleDraft
        bind:ruleAllowanceDraft
        {ruleSaving}
        {ruleMessage}
        {tier1EditUnlocked}
        onSelectRule={selectRule}
        onStartNewRule={startNewRule}
        onSaveRuleDraft={saveRuleDraft}
        onRemoveRuleDraft={removeRuleDraft}
      />
    {:else if activeView === "apps"}
      <AppRulesView
        {config}
        bind:appRuleDraft
        {appRuleSaving}
        {appRuleMessage}
        onSelectAppRule={selectAppRule}
        onStartNewAppRule={startNewAppRule}
        onSaveAppRuleDraft={saveAppRuleDraft}
        onRemoveAppRuleDraft={removeAppRuleDraft}
      />
    {:else if activeView === "schedule"}
      <SchedulesView
        {config}
        bind:scheduleDraft
        {scheduleSaving}
        {scheduleMessage}
        onSelectSchedule={selectSchedule}
        onStartNewSchedule={startNewSchedule}
        onSaveScheduleDraft={saveScheduleDraft}
        onRemoveScheduleDraft={removeScheduleDraft}
      />
    {:else if activeView === "statistics"}
      <StatisticsView {events} />
    {:else if activeView === "admin"}
      <AdminView
        {status}
        {enforcement}
        {health}
        {enforcementChanging}
        {enforcementMessage}
        bind:rawMethod
        bind:rawParams
        {rawResult}
        {rawRunning}
        {uninstallPhrase}
        bind:uninstallPhraseInput
        {uninstallRunning}
        {uninstallResult}
        {uninstallPhraseError}
        bind:tier1EditPhraseInput
        {tier1EditUnlocking}
        {tier1EditUnlocked}
        {tier1EditUnlockedUntil}
        {tier1EditRemainingSeconds}
        {tier1EditMessage}
        {tier1EditKeyError}
        onStartEnforcement={runStartEnforcement}
        onStopEnforcement={runStopEnforcement}
        onRunRawRpc={runRawRpc}
        onRunUninstallBlockuntu={runUninstallBlockuntu}
        onUnlockTier1Edit={runUnlockTier1Edit}
      />
    {/if}

    <footer class="footer-line">
      <span>{lastRefresh ? `Last refresh ${lastRefresh}` : "Not refreshed"}</span>
      <span>{health?.socket_path ?? socketPath}</span>
    </footer>
  </main>
</div>
