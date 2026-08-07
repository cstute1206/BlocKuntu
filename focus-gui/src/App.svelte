<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
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
    Timer,
    XCircle
  } from "@lucide/svelte";
  import AdminView from "./components/views/AdminView.svelte";
  import AppRulesView from "./components/views/AppRulesView.svelte";
  import DetoxView from "./components/views/DetoxView.svelte";
  import OverviewView from "./components/views/OverviewView.svelte";
  import SchedulesView from "./components/views/SchedulesView.svelte";
  import SiteListsView from "./components/views/SiteListsView.svelte";
  import StatisticsView from "./components/views/StatisticsView.svelte";
  import {
    installationInfo,
    cancelDetox,
    configSnapshot,
    daemonStatus,
    deleteAppRule,
    deleteSchedule,
    deleteSiteList,
    detoxSessions,
    enforcementStatus,
    evaluateUrl,
    exportPolicyToml,
    importPolicyToml,
    logSummary,
    notificationPreferences as fetchNotificationPreferences,
    runningApps as fetchRunningApps,
    requestUnlock,
    scheduleActivitySummary,
    setChromiumIncognitoChangeAccessMode,
    setChromiumIncognitoDisableScope,
    setChromiumIncognitoMode,
    setProtectedAccessMode,
    setUnsupportedBrowserBlockMode,
    setNotificationPreferences,
    startDetox,
    systemHealth,
    tier1EditStatus,
    uninstallBlockuntu,
    recoveryCredentials,
    hideRecoveryCredentials,
    unlockTier1Edit,
    upsertAllowance,
    upsertAppRule,
    upsertSchedule,
    upsertSiteList
  } from "./lib/api";
  import {
    applicationUiPreferences,
    appRuleIsActive,
    cloneAllowance,
    cloneAllowanceForRule,
    cloneAppRule,
    cloneRule,
    cloneSchedule,
    clearFirstRunOverviewDismissed,
    defaultApplicationUiPreferences,
    detectedMatchersForRunningApp,
    defaultAllowanceForRule,
    firstRunOverviewDismissed,
    formatError,
    lastSelectedView,
    markFirstRunOverviewDismissed,
    mergeAppMatchers,
    nextAvailableIndexedId,
    normalizeAllowanceDraft,
    normalizeAppRuleDraft,
    normalizeRuleDraft,
    normalizeScheduleDraft,
    ruleIsActive,
    saveApplicationUiPreferences,
    saveLastSelectedView
  } from "./lib/ui";
  import type { ApplicationUiPreferences } from "./lib/ui";
  import type {
    Allowance,
    AppRule,
    ConfigSnapshot,
    ChromiumIncognitoDisableScope,
    ChromiumIncognitoMode,
    DaemonStatus,
    DecisionResult,
    DetoxDurationUnit,
    DetoxSession,
    EnforcementStatus,
    LogSummary,
    NotificationPreferences,
    PolicyFileResult,
    ProtectedAccessMode,
    RunningApp,
    ScheduleActivitySummary,
    Rule,
    Schedule,
    SystemHealth,
    Tier1EditStatus,
    UninstallResult,
    UnlockResult,
    ViewId
  } from "./lib/types";

  type Icon = typeof LayoutDashboard;
  const TRAY_OPEN_VIEW_EVENT = "blockuntu-open-view";
  const TRAY_RUNTIME_REFRESH_EVENT = "blockuntu-runtime-refresh";
  interface RefreshOptions {
    silent?: boolean;
  }

  const navItems: Array<{ id: ViewId; label: string; icon: Icon }> = [
    { id: "overview", label: "Overview", icon: LayoutDashboard },
    { id: "blocks", label: "Websites", icon: ListChecks },
    { id: "apps", label: "Applications", icon: Gamepad2 },
    { id: "detox", label: "Detox", icon: Timer },
    { id: "schedule", label: "Schedule", icon: CalendarDays },
    { id: "statistics", label: "Statistics", icon: BarChart3 }
  ];

  let activeView: ViewId = $state("overview");
  let status = $state<DaemonStatus | null>(null);
  let enforcement = $state<EnforcementStatus | null>(null);
  let health = $state<SystemHealth | null>(null);
  let config = $state<ConfigSnapshot | null>(null);
  let detoxSessionList = $state<DetoxSession[]>([]);
  let logStatistics = $state<LogSummary | null>(null);
  let scheduleActivityStatistics = $state<ScheduleActivitySummary | null>(null);
  let runningApps = $state<RunningApp[]>([]);
  let runningAppsLoading = $state(false);
  let runningAppsError: string | null = $state(null);
  let runningAppsLoadedOnce = $state(false);
  let loading = $state(false);
  let lastError: string | null = $state(null);
  let lastRefresh: string | null = $state(null);
  let showFirstRunOverview = $state(false);
  let settingsOpen = $state(false);
  let refreshInFlight = false;
  let runtimeRefreshTimerId: number | null = null;
  let uiPreferences = $state<ApplicationUiPreferences>({ ...defaultApplicationUiPreferences });
  let notificationPreferenceState = $state<NotificationPreferences | null>(null);
  let notificationPreferencesSaving = $state(false);
  let notificationPreferencesError: string | null = $state(null);

  let testUrl = $state("https://youtube.com/");
  let urlDecision = $state<DecisionResult | null>(null);
  let urlChecking = $state(false);

  let unlockTarget = $state("https://youtube.com/");
  let unlockReason = $state("");
  let unlockResult = $state<UnlockResult | null>(null);
  let unlocking = $state(false);

  let selectedRuleId = $state<string | null>(null);
  let ruleDraft = $state<Rule | null>(null);
  let ruleAllowanceDraft = $state<Allowance | null>(null);
  let ruleSaving = $state(false);
  let ruleMessage: string | null = $state(null);

  let selectedAppRuleId = $state<string | null>(null);
  let appRuleDraft = $state<AppRule | null>(null);
  let appRuleAllowanceDraft = $state<Allowance | null>(null);
  let appRuleSaving = $state(false);
  let appRuleMessage: string | null = $state(null);

  let selectedScheduleId = $state<string | null>(null);
  let scheduleDraft = $state<Schedule | null>(null);
  let scheduleSiteRuleIds = $state<string[]>([]);
  let scheduleAppRuleIds = $state<string[]>([]);
  let scheduleSaving = $state(false);
  let scheduleMessage: string | null = $state(null);

  let detoxName = $state("Deep work");
  let detoxDurationValue: number | undefined = $state(1);
  let detoxDurationUnit: DetoxDurationUnit = $state("hours");
  let selectedDetoxSiteRuleIds = $state<string[]>([]);
  let selectedDetoxAppRuleIds = $state<string[]>([]);
  let detoxStarting = $state(false);
  let detoxCancellingId: string | null = $state(null);
  let detoxMessage: string | null = $state(null);

  let installationSerial: string | null = $state(null);
  let buildNumber: string | null = $state(null);
  let uninstallPhraseLoading = $state(false);
  let recoveryUninstallPhrase: string | null = $state(null);
  let recoveryTier1Key: string | null = $state(null);
  let uninstallPhraseError: string | null = $state(null);
  let uninstallPhraseInput = $state("");
  let uninstallRunning = $state(false);
  let uninstallResult: UninstallResult | null = $state(null);

  let tier1EditPhraseInput = $state("");
  let tier1EditCredentialConfigured = $state(false);
  let protectedAccessMode = $state<ProtectedAccessMode>("all_time");
  let unsupportedBrowserBlockMode = $state<ProtectedAccessMode>("all_time");
  let unsupportedBrowserBlockActive = $state(false);
  let chromiumIncognitoMode = $state<ChromiumIncognitoMode>("manual_consent");
  let chromiumIncognitoDisableScope = $state<ChromiumIncognitoDisableScope>("all_time");
  let chromiumIncognitoPrivateBrowsingDisabled = $state(false);
  let chromiumIncognitoChangeAccessMode = $state<ProtectedAccessMode>("all_time");
  let chromiumIncognitoSettingsChangeAllowed = $state(true);
  let chromiumIncognitoUrlBlockCount = $state(0);
  let chromiumIncognitoUnsupportedPatternCount = $state(0);
  let chromiumIncognitoUrlBlockLimitExceeded = $state(false);
  let tier1EditUnlocking = $state(false);
  let tier1EditUnlockedUntil: string | null = $state(null);
  let protectedAccessOpenFromDaemon: boolean | null = $state(null);
  let protectedAccessLabelFromDaemon: string | null = $state(null);
  let tier1EditMessage: string | null = $state(null);
  let nowMs = $state(Date.now());

  let policyExportRunning = $state(false);
  let policyImportRunning = $state(false);
  let policyTransferMessage: string | null = $state(null);
  let policyTransferError: string | null = $state(null);


  let daemonOnline = $derived(status?.status === "ok");
  let activeViewTitle = $derived(
    navItems.find((item) => item.id === activeView)?.label ?? "Overview"
  );
  let tier1EditUnlocked = $derived(
    Boolean(tier1EditUnlockedUntil && Date.parse(tier1EditUnlockedUntil) > nowMs)
  );
  let protectedAccessOpen = $derived(protectedAccessOpenFromDaemon ?? false);
  let protectedAccessLabel = $derived(protectedAccessLabelFromDaemon ?? "Unavailable");
  let activeDetoxSessions = $derived(
    detoxSessionList.filter(
      (session) =>
        session.status === "active" &&
        !session.cancelled_at &&
        Date.parse(session.ends_at) > nowMs
    )
  );
  let activeDetoxSiteRuleIds = $derived([
    ...new Set(activeDetoxSessions.flatMap((session) => session.site_rule_ids))
  ]);
  let activeDetoxAppRuleIds = $derived([
    ...new Set(activeDetoxSessions.flatMap((session) => session.app_rule_ids))
  ]);

  onMount(() => {
    uiPreferences = applicationUiPreferences();
    if (uiPreferences.restoreLastSelectedPage) {
      const savedView = lastSelectedView();
      if (savedView) {
        setActiveView(savedView);
      }
    }
    const clockInterval = window.setInterval(() => {
      nowMs = Date.now();
    }, 1000);
    configureRuntimeRefresh(uiPreferences.refreshIntervalSeconds);
    showFirstRunOverview = !firstRunOverviewDismissed();
    void loadUninstallPhrase();
    void loadInstallationInfo();
    void loadNotificationPreferences();
    void refreshAll();
    let disposed = false;
    let removeOpenViewListener: (() => void) | null = null;
    let removeRuntimeRefreshListener: (() => void) | null = null;

    void listen<string>(TRAY_OPEN_VIEW_EVENT, (event) => {
      if (isViewId(event.payload)) {
        setActiveView(event.payload);
        void refreshAll({ silent: true });
      }
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        removeOpenViewListener = unlisten;
      }
    });

    void listen(TRAY_RUNTIME_REFRESH_EVENT, () => {
      void refreshAll({ silent: true });
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        removeRuntimeRefreshListener = unlisten;
      }
    });

    return () => {
      disposed = true;
      window.clearInterval(clockInterval);
      if (runtimeRefreshTimerId !== null) {
        window.clearInterval(runtimeRefreshTimerId);
        runtimeRefreshTimerId = null;
      }
      removeOpenViewListener?.();
      removeRuntimeRefreshListener?.();
    };
  });

  function isViewId(value: unknown): value is ViewId {
    return typeof value === "string" && navItems.some((item) => item.id === value);
  }

  function socketArg(): string | undefined {
    return undefined;
  }

  function setActiveView(view: ViewId): void {
    if (view === "admin") {
      settingsOpen = true;
      void loadNotificationPreferences();
      return;
    }
    activeView = view;
    saveLastSelectedView(view);
    if (view === "apps" && !runningAppsLoadedOnce && !runningAppsLoading) {
      runningAppsLoadedOnce = true;
      void refreshRunningApps({ silent: true });
    }
    if (view === "statistics") {
      void refreshScheduleActivityStatistics();
    }
  }

  function closeSettings(): void {
    settingsOpen = false;
  }

  function configureRuntimeRefresh(intervalSeconds: ApplicationUiPreferences["refreshIntervalSeconds"]): void {
    if (runtimeRefreshTimerId !== null) {
      window.clearInterval(runtimeRefreshTimerId);
    }
    runtimeRefreshTimerId = window.setInterval(() => {
      void refreshRuntime({ silent: true });
    }, intervalSeconds * 1_000);
  }

  function updateTimeFormat(timeFormat: ApplicationUiPreferences["timeFormat"]): void {
    uiPreferences = { ...uiPreferences, timeFormat };
    saveApplicationUiPreferences(uiPreferences);
  }

  async function refreshRuntime(options: RefreshOptions = {}): Promise<void> {
    if (refreshInFlight) return;

    refreshInFlight = true;
    if (!options.silent) {
      loading = true;
    }
    lastError = null;

    try {
      const [
        statusResult,
        enforcementResult,
        detoxResult,
        logSummaryResult,
        healthResult,
        tier1EditStatusResult
      ] = await Promise.allSettled([
        daemonStatus(socketArg()),
        enforcementStatus(socketArg()),
        detoxSessions(false, socketArg()),
        logSummary(socketArg()),
        systemHealth(socketArg()),
        tier1EditStatus(socketArg())
      ]);

      applyRuntimeRefreshResults(
        statusResult,
        enforcementResult,
        detoxResult,
        logSummaryResult,
        healthResult,
        tier1EditStatusResult
      );
      if (activeView === "apps") {
        void refreshRunningApps({ silent: true });
      }
      if (activeView === "statistics") {
        void refreshScheduleActivityStatistics();
      }
      lastRefresh = new Date().toLocaleTimeString();
    } finally {
      refreshInFlight = false;
      if (!options.silent) {
        loading = false;
      }
    }
  }

  async function refreshAll(options: RefreshOptions = {}): Promise<void> {
    if (refreshInFlight) return;

    refreshInFlight = true;
    if (!options.silent) {
      loading = true;
    }
    lastError = null;

    try {
      const [
        statusResult,
        enforcementResult,
        configResult,
        detoxResult,
        logSummaryResult,
        healthResult,
        tier1EditStatusResult
      ] = await Promise.allSettled([
        daemonStatus(socketArg()),
        enforcementStatus(socketArg()),
        configSnapshot(socketArg()),
        detoxSessions(false, socketArg()),
        logSummary(socketArg()),
        systemHealth(socketArg()),
        tier1EditStatus(socketArg())
      ]);

      applyRuntimeRefreshResults(
        statusResult,
        enforcementResult,
        detoxResult,
        logSummaryResult,
        healthResult,
        tier1EditStatusResult
      );

      if (configResult.status === "fulfilled") {
        config = configResult.value;
        syncConfigSelection(configResult.value);
      }

      if (activeView === "apps" || runningAppsLoadedOnce) {
        void refreshRunningApps({ silent: true });
      }
      if (activeView === "statistics") {
        void refreshScheduleActivityStatistics();
      }

      lastRefresh = new Date().toLocaleTimeString();
    } finally {
      refreshInFlight = false;
      if (!options.silent) {
        loading = false;
      }
    }
  }

  function applyRuntimeRefreshResults(
    statusResult: PromiseSettledResult<DaemonStatus>,
    enforcementResult: PromiseSettledResult<EnforcementStatus>,
    detoxResult: PromiseSettledResult<{ sessions: DetoxSession[] }>,
    logSummaryResult: PromiseSettledResult<LogSummary>,
    healthResult: PromiseSettledResult<SystemHealth>,
    tier1EditStatusResult: PromiseSettledResult<Tier1EditStatus>
  ): void {
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

    if (detoxResult.status === "fulfilled") {
      detoxSessionList = detoxResult.value.sessions;
    }

    if (logSummaryResult.status === "fulfilled") {
      logStatistics = logSummaryResult.value;
    }

    if (healthResult.status === "fulfilled") {
      health = healthResult.value;
    }

    if (tier1EditStatusResult.status === "fulfilled") {
      tier1EditUnlockedUntil = tier1EditStatusResult.value.active
        ? (tier1EditStatusResult.value.expires_at ?? null)
        : null;
      protectedAccessMode =
        tier1EditStatusResult.value.protected_access_mode ??
        (tier1EditStatusResult.value.operator_window_restriction_enabled ? "sunday" : "all_time");
      protectedAccessOpenFromDaemon =
        tier1EditStatusResult.value.protected_access_open ??
        tier1EditStatusResult.value.operator_window_open ??
        null;
      protectedAccessLabelFromDaemon =
        tier1EditStatusResult.value.protected_access_label ??
        tier1EditStatusResult.value.operator_window_label ??
        null;
      unsupportedBrowserBlockMode =
        tier1EditStatusResult.value.unsupported_browser_block_mode ?? "all_time";
      unsupportedBrowserBlockActive =
        tier1EditStatusResult.value.unsupported_browser_block_active ?? false;
      chromiumIncognitoMode =
        tier1EditStatusResult.value.chromium_incognito_mode ?? "manual_consent";
      chromiumIncognitoDisableScope =
        tier1EditStatusResult.value.chromium_incognito_disable_scope ?? "all_time";
      chromiumIncognitoPrivateBrowsingDisabled =
        tier1EditStatusResult.value.chromium_incognito_private_browsing_disabled ?? false;
      chromiumIncognitoChangeAccessMode =
        tier1EditStatusResult.value.chromium_incognito_change_access_mode ?? "all_time";
      chromiumIncognitoSettingsChangeAllowed =
        tier1EditStatusResult.value.chromium_incognito_settings_change_allowed ?? true;
      chromiumIncognitoUrlBlockCount =
        tier1EditStatusResult.value.chromium_incognito_url_block_count ?? 0;
      chromiumIncognitoUnsupportedPatternCount =
        tier1EditStatusResult.value.chromium_incognito_unsupported_pattern_count ?? 0;
      chromiumIncognitoUrlBlockLimitExceeded =
        tier1EditStatusResult.value.chromium_incognito_url_block_limit_exceeded ?? false;
      tier1EditCredentialConfigured = tier1EditStatusResult.value.credential_configured ?? false;
    } else {
      protectedAccessOpenFromDaemon = null;
      protectedAccessLabelFromDaemon = null;
      unsupportedBrowserBlockActive = false;
      chromiumIncognitoMode = "manual_consent";
      chromiumIncognitoDisableScope = "all_time";
      chromiumIncognitoPrivateBrowsingDisabled = false;
      chromiumIncognitoChangeAccessMode = "all_time";
      chromiumIncognitoSettingsChangeAllowed = true;
      chromiumIncognitoUrlBlockCount = 0;
      chromiumIncognitoUnsupportedPatternCount = 0;
      chromiumIncognitoUrlBlockLimitExceeded = false;
      tier1EditCredentialConfigured = false;
    }
  }

  async function refreshRunningApps(options: RefreshOptions = {}): Promise<void> {
    if (runningAppsLoading) return;

    runningAppsLoading = true;
    if (!options.silent) {
      runningAppsError = null;
    }

    try {
      const response = await fetchRunningApps(socketArg());
      runningApps = response.apps;
      runningAppsError = null;
    } catch (error) {
      runningAppsError = formatError(error);
    } finally {
      runningAppsLoading = false;
    }
  }

  function syncConfigSelection(snapshot: ConfigSnapshot): void {
    if (!ruleDraftHasUnsavedChanges(snapshot)) {
      const selectedRuleSnapshot =
        snapshot.rules.find((rule) => rule.id === selectedRuleId) ?? null;
      if (!selectedRuleSnapshot) {
        selectedRuleId = snapshot.rules[0]?.id ?? null;
        setRuleDraft(snapshot.rules[0] ?? null, snapshot);
      } else {
        setRuleDraft(selectedRuleSnapshot, snapshot);
      }
    }

    if (!appRuleDraftHasUnsavedChanges(snapshot)) {
      const selectedAppRuleSnapshot =
        snapshot.app_rules.find((rule) => rule.id === selectedAppRuleId) ?? null;
      if (!selectedAppRuleSnapshot) {
        selectedAppRuleId = snapshot.app_rules[0]?.id ?? null;
        setAppRuleDraft(snapshot.app_rules[0] ?? null, snapshot);
      } else {
        setAppRuleDraft(selectedAppRuleSnapshot, snapshot);
      }
    }

    if (!scheduleDraftHasUnsavedChanges(snapshot)) {
      const selectedScheduleSnapshot =
        snapshot.schedules.find((schedule) => schedule.id === selectedScheduleId) ?? null;
      if (!selectedScheduleSnapshot) {
        selectedScheduleId = snapshot.schedules[0]?.id ?? null;
        setScheduleDraft(snapshot.schedules[0] ?? null, snapshot);
      } else {
        setScheduleDraft(selectedScheduleSnapshot, snapshot);
      }
    }

    selectedDetoxSiteRuleIds = selectedDetoxSiteRuleIds.filter((ruleId) =>
      snapshot.rules.some((rule) => rule.id === ruleId && rule.tier !== "hard")
    );
    selectedDetoxAppRuleIds = selectedDetoxAppRuleIds.filter((ruleId) =>
      snapshot.app_rules.some((rule) => rule.id === ruleId && rule.tier !== "hard")
    );
  }

  function ruleDraftHasUnsavedChanges(snapshot: ConfigSnapshot): boolean {
    if (!ruleDraft) return false;

    const savedRule = snapshot.rules.find((rule) => rule.id === selectedRuleId) ?? null;
    if (!savedRule) return true;

    if (!sameDraft(normalizeRuleDraft(ruleDraft), normalizeRuleDraft(savedRule))) {
      return true;
    }

    if (ruleDraft.tier !== "controlled_access" && savedRule.tier !== "controlled_access") {
      return false;
    }

    const draftAllowance = ruleAllowanceDraft ?? defaultAllowanceForRule(ruleDraft);
    const savedAllowance = cloneAllowanceForRule(savedRule, snapshot) ?? defaultAllowanceForRule(savedRule);

    return !sameDraft(
      normalizeAllowanceDraft(draftAllowance, ruleDraft),
      normalizeAllowanceDraft(savedAllowance, savedRule)
    );
  }

  function appRuleDraftHasUnsavedChanges(snapshot: ConfigSnapshot): boolean {
    if (!appRuleDraft) return false;

    const savedRule = snapshot.app_rules.find((rule) => rule.id === selectedAppRuleId) ?? null;
    if (!savedRule) return true;

    if (!sameDraft(normalizeAppRuleDraft(appRuleDraft), normalizeAppRuleDraft(savedRule))) {
      return true;
    }

    if (appRuleDraft.tier !== "controlled_access" && savedRule.tier !== "controlled_access") {
      return false;
    }

    const draftAllowance = appRuleAllowanceDraft ?? defaultAllowanceForRule(appRuleDraft);
    const savedAllowance =
      cloneAllowanceForRule(savedRule, snapshot) ?? defaultAllowanceForRule(savedRule);

    return !sameDraft(
      normalizeAllowanceDraft(draftAllowance, appRuleDraft),
      normalizeAllowanceDraft(savedAllowance, savedRule)
    );
  }

  function siteRuleSaveIsAdditiveOnly(savedRule: Rule, snapshot: ConfigSnapshot | null): boolean {
    if (activeDetoxSiteRuleIds.includes(savedRule.id)) return true;

    return (
      ruleIsActive(savedRule, snapshot?.schedules ?? []) &&
      !(savedRule.tier === "hard" && tier1EditUnlocked)
    );
  }

  function appRuleSaveIsAdditiveOnly(
    savedRule: AppRule,
    snapshot: ConfigSnapshot | null
  ): boolean {
    return (
      activeDetoxAppRuleIds.includes(savedRule.id) ||
      appRuleIsActive(savedRule, snapshot?.schedules ?? [])
    );
  }

  function normalizeRuleDraftForSave(draft: Rule, snapshot: ConfigSnapshot | null): Rule {
    const normalized = normalizeRuleDraft(draft);
    const savedRule = snapshot?.rules.find((rule) => rule.id === draft.id) ?? null;
    if (!savedRule || !siteRuleSaveIsAdditiveOnly(savedRule, snapshot)) {
      return normalized;
    }

    return {
      ...normalized,
      id: savedRule.id,
      name: savedRule.name,
      tier: savedRule.tier,
      enabled: savedRule.enabled,
      allowance_id: savedRule.allowance_id ?? null,
      schedule_ids: [...savedRule.schedule_ids],
      patterns: [
        ...savedRule.patterns.map((pattern) => ({ ...pattern })),
        ...normalized.patterns.slice(savedRule.patterns.length)
      ]
    };
  }

  function normalizeAppRuleDraftForSave(draft: AppRule, snapshot: ConfigSnapshot | null): AppRule {
    const normalized = normalizeAppRuleDraft(draft);
    const savedRule = snapshot?.app_rules.find((rule) => rule.id === draft.id) ?? null;
    if (!savedRule || !appRuleSaveIsAdditiveOnly(savedRule, snapshot)) {
      return normalized;
    }

    return {
      ...normalized,
      id: savedRule.id,
      name: savedRule.name,
      tier: savedRule.tier,
      enabled: savedRule.enabled,
      allowance_id: savedRule.allowance_id ?? null,
      schedule_ids: [...savedRule.schedule_ids],
      matchers: [
        ...savedRule.matchers.map((matcher) => ({ ...matcher })),
        ...normalized.matchers.slice(savedRule.matchers.length)
      ]
    };
  }

  function scheduleDraftHasUnsavedChanges(snapshot: ConfigSnapshot): boolean {
    if (!scheduleDraft) return false;

    const savedSchedule =
      snapshot.schedules.find((schedule) => schedule.id === selectedScheduleId) ?? null;
    if (!savedSchedule) return true;

    return (
      !sameDraft(normalizeScheduleDraft(scheduleDraft), normalizeScheduleDraft(savedSchedule)) ||
      !sameIdList(scheduleSiteRuleIds, scheduleTargetsFor(savedSchedule.id, snapshot.rules)) ||
      !sameIdList(scheduleAppRuleIds, scheduleTargetsFor(savedSchedule.id, snapshot.app_rules))
    );
  }

  function sameDraft(left: unknown, right: unknown): boolean {
    return stableStringify(left) === stableStringify(right);
  }

  function stableStringify(value: unknown): string {
    if (Array.isArray(value)) {
      return `[${value.map((item) => stableStringify(item)).join(",")}]`;
    }

    if (value && typeof value === "object") {
      const record = value as Record<string, unknown>;
      return `{${Object.keys(record)
        .sort()
        .map((key) => `${JSON.stringify(key)}:${stableStringify(record[key])}`)
        .join(",")}}`;
    }

    return JSON.stringify(value);
  }

  async function runUrlCheck(): Promise<void> {
    const normalizedUrl = normalizeProbeUrl(testUrl);
    if (!normalizedUrl) return;

    urlChecking = true;
    urlDecision = null;
    lastError = null;
    try {
      testUrl = normalizedUrl;
      urlDecision = await evaluateUrl(normalizedUrl, socketArg(), true);
    } catch (error) {
      lastError = formatError(error);
    } finally {
      urlChecking = false;
    }
  }

  function normalizeProbeUrl(value: string): string {
    const trimmed = value.trim();
    if (!trimmed) return "";
    return /^[a-z][a-z\d+.-]*:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`;
  }

  async function runUnlock(): Promise<void> {
    unlocking = true;
    unlockResult = null;
    lastError = null;
    try {
      unlockResult = await requestUnlock(unlockTarget, unlockReason, socketArg());
      unlockReason = "";
      await refreshLogStatistics();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      unlocking = false;
    }
  }

  async function runStartDetox(): Promise<void> {
    const durationMultipliers: Record<DetoxDurationUnit, number> = {
      minutes: 1,
      hours: 60,
      days: 24 * 60,
      weeks: 7 * 24 * 60
    };
    const durationMinutes = Math.max(
      0,
      Math.round(Number(detoxDurationValue ?? 0) * durationMultipliers[detoxDurationUnit])
    );
    if (durationMinutes <= 0) return;
    if (selectedDetoxSiteRuleIds.length + selectedDetoxAppRuleIds.length === 0) return;

    detoxStarting = true;
    detoxMessage = null;
    lastError = null;
    try {
      const response = await startDetox(
        detoxName.trim() || null,
        durationMinutes,
        selectedDetoxSiteRuleIds,
        selectedDetoxAppRuleIds,
        socketArg()
      );
      detoxMessage = `Detox active until ${new Date(response.session.ends_at).toLocaleString()}.`;
      await refreshDetoxAndLogStatistics();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      detoxStarting = false;
    }
  }

  async function runCancelDetox(id: string): Promise<void> {
    detoxCancellingId = id;
    detoxMessage = null;
    lastError = null;
    try {
      const response = await cancelDetox(id, socketArg());
      detoxMessage = `Detox ${response.session.status}.`;
      await refreshDetoxAndLogStatistics();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      detoxCancellingId = null;
    }
  }

  async function refreshLogStatistics(): Promise<void> {
    try {
      logStatistics = await logSummary(socketArg());
    } catch {
      // Full refresh surfaces connection errors; lightweight statistics refresh does not interrupt actions.
    }
  }

  async function refreshScheduleActivityStatistics(): Promise<void> {
    try {
      scheduleActivityStatistics = await scheduleActivitySummary(socketArg());
    } catch {
      // Full refresh surfaces connection errors; the Statistics page stays usable with its last total.
    }
  }

  async function refreshDetoxAndLogStatistics(): Promise<void> {
    try {
      const [detoxResult, logSummaryResult] = await Promise.allSettled([
        detoxSessions(false, socketArg()),
        logSummary(socketArg())
      ]);
      if (detoxResult.status === "fulfilled") {
        detoxSessionList = detoxResult.value.sessions;
      }
      if (logSummaryResult.status === "fulfilled") {
        logStatistics = logSummaryResult.value;
      }
    } catch {
      // Full refresh surfaces connection errors; action handlers keep their own result state.
    }
  }

  async function loadUninstallPhrase(): Promise<void> {
    uninstallPhraseLoading = true;
    uninstallPhraseError = null;
    try {
      const credentials = await recoveryCredentials();
      recoveryUninstallPhrase = credentials.uninstall_phrase;
      recoveryTier1Key = credentials.tier1_edit_key;
    } catch (error) {
      recoveryUninstallPhrase = null;
      recoveryTier1Key = null;
      uninstallPhraseError = formatError(error);
    } finally {
      uninstallPhraseLoading = false;
    }
  }

  async function hideDisplayedRecoveryCredentials(): Promise<void> {
    if (!window.confirm("Hide and permanently delete both recovery credentials from /etc/blockuntu? This cannot be undone.")) return;
    try {
      await hideRecoveryCredentials(socketArg());
      recoveryUninstallPhrase = null;
      recoveryTier1Key = null;
      tier1EditMessage = "Recovery credentials were removed.";
    } catch (error) {
      lastError = formatError(error);
    }
  }

  async function loadInstallationInfo(): Promise<void> {
    try {
      const info = await installationInfo();
      installationSerial = info.installation_serial;
      buildNumber = info.build_number;
    } catch {
      installationSerial = null;
      buildNumber = null;
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
        ? "Tier 1 edits unlocked for 5 minutes."
        : "Tier 1 edits remain locked.";
      tier1EditPhraseInput = "";
    } catch (error) {
      tier1EditMessage = null;
      lastError = formatError(error);
    } finally {
      tier1EditUnlocking = false;
    }
  }

  async function updateProtectedAccessMode(mode: ProtectedAccessMode): Promise<void> {
    try {
      await setProtectedAccessMode(mode, socketArg());
      protectedAccessMode = mode;
      await refreshRuntime({ silent: true });
    } catch (error) {
      lastError = formatError(error);
    }
  }

  async function updateUnsupportedBrowserBlockMode(mode: ProtectedAccessMode): Promise<void> {
    try {
      const result = await setUnsupportedBrowserBlockMode(mode, socketArg());
      unsupportedBrowserBlockMode = result.mode;
      unsupportedBrowserBlockActive = result.active;
      await refreshRuntime({ silent: true });
    } catch (error) {
      lastError = formatError(error);
    }
  }

  async function updateChromiumIncognitoMode(mode: ChromiumIncognitoMode): Promise<void> {
    try {
      const result = await setChromiumIncognitoMode(mode, socketArg());
      chromiumIncognitoMode = result.mode;
      chromiumIncognitoUrlBlockCount = result.url_block_count;
      chromiumIncognitoUnsupportedPatternCount = result.unsupported_pattern_count;
      await refreshRuntime({ silent: true });
    } catch (error) {
      lastError = formatError(error);
    }
  }

  async function updateChromiumIncognitoDisableScope(
    scope: ChromiumIncognitoDisableScope
  ): Promise<void> {
    try {
      const result = await setChromiumIncognitoDisableScope(scope, socketArg());
      chromiumIncognitoDisableScope = result.scope;
      chromiumIncognitoPrivateBrowsingDisabled = result.private_browsing_disabled;
      await refreshRuntime({ silent: true });
    } catch (error) {
      lastError = formatError(error);
    }
  }

  async function updateChromiumIncognitoChangeAccessMode(
    mode: ProtectedAccessMode
  ): Promise<void> {
    try {
      const result = await setChromiumIncognitoChangeAccessMode(mode, socketArg());
      chromiumIncognitoChangeAccessMode = result.mode;
      await refreshRuntime({ silent: true });
    } catch (error) {
      lastError = formatError(error);
    }
  }

  async function runUninstallBlockuntu(): Promise<void> {
    if (!uninstallPhraseInput.trim()) return;
    uninstallRunning = true;
    uninstallResult = null;
    lastError = null;
    try {
      uninstallResult = await uninstallBlockuntu(uninstallPhraseInput);
      clearFirstRunOverviewDismissed();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      uninstallRunning = false;
    }
  }

  async function runExportPolicyToml(): Promise<void> {
    policyExportRunning = true;
    policyTransferMessage = null;
    policyTransferError = null;
    lastError = null;
    try {
      const result = await exportPolicyToml(socketArg());
      policyTransferMessage = result.detail;
    } catch (error) {
      policyTransferError = formatError(error);
    } finally {
      policyExportRunning = false;
    }
  }

  async function runImportPolicyToml(): Promise<void> {
    policyImportRunning = true;
    policyTransferMessage = null;
    policyTransferError = null;
    lastError = null;
    try {
      const result: PolicyFileResult = await importPolicyToml(socketArg());
      policyTransferMessage = result.detail;
      if (result.config) {
        config = result.config;
        syncConfigSelection(result.config);
      }
      await refreshAll({ silent: true });
    } catch (error) {
      policyTransferError = formatError(error);
    } finally {
      policyImportRunning = false;
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
      name: `Website ${index}`,
      tier: "controlled_access",
      enabled: true,
      patterns: [{ kind: "domain", value: "", match_subdomains: true }],
      schedule_ids: [],
      allowance_id: null
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
      const normalizedRuleDraft = normalizeRuleDraft(ruleDraft);
      if (normalizedRuleDraft.patterns.length === 0) {
        lastError = "Add at least one website pattern before saving.";
        return;
      }

      const socket = socketArg();
      const savedRule = config?.rules.find((rule) => rule.id === ruleDraft?.id) ?? null;
      const additiveOnlySave = savedRule ? siteRuleSaveIsAdditiveOnly(savedRule, config) : false;
      if (normalizedRuleDraft.tier === "controlled_access" && !additiveOnlySave) {
        const allowance = normalizeAllowanceDraft(
          ruleAllowanceDraft ?? defaultAllowanceForRule(normalizedRuleDraft),
          normalizedRuleDraft
        );
        const allowanceResponse = await upsertAllowance(allowance, socket);
        config = allowanceResponse.config;
        ruleAllowanceDraft = cloneAllowance(allowance);
        normalizedRuleDraft.allowance_id = allowance.id;
      } else if (savedRule && additiveOnlySave) {
        normalizedRuleDraft.allowance_id = savedRule.allowance_id ?? null;
      } else {
        normalizedRuleDraft.allowance_id = null;
      }

      const savedRuleDraft = normalizeRuleDraftForSave(normalizedRuleDraft, config);
      const response = await upsertSiteList(savedRuleDraft, socket);
      config = response.config;
      selectedRuleId = savedRuleDraft.id;
      setRuleDraft(
        response.config.rules.find((rule) => rule.id === selectedRuleId) ??
          response.config.rules[0] ??
          null,
        response.config
      );
      ruleMessage = "Saved.";
      await refreshLogStatistics();
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
      await refreshLogStatistics();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      ruleSaving = false;
    }
  }

  function selectAppRule(rule: AppRule): void {
    selectedAppRuleId = rule.id;
    setAppRuleDraft(rule, config);
    appRuleMessage = null;
  }

  function setAppRuleDraft(
    rule: AppRule | null,
    snapshot: ConfigSnapshot | null = config
  ): void {
    appRuleDraft = rule ? cloneAppRule(rule) : null;
    appRuleAllowanceDraft = rule ? cloneAllowanceForRule(rule, snapshot) : null;
  }

  function startNewAppRule(): void {
    const { id, index } = nextAvailableIndexedId(
      config?.app_rules.map((rule) => rule.id) ?? [],
      "app-rule"
    );
    selectedAppRuleId = null;
    appRuleDraft = {
      id,
      name: `Application ${index}`,
      tier: "hard",
      enabled: true,
      matchers: [{ kind: "command_name", value: "" }],
      schedule_ids: [],
      allowance_id: null
    };
    appRuleAllowanceDraft = null;
    appRuleMessage = null;
  }

  function startNewAppRuleFromRunningApp(app: RunningApp): void {
    startNewAppRule();
    if (!appRuleDraft) return;
    appRuleDraft.name = app.display_name;
    appRuleDraft.matchers = detectedMatchersForRunningApp(app);
    appRuleMessage = `Loaded detected matchers from PID ${app.pid}.`;
  }

  function addDetectedMatchersToDraft(app: RunningApp): void {
    if (!appRuleDraft) {
      startNewAppRuleFromRunningApp(app);
      return;
    }

    appRuleDraft.matchers = mergeAppMatchers(
      appRuleDraft.matchers,
      detectedMatchersForRunningApp(app)
    );
    appRuleMessage = `Merged detected matchers from PID ${app.pid}.`;
  }

  async function saveAppRuleDraft(): Promise<void> {
    if (!appRuleDraft) return;
    appRuleSaving = true;
    lastError = null;
    appRuleMessage = null;
    try {
      if (normalizeAppRuleDraft(appRuleDraft).matchers.length === 0) {
        lastError = "Add at least one application matcher before saving.";
        return;
      }

      const socket = socketArg();
      const savedRule = config?.app_rules.find((rule) => rule.id === appRuleDraft?.id) ?? null;
      const additiveOnlySave = savedRule ? appRuleSaveIsAdditiveOnly(savedRule, config) : false;
      if (appRuleDraft.tier === "controlled_access" && !additiveOnlySave) {
        const allowance = normalizeAllowanceDraft(
          appRuleAllowanceDraft ?? defaultAllowanceForRule(appRuleDraft),
          appRuleDraft
        );
        const allowanceResponse = await upsertAllowance(allowance, socket);
        config = allowanceResponse.config;
        appRuleAllowanceDraft = cloneAllowance(allowance);
        appRuleDraft.allowance_id = allowance.id;
      } else if (savedRule && additiveOnlySave) {
        appRuleDraft.allowance_id = savedRule.allowance_id ?? null;
      } else {
        appRuleDraft.allowance_id = null;
      }

      const savedAppRuleDraft = normalizeAppRuleDraftForSave(appRuleDraft, config);
      const response = await upsertAppRule(savedAppRuleDraft, socket);
      config = response.config;
      selectedAppRuleId = savedAppRuleDraft.id;
      setAppRuleDraft(
        response.config.app_rules.find((rule) => rule.id === selectedAppRuleId) ??
          response.config.app_rules[0] ??
          null,
        response.config
      );
      appRuleMessage = "Saved.";
      await refreshLogStatistics();
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
      setAppRuleDraft(response.config.app_rules[0] ?? null, response.config);
      appRuleMessage = "Deleted.";
      await refreshLogStatistics();
    } catch (error) {
      lastError = formatError(error);
    } finally {
      appRuleSaving = false;
    }
  }

  function selectSchedule(schedule: Schedule): void {
    selectedScheduleId = schedule.id;
    setScheduleDraft(schedule);
    scheduleMessage = null;
  }

  function setScheduleDraft(
    schedule: Schedule | null,
    snapshot: ConfigSnapshot | null = config
  ): void {
    scheduleDraft = schedule ? cloneSchedule(schedule) : null;
    scheduleSiteRuleIds = schedule ? scheduleTargetsFor(schedule.id, snapshot?.rules ?? []) : [];
    scheduleAppRuleIds = schedule ? scheduleTargetsFor(schedule.id, snapshot?.app_rules ?? []) : [];
  }

  function scheduleTargetsFor(
    scheduleId: string,
    rules: Array<Rule | AppRule>
  ): string[] {
    return rules
      .filter((rule) => rule.tier !== "hard" && rule.schedule_ids.includes(scheduleId))
      .map((rule) => rule.id);
  }

  function sameIdList(left: string[], right: string[]): boolean {
    return left.length === right.length && left.every((id) => right.includes(id));
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
    scheduleSiteRuleIds = [];
    scheduleAppRuleIds = [];
    scheduleMessage = null;
  }

  async function saveScheduleDraft(): Promise<void> {
    if (!scheduleDraft) return;
    scheduleSaving = true;
    lastError = null;
    scheduleMessage = null;
    try {
      const normalizedScheduleDraft = normalizeScheduleDraft(scheduleDraft);
      const response = await upsertSchedule(
        normalizedScheduleDraft,
        socketArg(),
        scheduleSiteRuleIds,
        scheduleAppRuleIds
      );
      config = response.config;
      selectedScheduleId = normalizedScheduleDraft.id;
      setScheduleDraft(
        response.config.schedules.find((schedule) => schedule.id === selectedScheduleId) ??
          response.config.schedules[0] ??
          null,
        response.config
      );
      scheduleMessage = "Saved.";
      await refreshLogStatistics();
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
      setScheduleDraft(response.config.schedules[0] ?? null, response.config);
      scheduleMessage = "Deleted.";
      await refreshLogStatistics();
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

  async function loadNotificationPreferences(): Promise<void> {
    notificationPreferencesError = null;
    try {
      notificationPreferenceState = await fetchNotificationPreferences(socketArg());
    } catch (error) {
      notificationPreferenceState = null;
      notificationPreferencesError = formatError(error);
    }
  }

  async function updateNotificationPreferences(
    preferences: NotificationPreferences
  ): Promise<void> {
    notificationPreferencesSaving = true;
    notificationPreferencesError = null;
    try {
      notificationPreferenceState = await setNotificationPreferences(preferences, socketArg());
    } catch (error) {
      notificationPreferencesError = formatError(error);
    } finally {
      notificationPreferencesSaving = false;
    }
  }

  function showFirstRunOverviewAgain(): void {
    clearFirstRunOverviewDismissed();
    showFirstRunOverview = true;
    closeSettings();
    setActiveView("overview");
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
          onclick={() => setActiveView(item.id)}
        >
          <IconComponent size={18} aria-hidden="true" />
          <span>{item.label}</span>
        </button>
      {/each}
    </nav>

    <button
      class="sidebar-settings"
      class:active={settingsOpen}
      title="Settings"
      onclick={() => setActiveView("admin")}
    >
      <Settings size={18} aria-hidden="true" />
      <span>Settings</span>
    </button>

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
        <button class="icon-button" title="Refresh" onclick={() => refreshAll()} disabled={loading}>
          <span class:spin={loading}>
            <RefreshCw size={18} aria-hidden="true" />
          </span>
        </button>
      </div>
    </header>

    {#if lastError}
      <section class="alert-row" role="alert">
        <AlertTriangle size={18} aria-hidden="true" />
        <span>{lastError}</span>
      </section>
    {/if}

    {#if activeView === "overview"}
      <OverviewView
        {status}
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
        uninstallPhrase={recoveryUninstallPhrase}
        tier1EditKey={recoveryTier1Key}
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
        {activeDetoxSiteRuleIds}
        onSelectRule={selectRule}
        onStartNewRule={startNewRule}
        onSaveRuleDraft={saveRuleDraft}
        onRemoveRuleDraft={removeRuleDraft}
      />
    {:else if activeView === "apps"}
      <AppRulesView
        {config}
        {runningApps}
        {runningAppsError}
        {runningAppsLoading}
        bind:appRuleDraft
        bind:appRuleAllowanceDraft
        {appRuleSaving}
        {appRuleMessage}
        {activeDetoxAppRuleIds}
        onSelectAppRule={selectAppRule}
        onStartNewAppRule={startNewAppRule}
        onAddDetectedMatchers={addDetectedMatchersToDraft}
        onRefreshRunningApps={() => refreshRunningApps()}
        onSaveAppRuleDraft={saveAppRuleDraft}
        onRemoveAppRuleDraft={removeAppRuleDraft}
      />
    {:else if activeView === "detox"}
      <DetoxView
        {config}
        detoxSessions={detoxSessionList}
        bind:detoxName
        bind:detoxDurationValue
        bind:detoxDurationUnit
        bind:selectedSiteRuleIds={selectedDetoxSiteRuleIds}
        bind:selectedAppRuleIds={selectedDetoxAppRuleIds}
        {detoxStarting}
        {detoxCancellingId}
        {detoxMessage}
        {tier1EditUnlocked}
        {nowMs}
        onStartDetox={runStartDetox}
        onCancelDetox={runCancelDetox}
      />
    {:else if activeView === "schedule"}
      <SchedulesView
        {config}
        bind:scheduleDraft
        bind:scheduleSiteRuleIds
        bind:scheduleAppRuleIds
        {scheduleSaving}
        {scheduleMessage}
        timeFormat={uiPreferences.timeFormat}
        onSelectSchedule={selectSchedule}
        onStartNewSchedule={startNewSchedule}
        onSaveScheduleDraft={saveScheduleDraft}
        onRemoveScheduleDraft={removeScheduleDraft}
      />
    {:else if activeView === "statistics"}
      <StatisticsView
        logSummary={logStatistics}
        scheduleActivitySummary={scheduleActivityStatistics}
      />
    {/if}

    <footer class="footer-line">
      <span>{lastRefresh ? `Last refresh ${lastRefresh}` : "Not refreshed"}</span>
    </footer>
  </main>

  {#if settingsOpen}
    <AdminView
        {health}
        {enforcement}
        notificationPreferences={notificationPreferenceState}
        {notificationPreferencesSaving}
        {notificationPreferencesError}
        {installationSerial}
        {buildNumber}
        {uninstallPhraseLoading}
        bind:uninstallPhraseInput
        {uninstallRunning}
        {uninstallResult}
        {uninstallPhraseError}
        bind:tier1EditPhraseInput
        {tier1EditUnlocking}
        tier1EditUnlockedUntil={tier1EditUnlockedUntil}
        {protectedAccessMode}
        {protectedAccessOpen}
        {protectedAccessLabel}
        {unsupportedBrowserBlockMode}
        {unsupportedBrowserBlockActive}
        {chromiumIncognitoMode}
        {chromiumIncognitoDisableScope}
        {chromiumIncognitoPrivateBrowsingDisabled}
        {chromiumIncognitoChangeAccessMode}
        {chromiumIncognitoSettingsChangeAllowed}
        {chromiumIncognitoUrlBlockCount}
        {chromiumIncognitoUnsupportedPatternCount}
        {chromiumIncognitoUrlBlockLimitExceeded}
        {tier1EditCredentialConfigured}
        {tier1EditMessage}
        timeFormat={uiPreferences.timeFormat}
        {policyExportRunning}
        {policyImportRunning}
        {policyTransferMessage}
        {policyTransferError}
        onRefreshHealth={() => refreshAll()}
        onRunUninstallBlockuntu={runUninstallBlockuntu}
        onUnlockTier1Edit={runUnlockTier1Edit}
        onUpdateProtectedAccessMode={updateProtectedAccessMode}
        onUpdateUnsupportedBrowserBlockMode={updateUnsupportedBrowserBlockMode}
        onUpdateChromiumIncognitoMode={updateChromiumIncognitoMode}
        onUpdateChromiumIncognitoDisableScope={updateChromiumIncognitoDisableScope}
        onUpdateChromiumIncognitoChangeAccessMode={updateChromiumIncognitoChangeAccessMode}
        onExportPolicyToml={runExportPolicyToml}
        onImportPolicyToml={runImportPolicyToml}
        onUpdateNotificationPreferences={updateNotificationPreferences}
        onShowFirstRunOverview={showFirstRunOverviewAgain}
        onUpdateTimeFormat={updateTimeFormat}
        recoveryCredentialsVisible={Boolean(recoveryUninstallPhrase && recoveryTier1Key)}
        onHideRecoveryCredentials={hideDisplayedRecoveryCredentials}
        onClose={closeSettings}
    />
  {/if}
</div>
