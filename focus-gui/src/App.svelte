<script lang="ts">
  import { onMount } from "svelte";
  import {
    Activity,
    AlertTriangle,
    BarChart3,
    CalendarDays,
    CheckCircle2,
    Clock3,
    Gauge,
    Gamepad2,
    LayoutDashboard,
    ListChecks,
    LockKeyhole,
    Play,
    Power,
    PowerOff,
    Plus,
    RefreshCw,
    Save,
    Search,
    Server,
    Settings,
    Shield,
    Terminal,
    Timer,
    Trash2,
    Unlock,
    Wrench,
    XCircle
  } from "@lucide/svelte";
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
    upsertAppRule,
    upsertSchedule,
    upsertSiteList
  } from "./lib/api";
  import type {
    AppMatcher,
    AppRule,
    ConfigSnapshot,
    DaemonStatus,
    DecisionResult,
    EnforcementStatus,
    HealthCheck,
    RecentEvent,
    Rule,
    RulePattern,
    Schedule,
    ScheduleWindow,
    SystemHealth,
    UnlockResult,
    ViewId
  } from "./lib/types";

  type Icon = typeof LayoutDashboard;

  const navItems: Array<{ id: ViewId; label: string; icon: Icon }> = [
    { id: "overview", label: "Dashboard", icon: LayoutDashboard },
    { id: "blocks", label: "Lists", icon: ListChecks },
    { id: "apps", label: "Apps", icon: Gamepad2 },
    { id: "schedule", label: "Schedule", icon: CalendarDays },
    { id: "allowances", label: "Allowances", icon: Timer },
    { id: "statistics", label: "Statistics", icon: BarChart3 },
    { id: "admin", label: "Admin", icon: Settings }
  ];

  const weekdays = [
    { id: "mon", label: "Mon" },
    { id: "tue", label: "Tue" },
    { id: "wed", label: "Wed" },
    { id: "thu", label: "Thu" },
    { id: "fri", label: "Fri" },
    { id: "sat", label: "Sat" },
    { id: "sun", label: "Sun" }
  ] as const;

  const patternKinds: Array<{ id: RulePattern["kind"]; label: string }> = [
    { id: "domain", label: "Domain" },
    { id: "exact_url", label: "Exact URL" },
    { id: "url_prefix", label: "URL prefix" },
    { id: "path_prefix", label: "Path prefix" }
  ];

  const appMatcherKinds: Array<{ id: AppMatcher["kind"]; label: string }> = [
    { id: "command_name", label: "Command" },
    { id: "executable_basename", label: "Binary" },
    { id: "executable_path", label: "Path" },
    { id: "desktop_id", label: "Desktop ID" },
    { id: "window_title_contains", label: "Title contains" },
    { id: "window_title_exact", label: "Title exact" }
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

  let testUrl = $state("https://youtube.com/");
  let urlDecision = $state<DecisionResult | null>(null);
  let urlChecking = $state(false);

  let unlockTarget = $state("youtube-controlled");
  let unlockMinutes = $state(10);
  let unlockReason = $state("Task-related access");
  let unlockResult = $state<UnlockResult | null>(null);
  let unlocking = $state(false);

  let selectedRuleId = $state<string | null>(null);
  let ruleDraft = $state<Rule | null>(null);
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

  let hardRules = $derived(config?.rules.filter((rule) => rule.tier === "hard") ?? []);
  let controlledRules = $derived(
    config?.rules.filter((rule) => rule.tier === "controlled_access") ?? []
  );
  let hardAppRules = $derived(config?.app_rules.filter((rule) => rule.tier === "hard") ?? []);
  let controlledAppRules = $derived(
    config?.app_rules.filter((rule) => rule.tier === "controlled_access") ?? []
  );
  let hardBlockCount = $derived(hardRules.length + hardAppRules.length);
  let controlledBlockCount = $derived(controlledRules.length + controlledAppRules.length);
  let selectedRule = $derived(
    config?.rules.find((rule) => rule.id === selectedRuleId) ?? config?.rules[0] ?? null
  );
  let selectedRuleIsActive = $derived(selectedRule ? ruleIsActive(selectedRule) : false);
  let ruleDraftIsExisting = $derived(
    Boolean(ruleDraft && config?.rules.some((rule) => rule.id === ruleDraft?.id))
  );
  let ruleDraftLocked = $derived(Boolean(ruleDraft && ruleDraftIsExisting && selectedRuleIsActive));
  let selectedAppRule = $derived(
    config?.app_rules.find((rule) => rule.id === selectedAppRuleId) ??
      config?.app_rules[0] ??
      null
  );
  let selectedAppRuleIsActive = $derived(
    selectedAppRule ? appRuleIsActive(selectedAppRule) : false
  );
  let appRuleDraftIsExisting = $derived(
    Boolean(appRuleDraft && config?.app_rules.some((rule) => rule.id === appRuleDraft?.id))
  );
  let appRuleDraftLocked = $derived(
    Boolean(appRuleDraft && appRuleDraftIsExisting && selectedAppRuleIsActive)
  );
  let selectedSchedule = $derived(
    config?.schedules.find((schedule) => schedule.id === selectedScheduleId) ??
      config?.schedules[0] ??
      null
  );
  let selectedScheduleIsActive = $derived(
    selectedSchedule ? scheduleIsActive(selectedSchedule) : false
  );
  let scheduleDraftIsExisting = $derived(
    Boolean(scheduleDraft && config?.schedules.some((schedule) => schedule.id === scheduleDraft?.id))
  );
  let scheduleDraftLocked = $derived(
    Boolean(scheduleDraft && scheduleDraftIsExisting && selectedScheduleIsActive)
  );
  let eventBuckets = $derived.by(() => {
    const counts: Record<string, number> = {};
    for (const event of events) {
      counts[event.kind] = (counts[event.kind] ?? 0) + 1;
    }
    return Object.entries(counts).map(([kind, count]) => ({ kind, count })).sort(
      (a, b) => b.count - a.count
    );
  });
  let maxEventCount = $derived(Math.max(1, ...eventBuckets.map((bucket) => bucket.count)));
  let daemonOnline = $derived(status?.status === "ok");
  let currentEnforcementState = $derived(
    enforcement?.enforcement_state ?? status?.enforcement_state ?? "unknown"
  );
  let enforcementActive = $derived(currentEnforcementState === "active");
  let failingChecks = $derived(
    health?.checks.filter((check) => check.state === "error" || check.state === "warn") ?? []
  );

  onMount(() => {
    void refreshAll();
  });

  function socketArg(): string | undefined {
    const trimmed = socketPath.trim();
    return trimmed.length > 0 ? trimmed : undefined;
  }

  async function refreshAll(): Promise<void> {
    loading = true;
    lastError = null;

    const [statusResult, enforcementResult, configResult, eventsResult, healthResult] =
      await Promise.allSettled([
      daemonStatus(socketArg()),
      enforcementStatus(socketArg()),
      configSnapshot(socketArg()),
      recentEvents(80, socketArg()),
      systemHealth(socketArg())
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

    lastRefresh = new Date().toLocaleTimeString();
    loading = false;
  }

  function syncConfigSelection(snapshot: ConfigSnapshot): void {
    const selectedRuleStillExists = snapshot.rules.some((rule) => rule.id === selectedRuleId);
    if (!selectedRuleStillExists) {
      selectedRuleId = snapshot.rules[0]?.id ?? null;
      ruleDraft = snapshot.rules[0] ? cloneRule(snapshot.rules[0]) : null;
    } else if (!ruleDraft && selectedRule) {
      ruleDraft = cloneRule(selectedRule);
    }

    const selectedAppRuleStillExists = snapshot.app_rules.some(
      (rule) => rule.id === selectedAppRuleId
    );
    if (!selectedAppRuleStillExists) {
      selectedAppRuleId = snapshot.app_rules[0]?.id ?? null;
      appRuleDraft = snapshot.app_rules[0] ? cloneAppRule(snapshot.app_rules[0]) : null;
    } else if (!appRuleDraft && selectedAppRule) {
      appRuleDraft = cloneAppRule(selectedAppRule);
    }

    const selectedScheduleStillExists = snapshot.schedules.some(
      (schedule) => schedule.id === selectedScheduleId
    );
    if (!selectedScheduleStillExists) {
      selectedScheduleId = snapshot.schedules[0]?.id ?? null;
      scheduleDraft = snapshot.schedules[0] ? cloneSchedule(snapshot.schedules[0]) : null;
    } else if (!scheduleDraft && selectedSchedule) {
      scheduleDraft = cloneSchedule(selectedSchedule);
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
      unlockResult = await requestUnlock(unlockTarget, unlockMinutes, unlockReason, socketArg());
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

  function selectRule(rule: Rule): void {
    selectedRuleId = rule.id;
    ruleDraft = cloneRule(rule);
    ruleMessage = null;
  }

  function startNewRule(): void {
    const index = (config?.rules.length ?? 0) + 1;
    selectedRuleId = null;
    ruleDraft = {
      id: `site-list-${index}`,
      name: `Site list ${index}`,
      tier: "controlled_access",
      enabled: true,
      patterns: [{ kind: "domain", value: "example.com", match_subdomains: true }],
      schedule_ids: [],
      allowance_id: null,
      unlock_policy: null
    };
    ruleMessage = null;
  }

  async function saveRuleDraft(): Promise<void> {
    if (!ruleDraft) return;
    ruleSaving = true;
    lastError = null;
    ruleMessage = null;
    try {
      const response = await upsertSiteList(normalizeRuleDraft(ruleDraft), socketArg());
      config = response.config;
      selectedRuleId = ruleDraft.id.trim();
      ruleDraft = cloneRule(
        response.config.rules.find((rule) => rule.id === selectedRuleId) ?? response.config.rules[0]
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
    if (!ruleDraft || !ruleDraftIsExisting) return;
    ruleSaving = true;
    lastError = null;
    ruleMessage = null;
    try {
      const response = await deleteSiteList(ruleDraft.id, socketArg());
      config = response.config;
      selectedRuleId = response.config.rules[0]?.id ?? null;
      ruleDraft = response.config.rules[0] ? cloneRule(response.config.rules[0]) : null;
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
    const index = (config?.app_rules.length ?? 0) + 1;
    selectedAppRuleId = null;
    appRuleDraft = {
      id: `app-rule-${index}`,
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
    if (!appRuleDraft || !appRuleDraftIsExisting) return;
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
    const index = (config?.schedules.length ?? 0) + 1;
    selectedScheduleId = null;
    scheduleDraft = {
      id: `schedule-${index}`,
      name: `Schedule ${index}`,
      windows: [{ weekday: "mon", start: "09:00", end: "17:00" }]
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
    if (!scheduleDraft || !scheduleDraftIsExisting) return;
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

  function cloneRule(rule: Rule): Rule {
    return {
      ...rule,
      patterns: rule.patterns.map((pattern) => ({ ...pattern })),
      schedule_ids: [...rule.schedule_ids],
      allowance_id: rule.allowance_id ?? null,
      unlock_policy: rule.unlock_policy ? { ...rule.unlock_policy } : null
    };
  }

  function cloneAppRule(rule: AppRule): AppRule {
    return {
      ...rule,
      matchers: rule.matchers.map((matcher) => ({ ...matcher })),
      schedule_ids: [...rule.schedule_ids],
      allowance_id: rule.allowance_id ?? null,
      unlock_policy: rule.unlock_policy ? { ...rule.unlock_policy } : null
    };
  }

  function cloneSchedule(schedule: Schedule): Schedule {
    return {
      ...schedule,
      name: schedule.name ?? "",
      windows: schedule.windows.map((window) => ({ ...window }))
    };
  }

  function normalizeRuleDraft(rule: Rule): Rule {
    return {
      ...rule,
      id: rule.id.trim(),
      name: rule.name.trim(),
      allowance_id: null,
      unlock_policy: rule.tier === "controlled_access" ? rule.unlock_policy : null,
      patterns: rule.patterns.map((pattern) => ({
        ...pattern,
        value: pattern.value.trim(),
        match_subdomains: pattern.kind === "domain" ? pattern.match_subdomains : false
      })),
      schedule_ids: [...rule.schedule_ids]
    };
  }

  function normalizeAppRuleDraft(rule: AppRule): AppRule {
    return {
      ...rule,
      id: rule.id.trim(),
      name: rule.name.trim(),
      allowance_id:
        rule.tier === "controlled_access" && rule.allowance_id ? rule.allowance_id : null,
      unlock_policy: rule.tier === "controlled_access" ? rule.unlock_policy : null,
      matchers: rule.matchers.map((matcher) => ({
        ...matcher,
        value: matcher.value.trim()
      })),
      schedule_ids: [...rule.schedule_ids]
    };
  }

  function normalizeScheduleDraft(schedule: Schedule): Schedule {
    return {
      ...schedule,
      id: schedule.id.trim(),
      name: schedule.name?.trim() || null,
      windows: schedule.windows.map((window) => ({ ...window }))
    };
  }

  function addPattern(): void {
    if (!ruleDraft) return;
    ruleDraft.patterns = [
      ...ruleDraft.patterns,
      { kind: "domain", value: "", match_subdomains: true }
    ];
  }

  function removePattern(index: number): void {
    if (!ruleDraft || ruleDraft.patterns.length <= 1) return;
    ruleDraft.patterns = ruleDraft.patterns.filter((_, patternIndex) => patternIndex !== index);
  }

  function addAppMatcher(): void {
    if (!appRuleDraft) return;
    appRuleDraft.matchers = [...appRuleDraft.matchers, { kind: "command_name", value: "" }];
  }

  function removeAppMatcher(index: number): void {
    if (!appRuleDraft || appRuleDraft.matchers.length <= 1) return;
    appRuleDraft.matchers = appRuleDraft.matchers.filter(
      (_, matcherIndex) => matcherIndex !== index
    );
  }

  function setRuleTier(tier: Rule["tier"]): void {
    if (!ruleDraft) return;
    ruleDraft.tier = tier;
    if (tier === "hard") {
      ruleDraft.allowance_id = null;
      ruleDraft.unlock_policy = null;
    }
  }

  function setRuleAllowance(value: string): void {
    if (!ruleDraft) return;
    ruleDraft.allowance_id = value || null;
  }

  function setAppRuleTier(tier: AppRule["tier"]): void {
    if (!appRuleDraft) return;
    appRuleDraft.tier = tier;
    if (tier === "hard") {
      appRuleDraft.allowance_id = null;
      appRuleDraft.unlock_policy = null;
    }
  }

  function toggleDraftSchedule(scheduleId: string): void {
    if (!ruleDraft) return;
    if (ruleDraft.schedule_ids.includes(scheduleId)) {
      ruleDraft.schedule_ids = ruleDraft.schedule_ids.filter((id) => id !== scheduleId);
    } else {
      ruleDraft.schedule_ids = [...ruleDraft.schedule_ids, scheduleId];
    }
  }

  function toggleAppRuleSchedule(scheduleId: string): void {
    if (!appRuleDraft) return;
    if (appRuleDraft.schedule_ids.includes(scheduleId)) {
      appRuleDraft.schedule_ids = appRuleDraft.schedule_ids.filter((id) => id !== scheduleId);
    } else {
      appRuleDraft.schedule_ids = [...appRuleDraft.schedule_ids, scheduleId];
    }
  }

  function addScheduleWindow(): void {
    if (!scheduleDraft) return;
    scheduleDraft.windows = [
      ...scheduleDraft.windows,
      { weekday: "mon", start: "09:00", end: "17:00" }
    ];
  }

  function removeScheduleWindow(index: number): void {
    if (!scheduleDraft) return;
    scheduleDraft.windows = scheduleDraft.windows.filter((_, windowIndex) => windowIndex !== index);
  }

  function ruleIsActive(rule: Rule): boolean {
    if (!rule.enabled) return false;
    if (rule.schedule_ids.length === 0) return true;

    return rule.schedule_ids.some((scheduleId) => {
      const schedule = config?.schedules.find((candidate) => candidate.id === scheduleId);
      return schedule ? scheduleIsActive(schedule) : true;
    });
  }

  function appRuleIsActive(rule: AppRule): boolean {
    if (!rule.enabled) return false;
    if (rule.schedule_ids.length === 0) return true;

    return rule.schedule_ids.some((scheduleId) => {
      const schedule = config?.schedules.find((candidate) => candidate.id === scheduleId);
      return schedule ? scheduleIsActive(schedule) : true;
    });
  }

  function scheduleIsActive(schedule: Schedule): boolean {
    return schedule.windows.some((window) => windowIsActive(window));
  }

  function windowIsActive(window: ScheduleWindow): boolean {
    const now = new Date();
    const today = weekdays[(now.getDay() + 6) % 7].id;
    const yesterday = weekdays[(now.getDay() + 5) % 7].id;
    const currentMinute = now.getHours() * 60 + now.getMinutes();
    const start = minutesAfterMidnight(window.start);
    const end = minutesAfterMidnight(window.end);

    if (start < end) {
      return window.weekday === today && currentMinute >= start && currentMinute < end;
    }

    return (
      (window.weekday === today && currentMinute >= start) ||
      (window.weekday === yesterday && currentMinute < end)
    );
  }

  function minutesAfterMidnight(value: string): number {
    const [hours, minutes] = value.split(":").map(Number);
    return hours * 60 + minutes;
  }

  function formatError(error: unknown): string {
    if (error instanceof Error) {
      return error.message;
    }
    return String(error);
  }

  function checkIcon(check: HealthCheck): Icon {
    if (check.state === "ok") return CheckCircle2;
    if (check.state === "error") return XCircle;
    if (check.state === "warn") return AlertTriangle;
    return Activity;
  }

  function eventPercent(count: number): number {
    return Math.max(4, Math.round((count / maxEventCount) * 100));
  }

  function windowsFor(schedule: Schedule, weekday: string): string {
    const windows = schedule.windows.filter((window) => window.weekday === weekday);
    return windows.map((window) => `${window.start}-${window.end}`).join(", ");
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
        <h1>{navItems.find((item) => item.id === activeView)?.label}</h1>
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
      <section class="dashboard-grid">
        <article class="panel metric-panel">
          <div class="panel-title">
            <Server size={18} aria-hidden="true" />
            <h2>Daemon</h2>
          </div>
          <div class="metric-line">
            <span class="metric-value">{status?.rules ?? "?"}</span>
            <span>site lists</span>
          </div>
          <div class="metric-line">
            <span class="metric-value">{status?.app_rules ?? "?"}</span>
            <span>app rules</span>
          </div>
          <div class="metric-line">
            <span class="metric-value">{status?.schedules ?? "?"}</span>
            <span>schedules</span>
          </div>
          <div class="metric-line">
            <span class="metric-value">{status?.allowances ?? "?"}</span>
            <span>allowances</span>
          </div>
        </article>

        <article class="panel metric-panel">
          <div class="panel-title">
            <Shield size={18} aria-hidden="true" />
            <h2>Block Tiers</h2>
          </div>
          <div class="metric-line">
            <span class="metric-value danger">{hardBlockCount}</span>
            <span>hard blocks</span>
          </div>
          <div class="metric-line">
            <span class="metric-value accent">{controlledBlockCount}</span>
            <span>controlled</span>
          </div>
        </article>

        <article class="panel metric-panel">
          <div class="panel-title">
            <Wrench size={18} aria-hidden="true" />
            <h2>System</h2>
          </div>
          <div class="metric-line">
            <span class="metric-value">{health?.checks.length ?? "?"}</span>
            <span>checks</span>
          </div>
          <div class="metric-line">
            <span class="metric-value warn">{failingChecks.length}</span>
            <span>warnings</span>
          </div>
        </article>
      </section>

      <section class="content-grid">
        <article class="panel">
          <div class="panel-title">
            <Search size={18} aria-hidden="true" />
            <h2>URL Probe</h2>
          </div>
          <div class="input-row">
            <input bind:value={testUrl} placeholder="https://example.com/" />
            <button class="primary" onclick={runUrlCheck} disabled={urlChecking}>
              <Play size={17} aria-hidden="true" />
              <span>Check</span>
            </button>
          </div>
          {#if urlDecision}
            <div class:blocked={urlDecision.decision === "block"} class="decision-row">
              {#if urlDecision.decision === "block"}
                <XCircle size={18} aria-hidden="true" />
                <span>{urlDecision.reason?.kind ?? "blocked"}</span>
              {:else}
                <CheckCircle2 size={18} aria-hidden="true" />
                <span>allowed</span>
              {/if}
            </div>
          {/if}
        </article>

        <article class="panel">
          <div class="panel-title">
            <Unlock size={18} aria-hidden="true" />
            <h2>Manual Unlock</h2>
          </div>
          <div class="unlock-grid">
            <label>
              <span>Target</span>
              <input bind:value={unlockTarget} />
            </label>
            <label>
              <span>Minutes</span>
              <input type="number" min="1" max="240" bind:value={unlockMinutes} />
            </label>
            <label class="reason-field">
              <span>Reason</span>
              <input bind:value={unlockReason} />
            </label>
            <button class="primary" onclick={runUnlock} disabled={unlocking}>
              <Unlock size={17} aria-hidden="true" />
              <span>Unlock</span>
            </button>
          </div>
          {#if unlockResult}
            <p class="result-text">
              Active until {new Date(unlockResult.expires_at).toLocaleTimeString()} for
              {unlockResult.rule_id}
            </p>
          {/if}
        </article>
      </section>
    {:else if activeView === "blocks"}
      <section class="split-view">
        <article class="panel list-panel">
          <div class="panel-title">
            <ListChecks size={18} aria-hidden="true" />
            <h2>Site Lists</h2>
          </div>
          <button class="secondary wide-button" onclick={startNewRule}>
            <Plus size={17} aria-hidden="true" />
            <span>New list</span>
          </button>
          <div class="rule-list">
            {#each config?.rules ?? [] as rule (rule.id)}
              <button
                class:active={ruleDraft?.id === rule.id}
                onclick={() => selectRule(rule)}
              >
                <span class:hard={rule.tier === "hard"} class="tier-dot"></span>
                <span>{rule.name}</span>
                <em>{rule.tier === "hard" ? "Tier 1" : "Tier 2"}</em>
              </button>
            {:else}
              <p class="empty-state">No lists reported by the daemon.</p>
            {/each}
          </div>
        </article>

        <article class="panel detail-panel">
          <div class="panel-title">
            <Shield size={18} aria-hidden="true" />
            <h2>{ruleDraft?.name || "Site list"}</h2>
          </div>
          {#if ruleDraft}
            {#if ruleDraftLocked}
              <section class="inline-warning">
                <AlertTriangle size={17} aria-hidden="true" />
                <span>This list is active right now.</span>
              </section>
            {/if}
            <div class="form-grid">
              <label>
                <span>List ID</span>
                <input bind:value={ruleDraft.id} readonly={ruleDraftIsExisting} disabled={ruleDraftLocked} />
              </label>
              <label>
                <span>Tier</span>
                <select
                  value={ruleDraft.tier}
                  disabled={ruleDraftLocked}
                  onchange={(event) =>
                    setRuleTier(event.currentTarget.value as Rule["tier"])}
                >
                  <option value="controlled_access">Tier 2</option>
                  <option value="hard">Tier 1</option>
                </select>
              </label>
              <label>
                <span>Name</span>
                <input bind:value={ruleDraft.name} disabled={ruleDraftLocked} />
              </label>
              <label>
                <span>Allowance</span>
                <select
                  value={ruleDraft.allowance_id ?? ""}
                  disabled={ruleDraftLocked || ruleDraft.tier === "hard"}
                  onchange={(event) => setRuleAllowance(event.currentTarget.value)}
                >
                  <option value="">None</option>
                  {#each config?.allowances ?? [] as allowance (allowance.id)}
                    <option value={allowance.id}>{allowance.name ?? allowance.id}</option>
                  {/each}
                </select>
              </label>
            </div>

            <label class="check-row">
              <input type="checkbox" bind:checked={ruleDraft.enabled} disabled={ruleDraftLocked} />
              <span>Enabled</span>
            </label>

            <div class="section-label">Schedules</div>
            <div class="chip-grid">
              {#each config?.schedules ?? [] as schedule (schedule.id)}
                <label class="chip-check">
                  <input
                    type="checkbox"
                    checked={ruleDraft.schedule_ids.includes(schedule.id)}
                    disabled={ruleDraftLocked}
                    onchange={() => toggleDraftSchedule(schedule.id)}
                  />
                  <span>{schedule.name ?? schedule.id}</span>
                </label>
              {:else}
                <p class="empty-state">No schedules available.</p>
              {/each}
            </div>

            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Type</th>
                    <th>Pattern</th>
                    <th>Subdomains</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each ruleDraft.patterns as pattern, index (pattern)}
                    <tr>
                      <td>
                        <select bind:value={pattern.kind} disabled={ruleDraftLocked}>
                          {#each patternKinds as kind (kind.id)}
                            <option value={kind.id}>{kind.label}</option>
                          {/each}
                        </select>
                      </td>
                      <td>
                        <input bind:value={pattern.value} disabled={ruleDraftLocked} />
                      </td>
                      <td>
                        <input
                          type="checkbox"
                          bind:checked={pattern.match_subdomains}
                          disabled={ruleDraftLocked || pattern.kind !== "domain"}
                        />
                      </td>
                      <td>
                        <button
                          class="icon-button"
                          title="Remove pattern"
                          onclick={() => removePattern(index)}
                          disabled={ruleDraftLocked || ruleDraft.patterns.length <= 1}
                        >
                          <Trash2 size={16} aria-hidden="true" />
                        </button>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>

            <div class="button-row">
              <button class="secondary" onclick={addPattern} disabled={ruleDraftLocked}>
                <Plus size={17} aria-hidden="true" />
                <span>Pattern</span>
              </button>
              <button
                class="secondary"
                onclick={removeRuleDraft}
                disabled={ruleSaving || ruleDraftLocked || !ruleDraftIsExisting}
              >
                <Trash2 size={17} aria-hidden="true" />
                <span>Delete</span>
              </button>
              <button
                class="primary"
                onclick={saveRuleDraft}
                disabled={ruleSaving || ruleDraftLocked}
              >
                <Save size={17} aria-hidden="true" />
                <span>Save</span>
              </button>
            </div>
            {#if ruleMessage}
              <p class="result-text">{ruleMessage}</p>
            {/if}
          {/if}
        </article>
      </section>
    {:else if activeView === "apps"}
      <section class="split-view">
        <article class="panel list-panel">
          <div class="panel-title">
            <Gamepad2 size={18} aria-hidden="true" />
            <h2>App Rules</h2>
          </div>
          <button class="secondary wide-button" onclick={startNewAppRule}>
            <Plus size={17} aria-hidden="true" />
            <span>New app</span>
          </button>
          <div class="rule-list">
            {#each config?.app_rules ?? [] as rule (rule.id)}
              <button
                class:active={appRuleDraft?.id === rule.id}
                onclick={() => selectAppRule(rule)}
              >
                <span class:hard={rule.tier === "hard"} class="tier-dot"></span>
                <span>{rule.name}</span>
                <em>{rule.tier === "hard" ? "Tier 1" : "Tier 2"}</em>
              </button>
            {:else}
              <p class="empty-state">No app rules reported by the daemon.</p>
            {/each}
          </div>
        </article>

        <article class="panel detail-panel">
          <div class="panel-title">
            <Gamepad2 size={18} aria-hidden="true" />
            <h2>{appRuleDraft?.name || "App rule"}</h2>
          </div>
          {#if appRuleDraft}
            {#if appRuleDraftLocked}
              <section class="inline-warning">
                <AlertTriangle size={17} aria-hidden="true" />
                <span>This app rule is active right now.</span>
              </section>
            {/if}
            <div class="form-grid">
              <label>
                <span>Rule ID</span>
                <input
                  bind:value={appRuleDraft.id}
                  readonly={appRuleDraftIsExisting}
                  disabled={appRuleDraftLocked}
                />
              </label>
              <label>
                <span>Tier</span>
                <select
                  value={appRuleDraft.tier}
                  disabled={appRuleDraftLocked}
                  onchange={(event) =>
                    setAppRuleTier(event.currentTarget.value as AppRule["tier"])}
                >
                  <option value="hard">Tier 1</option>
                  <option value="controlled_access">Tier 2</option>
                </select>
              </label>
              <label>
                <span>Name</span>
                <input bind:value={appRuleDraft.name} disabled={appRuleDraftLocked} />
              </label>
            </div>

            <label class="check-row">
              <input type="checkbox" bind:checked={appRuleDraft.enabled} disabled={appRuleDraftLocked} />
              <span>Enabled</span>
            </label>

            <div class="section-label">Schedules</div>
            <div class="chip-grid">
              {#each config?.schedules ?? [] as schedule (schedule.id)}
                <label class="chip-check">
                  <input
                    type="checkbox"
                    checked={appRuleDraft.schedule_ids.includes(schedule.id)}
                    disabled={appRuleDraftLocked}
                    onchange={() => toggleAppRuleSchedule(schedule.id)}
                  />
                  <span>{schedule.name ?? schedule.id}</span>
                </label>
              {:else}
                <p class="empty-state">No schedules available.</p>
              {/each}
            </div>

            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Matcher</th>
                    <th>Value</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each appRuleDraft.matchers as matcher, index (matcher)}
                    <tr>
                      <td>
                        <select bind:value={matcher.kind} disabled={appRuleDraftLocked}>
                          {#each appMatcherKinds as kind (kind.id)}
                            <option value={kind.id}>{kind.label}</option>
                          {/each}
                        </select>
                      </td>
                      <td>
                        <input bind:value={matcher.value} disabled={appRuleDraftLocked} />
                      </td>
                      <td>
                        <button
                          class="icon-button"
                          title="Remove matcher"
                          onclick={() => removeAppMatcher(index)}
                          disabled={appRuleDraftLocked || appRuleDraft.matchers.length <= 1}
                        >
                          <Trash2 size={16} aria-hidden="true" />
                        </button>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>

            <div class="button-row">
              <button class="secondary" onclick={addAppMatcher} disabled={appRuleDraftLocked}>
                <Plus size={17} aria-hidden="true" />
                <span>Matcher</span>
              </button>
              <button
                class="secondary"
                onclick={removeAppRuleDraft}
                disabled={appRuleSaving || appRuleDraftLocked || !appRuleDraftIsExisting}
              >
                <Trash2 size={17} aria-hidden="true" />
                <span>Delete</span>
              </button>
              <button
                class="primary"
                onclick={saveAppRuleDraft}
                disabled={appRuleSaving || appRuleDraftLocked}
              >
                <Save size={17} aria-hidden="true" />
                <span>Save</span>
              </button>
            </div>
            {#if appRuleMessage}
              <p class="result-text">{appRuleMessage}</p>
            {/if}
          {/if}
        </article>
      </section>
    {:else if activeView === "schedule"}
      <section class="split-view">
        <article class="panel list-panel">
          <div class="panel-title">
            <CalendarDays size={18} aria-hidden="true" />
            <h2>Schedules</h2>
          </div>
          <button class="secondary wide-button" onclick={startNewSchedule}>
            <Plus size={17} aria-hidden="true" />
            <span>New schedule</span>
          </button>
          <div class="rule-list">
            {#each config?.schedules ?? [] as schedule (schedule.id)}
              <button
                class:active={scheduleDraft?.id === schedule.id}
                onclick={() => selectSchedule(schedule)}
              >
                <span class:hard={scheduleIsActive(schedule)} class="tier-dot"></span>
                <span>{schedule.name ?? schedule.id}</span>
                <em>{scheduleIsActive(schedule) ? "Active" : "Idle"}</em>
              </button>
            {:else}
              <p class="empty-state">No schedules reported by the daemon.</p>
            {/each}
          </div>
        </article>

        <article class="panel detail-panel">
          <div class="panel-title">
            <CalendarDays size={18} aria-hidden="true" />
            <h2>{scheduleDraft?.name || "Schedule"}</h2>
          </div>
          {#if scheduleDraft}
            {#if scheduleDraftLocked}
              <section class="inline-warning">
                <AlertTriangle size={17} aria-hidden="true" />
                <span>This schedule is active right now.</span>
              </section>
            {/if}
            <div class="form-grid">
              <label>
                <span>Schedule ID</span>
                <input
                  bind:value={scheduleDraft.id}
                  readonly={scheduleDraftIsExisting}
                  disabled={scheduleDraftLocked}
                />
              </label>
              <label>
                <span>Name</span>
                <input bind:value={scheduleDraft.name} disabled={scheduleDraftLocked} />
              </label>
            </div>

            <div class="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Weekday</th>
                    <th>Start</th>
                    <th>End</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {#each scheduleDraft.windows as window, index (window)}
                    <tr>
                      <td>
                        <select bind:value={window.weekday} disabled={scheduleDraftLocked}>
                          {#each weekdays as day (day.id)}
                            <option value={day.id}>{day.label}</option>
                          {/each}
                        </select>
                      </td>
                      <td>
                        <input type="time" bind:value={window.start} disabled={scheduleDraftLocked} />
                      </td>
                      <td>
                        <input type="time" bind:value={window.end} disabled={scheduleDraftLocked} />
                      </td>
                      <td>
                        <button
                          class="icon-button"
                          title="Remove window"
                          onclick={() => removeScheduleWindow(index)}
                          disabled={scheduleDraftLocked}
                        >
                          <Trash2 size={16} aria-hidden="true" />
                        </button>
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>

            <div class="button-row">
              <button class="secondary" onclick={addScheduleWindow} disabled={scheduleDraftLocked}>
                <Plus size={17} aria-hidden="true" />
                <span>Window</span>
              </button>
              <button
                class="secondary"
                onclick={removeScheduleDraft}
                disabled={scheduleSaving || scheduleDraftLocked || !scheduleDraftIsExisting}
              >
                <Trash2 size={17} aria-hidden="true" />
                <span>Delete</span>
              </button>
              <button
                class="primary"
                onclick={saveScheduleDraft}
                disabled={scheduleSaving || scheduleDraftLocked}
              >
                <Save size={17} aria-hidden="true" />
                <span>Save</span>
              </button>
            </div>
            {#if scheduleMessage}
              <p class="result-text">{scheduleMessage}</p>
            {/if}
          {/if}
        </article>

        <article class="panel wide-panel">
          <div class="panel-title">
            <CalendarDays size={18} aria-hidden="true" />
            <h2>Weekly Grid</h2>
          </div>
          <div class="schedule-grid">
            <div class="schedule-head">Schedule</div>
            {#each weekdays as day}
              <div class="schedule-head">{day.label}</div>
            {/each}
            {#each config?.schedules ?? [] as schedule (schedule.id)}
              <div class="schedule-name">{schedule.name ?? schedule.id}</div>
              {#each weekdays as day}
                <div class:filled={windowsFor(schedule, day.id)} class="schedule-cell">
                  {windowsFor(schedule, day.id) || "-"}
                </div>
              {/each}
            {:else}
              <p class="empty-state">No schedules reported by the daemon.</p>
            {/each}
          </div>
        </article>
      </section>
    {:else if activeView === "allowances"}
      <section class="content-grid">
        {#each config?.allowances ?? [] as allowance (allowance.id)}
          <article class="panel allowance-panel">
            <div class="panel-title">
              <Clock3 size={18} aria-hidden="true" />
              <h2>{allowance.name ?? allowance.id}</h2>
            </div>
            <div class="allowance-value">{allowance.daily_minutes} min</div>
            <p class="muted">Daily allowance</p>
          </article>
        {:else}
          <article class="panel">
            <p class="empty-state">No allowances reported by the daemon.</p>
          </article>
        {/each}
      </section>
    {:else if activeView === "statistics"}
      <section class="content-grid">
        <article class="panel">
          <div class="panel-title">
            <BarChart3 size={18} aria-hidden="true" />
            <h2>Event Mix</h2>
          </div>
          <div class="bar-list">
            {#each eventBuckets as bucket (bucket.kind)}
              <div class="bar-row">
                <span>{bucket.kind}</span>
                <div class="bar-track">
                  <div class="bar-fill" style={`width: ${eventPercent(bucket.count)}%`}></div>
                </div>
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
                <span>{event.kind}</span>
                <strong>{event.target ?? "system"}</strong>
                <time>{new Date(event.created_at).toLocaleTimeString()}</time>
              </div>
            {:else}
              <p class="empty-state">No events recorded yet.</p>
            {/each}
          </div>
        </article>
      </section>
    {:else if activeView === "admin"}
      <section class="content-grid admin-grid">
        <article class="panel">
          <div class="panel-title">
            <Gauge size={18} aria-hidden="true" />
            <h2>Health</h2>
          </div>
          <div class="health-list">
            {#each health?.checks ?? [] as check (check.key)}
              {@const HealthIcon = checkIcon(check)}
              <div class="health-row" data-state={check.state}>
                <HealthIcon size={18} aria-hidden="true" />
                <span>{check.label}</span>
                <strong>{check.state}</strong>
                <small>{check.detail}</small>
              </div>
            {:else}
              <p class="empty-state">No health checks available.</p>
            {/each}
          </div>
        </article>

        <article class="panel">
          <div class="panel-title">
            <Shield size={18} aria-hidden="true" />
            <h2>Enforcement</h2>
          </div>
          <div class="status-list">
            <div class="status-row">
              <span>Mode</span>
              <strong data-state={currentEnforcementState}>{currentEnforcementState}</strong>
            </div>
            <div class="status-row">
              <span>Firefox policy</span>
              <small>{enforcement?.firefox_policy.path ?? "unknown"}</small>
            </div>
            <div class="status-row">
              <span>Hosts file</span>
              <small>{enforcement?.hosts_file.path ?? "unknown"}</small>
            </div>
          </div>
          <div class="button-row enforcement-actions">
            <button
              class="primary"
              onclick={runStartEnforcement}
              disabled={enforcementChanging || enforcementActive}
            >
              <Power size={17} aria-hidden="true" />
              <span>Start</span>
            </button>
            <button
              class="secondary danger-action"
              onclick={runStopEnforcement}
              disabled={enforcementChanging || !enforcementActive}
            >
              <PowerOff size={17} aria-hidden="true" />
              <span>Stop</span>
            </button>
          </div>
          {#if enforcementMessage}
            <p class="result-text">{enforcementMessage}</p>
          {/if}
        </article>

        <article class="panel">
          <div class="panel-title">
            <Terminal size={18} aria-hidden="true" />
            <h2>JSON-RPC</h2>
          </div>
          <div class="rpc-form">
            <label>
              <span>Method</span>
              <input bind:value={rawMethod} />
            </label>
            <label>
              <span>Params</span>
              <textarea bind:value={rawParams} spellcheck="false"></textarea>
            </label>
            <button class="primary" onclick={runRawRpc} disabled={rawRunning}>
              <Play size={17} aria-hidden="true" />
              <span>Run</span>
            </button>
          </div>
          {#if rawResult}
            <pre>{rawResult}</pre>
          {/if}
        </article>
      </section>
    {/if}

    <footer class="footer-line">
      <span>{lastRefresh ? `Last refresh ${lastRefresh}` : "Not refreshed"}</span>
      <span>{health?.socket_path ?? socketPath}</span>
    </footer>
  </main>
</div>

<style>
  .app-shell {
    display: grid;
    grid-template-columns: 236px minmax(0, 1fr);
    min-height: 100vh;
    background: #eef2ee;
  }

  .sidebar {
    background: #171b1d;
    color: #f4f7f5;
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    padding: 18px 14px;
  }

  .brand {
    align-items: center;
    display: flex;
    gap: 12px;
    min-height: 54px;
    padding: 0 8px 16px;
  }

  .brand-mark {
    align-items: center;
    background: #2fb67d;
    border-radius: 8px;
    color: #07110d;
    display: grid;
    height: 40px;
    justify-content: center;
    width: 40px;
  }

  .brand strong,
  .brand span {
    display: block;
  }

  .brand span {
    color: #aab6b0;
    font-size: 0.82rem;
  }

  .nav-list {
    display: grid;
    gap: 4px;
    margin-top: 8px;
  }

  .nav-list button {
    align-items: center;
    background: transparent;
    border: 0;
    border-radius: 8px;
    color: #dce5df;
    display: flex;
    gap: 10px;
    min-height: 42px;
    padding: 0 10px;
    text-align: left;
  }

  .nav-list button.active,
  .nav-list button:hover {
    background: #27302d;
    color: #ffffff;
  }

  .daemon-strip {
    align-items: center;
    background: #20342c;
    border-radius: 8px;
    color: #c9f3df;
    display: flex;
    gap: 10px;
    margin-top: auto;
    min-height: 44px;
    padding: 0 10px;
  }

  .daemon-strip.offline {
    background: #3c2425;
    color: #ffd4d4;
  }

  .workspace {
    display: flex;
    flex-direction: column;
    min-width: 0;
    max-height: 100vh;
    overflow: auto;
    padding: 22px;
  }

  .topbar {
    align-items: center;
    display: flex;
    gap: 18px;
    justify-content: space-between;
    margin-bottom: 16px;
  }

  .eyebrow {
    color: #62706a;
    font-size: 0.78rem;
    font-weight: 700;
    letter-spacing: 0;
    margin: 0 0 2px;
    text-transform: uppercase;
  }

  h1,
  h2,
  p {
    margin: 0;
  }

  h1 {
    font-size: 2rem;
    letter-spacing: 0;
    line-height: 1.1;
  }

  h2 {
    font-size: 1rem;
    letter-spacing: 0;
  }

  .topbar-actions {
    align-items: end;
    display: flex;
    gap: 10px;
  }

  .socket-field {
    display: grid;
    gap: 4px;
    min-width: min(44vw, 420px);
  }

  label span,
  .socket-field span {
    color: #63716b;
    font-size: 0.78rem;
    font-weight: 700;
  }

  input,
  select,
  textarea {
    background: #ffffff;
    border: 1px solid #cdd7d1;
    border-radius: 8px;
    color: #1d2522;
    min-height: 40px;
    min-width: 0;
    padding: 0 10px;
    width: 100%;
  }

  input[type="checkbox"] {
    min-height: auto;
    width: 16px;
  }

  select {
    appearance: auto;
  }

  textarea {
    min-height: 124px;
    padding: 10px;
    resize: vertical;
  }

  .icon-button,
  .primary,
  .secondary {
    align-items: center;
    border: 0;
    border-radius: 8px;
    display: inline-flex;
    gap: 8px;
    justify-content: center;
    min-height: 40px;
    padding: 0 12px;
    white-space: nowrap;
  }

  .icon-button {
    background: #ffffff;
    border: 1px solid #cdd7d1;
    color: #1d2522;
    width: 42px;
  }

  .primary {
    background: #1f8f68;
    color: #ffffff;
  }

  .secondary {
    background: #dde5df;
    color: #34413b;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  .spin {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .alert-row,
  .decision-row {
    align-items: center;
    background: #fff4d8;
    border: 1px solid #e5bc5c;
    border-radius: 8px;
    color: #5b4410;
    display: flex;
    gap: 10px;
    margin-bottom: 14px;
    min-height: 42px;
    padding: 0 12px;
  }

  .decision-row {
    background: #e6f6ee;
    border-color: #8bd3ad;
    color: #145c3d;
    margin: 12px 0 0;
  }

  .decision-row.blocked {
    background: #ffe5e5;
    border-color: #ee9d9d;
    color: #8b1d1d;
  }

  .dashboard-grid,
  .content-grid {
    display: grid;
    gap: 14px;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    margin-bottom: 14px;
  }

  .content-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .admin-grid {
    align-items: start;
  }

  .panel {
    background: #ffffff;
    border: 1px solid #d5ddd8;
    border-radius: 8px;
    min-width: 0;
    padding: 16px;
  }

  .panel-title {
    align-items: center;
    color: #29332f;
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
  }

  .metric-panel {
    display: grid;
    gap: 8px;
  }

  .metric-line {
    align-items: baseline;
    display: flex;
    gap: 10px;
    justify-content: space-between;
  }

  .metric-line span:last-child,
  .muted {
    color: #64736c;
  }

  .metric-value {
    color: #1f8f68;
    font-size: 2rem;
    font-weight: 800;
    letter-spacing: 0;
  }

  .metric-value.danger {
    color: #c94f4f;
  }

  .metric-value.warn {
    color: #b87912;
  }

  .metric-value.accent {
    color: #386dc0;
  }

  .input-row,
  .button-row {
    display: flex;
    gap: 10px;
  }

  .unlock-grid,
  .form-grid,
  .rpc-form {
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .reason-field,
  .rpc-form label:nth-child(2) {
    grid-column: 1 / -1;
  }

  .result-text {
    color: #145c3d;
    margin-top: 12px;
  }

  .split-view {
    display: grid;
    gap: 14px;
    grid-template-columns: minmax(280px, 0.34fr) minmax(0, 1fr);
  }

  .wide-panel {
    grid-column: 1 / -1;
  }

  .wide-button {
    margin-bottom: 12px;
    width: 100%;
  }

  .inline-warning {
    align-items: center;
    background: #fff4d8;
    border: 1px solid #e5bc5c;
    border-radius: 8px;
    color: #5b4410;
    display: flex;
    gap: 10px;
    margin-bottom: 12px;
    min-height: 38px;
    padding: 0 10px;
  }

  .section-label {
    color: #63716b;
    font-size: 0.78rem;
    font-weight: 800;
    margin: 14px 0 8px;
    text-transform: uppercase;
  }

  .check-row,
  .chip-check {
    align-items: center;
    display: flex;
    gap: 8px;
  }

  .check-row {
    margin-top: 12px;
  }

  .chip-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .chip-check {
    background: #f5f7f5;
    border: 1px solid #dfe6e1;
    border-radius: 8px;
    min-height: 36px;
    padding: 0 10px;
  }

  .rule-list {
    display: grid;
    gap: 6px;
  }

  .rule-list button {
    align-items: center;
    background: #f5f7f5;
    border: 1px solid #dfe6e1;
    border-radius: 8px;
    color: #26312c;
    display: grid;
    gap: 8px;
    grid-template-columns: 10px minmax(0, 1fr) max-content;
    min-height: 42px;
    padding: 0 10px;
    text-align: left;
  }

  .rule-list button.active {
    border-color: #1f8f68;
    box-shadow: inset 3px 0 0 #1f8f68;
  }

  .rule-list em {
    color: #65746d;
    font-size: 0.76rem;
    font-style: normal;
  }

  .tier-dot {
    background: #386dc0;
    border-radius: 999px;
    height: 10px;
    width: 10px;
  }

  .tier-dot.hard {
    background: #c94f4f;
  }

  .table-wrap {
    margin-top: 14px;
    overflow-x: auto;
  }

  table {
    border-collapse: collapse;
    min-width: 100%;
  }

  th,
  td {
    border-bottom: 1px solid #e2e8e4;
    padding: 10px 8px;
    text-align: left;
  }

  th {
    color: #62706a;
    font-size: 0.78rem;
    text-transform: uppercase;
  }

  .schedule-grid {
    display: grid;
    gap: 1px;
    grid-template-columns: 180px repeat(7, minmax(92px, 1fr));
    overflow-x: auto;
  }

  .schedule-head,
  .schedule-name,
  .schedule-cell {
    background: #f6f8f6;
    min-height: 42px;
    padding: 10px;
  }

  .schedule-head {
    color: #62706a;
    font-size: 0.78rem;
    font-weight: 800;
    text-transform: uppercase;
  }

  .schedule-name {
    font-weight: 700;
  }

  .schedule-cell {
    color: #68766f;
    font-size: 0.9rem;
  }

  .schedule-cell.filled {
    background: #e2f3ea;
    color: #145c3d;
    font-weight: 700;
  }

  .allowance-panel {
    min-height: 148px;
  }

  .allowance-value {
    font-size: 2.4rem;
    font-weight: 800;
    letter-spacing: 0;
  }

  .bar-list,
  .event-list,
  .health-list {
    display: grid;
    gap: 8px;
  }

  .bar-row,
  .event-row,
  .health-row {
    align-items: center;
    display: grid;
    gap: 10px;
    min-height: 36px;
  }

  .bar-row {
    grid-template-columns: 140px minmax(0, 1fr) 42px;
  }

  .bar-track {
    background: #e4ebe6;
    border-radius: 999px;
    height: 10px;
    overflow: hidden;
  }

  .bar-fill {
    background: #386dc0;
    height: 100%;
  }

  .event-row {
    grid-template-columns: 150px minmax(0, 1fr) max-content;
  }

  .event-row span,
  .event-row time {
    color: #68766f;
    font-size: 0.88rem;
  }

  .status-list {
    display: grid;
    gap: 10px;
  }

  .status-row {
    align-items: center;
    border-bottom: 1px solid #e2e8e4;
    display: grid;
    gap: 10px;
    grid-template-columns: 132px minmax(0, 1fr);
    min-height: 34px;
    padding-bottom: 8px;
  }

  .status-row span {
    color: #63716b;
    font-size: 0.78rem;
    font-weight: 800;
    text-transform: uppercase;
  }

  .status-row strong {
    justify-self: start;
  }

  .status-row strong[data-state="active"] {
    color: #1f8f68;
  }

  .status-row strong[data-state="stopped"] {
    color: #b87912;
  }

  .status-row small {
    color: #68766f;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .enforcement-actions {
    margin-top: 14px;
  }

  .danger-action {
    color: #7b2727;
  }

  .health-row {
    border-bottom: 1px solid #e2e8e4;
    grid-template-columns: 22px 170px 70px minmax(0, 1fr);
    padding-bottom: 8px;
  }

  .health-row[data-state="ok"] {
    --health-color: #1f8f68;
  }

  .health-row[data-state="warn"] {
    --health-color: #b87912;
  }

  .health-row[data-state="error"] {
    --health-color: #c94f4f;
  }

  .health-row :global(svg) {
    color: var(--health-color, #64736c);
  }

  .health-row small {
    color: #68766f;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  pre {
    background: #18201d;
    border-radius: 8px;
    color: #dcf4e8;
    font-size: 0.85rem;
    margin: 14px 0 0;
    max-height: 340px;
    overflow: auto;
    padding: 12px;
  }

  .empty-state {
    color: #66756e;
    padding: 12px 0;
  }

  .footer-line {
    color: #65736d;
    display: flex;
    font-size: 0.82rem;
    gap: 18px;
    justify-content: space-between;
    margin-top: auto;
    min-height: 32px;
    padding-top: 12px;
  }

  @media (max-width: 980px) {
    .app-shell {
      grid-template-columns: 1fr;
    }

    .sidebar {
      min-height: auto;
    }

    .nav-list {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }

    .workspace {
      max-height: none;
    }

    .dashboard-grid,
    .content-grid,
    .split-view {
      grid-template-columns: 1fr;
    }

    .topbar,
    .topbar-actions {
      align-items: stretch;
      flex-direction: column;
    }

    .socket-field {
      min-width: 0;
    }
  }
</style>
