import { invoke } from "@tauri-apps/api/core";
import type {
  InstallationInfo,
  ConfigSnapshot,
  ConfigMutationResponse,
  DaemonStatus,
  DecisionResult,
  DetoxMutationResponse,
  DetoxSessionsResponse,
  EnforcementStatus,
  LogSummary,
  NotificationPreferences,
  ScheduleActivitySummary,
  PolicyFileResult,
  RunningAppsResponse,
  AppRule,
  Allowance,
  Rule,
  RecoveryCredentials,
  Schedule,
  SnapPolicyDiagnostic,
  SnapPolicyDiagnosticBrowser,
  SystemHealth,
  Tier1EditStatus,
  ChromiumIncognitoDisableScope,
  ChromiumIncognitoMode,
  ProtectedAccessMode,
  UninstallResult,
  UnlockResult
} from "./types";

export function daemonStatus(socketPath?: string): Promise<DaemonStatus> {
  return invoke("daemon_status", { socketPath });
}

export function enforcementStatus(socketPath?: string): Promise<EnforcementStatus> {
  return invoke("enforcement_status", { socketPath });
}

export function configSnapshot(socketPath?: string): Promise<ConfigSnapshot> {
  return invoke("config_snapshot", { socketPath });
}

export function exportPolicyToml(socketPath?: string): Promise<PolicyFileResult> {
  return invoke("export_policy_toml", { socketPath });
}

export function importPolicyToml(socketPath?: string): Promise<PolicyFileResult> {
  return invoke("import_policy_toml", { socketPath });
}

export function upsertSiteList(rule: Rule, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "upsert_site_list",
    { rule, now: clientNow() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function deleteSiteList(id: string, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "delete_site_list",
    { id, now: clientNow() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function upsertAllowance(
  allowance: Allowance,
  socketPath?: string
): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "upsert_allowance",
    { allowance, now: clientNow() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function deleteAllowance(id: string, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "delete_allowance",
    { id, now: clientNow() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function upsertAppRule(
  rule: AppRule,
  socketPath?: string
): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "upsert_app_rule",
    { rule, now: clientNow() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function deleteAppRule(id: string, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "delete_app_rule",
    { id, now: clientNow() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function upsertSchedule(
  schedule: Schedule,
  socketPath?: string,
  siteRuleIds?: string[],
  appRuleIds?: string[]
): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "upsert_schedule",
    {
      schedule,
      site_rule_ids: siteRuleIds,
      app_rule_ids: appRuleIds,
      now: clientNow()
    },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function deleteSchedule(id: string, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "delete_schedule",
    { id, now: clientNow() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function detoxSessions(
  activeOnly = false,
  socketPath?: string
): Promise<DetoxSessionsResponse> {
  return daemonRpc(
    "detox_sessions",
    { active_only: activeOnly, limit: 80, now: clientNow() },
    socketPath
  ) as Promise<DetoxSessionsResponse>;
}

export function startDetox(
  name: string | null,
  durationMinutes: number,
  siteRuleIds: string[],
  appRuleIds: string[],
  socketPath?: string
): Promise<DetoxMutationResponse> {
  return daemonRpc(
    "start_detox",
    {
      name,
      duration_minutes: durationMinutes,
      site_rule_ids: siteRuleIds,
      app_rule_ids: appRuleIds,
      now: clientNow()
    },
    socketPath
  ) as Promise<DetoxMutationResponse>;
}

export function cancelDetox(id: string, socketPath?: string): Promise<DetoxMutationResponse> {
  return daemonRpc(
    "cancel_detox",
    { id, now: clientNow() },
    socketPath
  ) as Promise<DetoxMutationResponse>;
}

export function logSummary(socketPath?: string): Promise<LogSummary> {
  return daemonRpc("log_summary", {}, socketPath) as Promise<LogSummary>;
}

export function scheduleActivitySummary(socketPath?: string): Promise<ScheduleActivitySummary> {
  return daemonRpc(
    "schedule_activity_summary",
    { now: clientNow() },
    socketPath
  ) as Promise<ScheduleActivitySummary>;
}

export function notificationPreferences(socketPath?: string): Promise<NotificationPreferences> {
  return daemonRpc("notification_preferences", {}, socketPath) as Promise<NotificationPreferences>;
}

export function setNotificationPreferences(
  preferences: NotificationPreferences,
  socketPath?: string
): Promise<NotificationPreferences> {
  return daemonRpc(
    "set_notification_preferences",
    { preferences },
    socketPath
  ) as Promise<NotificationPreferences>;
}

export function runningApps(socketPath?: string): Promise<RunningAppsResponse> {
  return daemonRpc("running_apps", { now: clientNow() }, socketPath) as Promise<RunningAppsResponse>;
}

export function systemHealth(socketPath?: string): Promise<SystemHealth> {
  return invoke("system_health", { socketPath });
}

export function diagnoseSnapPolicy(
  browser: SnapPolicyDiagnosticBrowser,
  socketPath?: string
): Promise<SnapPolicyDiagnostic> {
  return invoke("diagnose_snap_policy", { browser, socketPath });
}

export function evaluateUrl(
  url: string,
  socketPath?: string,
  probe = false
): Promise<DecisionResult> {
  return invoke("evaluate_url", { url, socketPath, probe });
}

export function requestUnlock(
  target: string,
  reason: string,
  socketPath?: string
): Promise<UnlockResult> {
  return invoke("request_unlock", {
    request: { target, reason },
    socketPath
  });
}

export function recoveryCredentials(): Promise<RecoveryCredentials> {
  return invoke("recovery_credentials");
}

export function hideRecoveryCredentials(socketPath?: string): Promise<{ hidden: boolean }> {
  return daemonRpc("hide_recovery_credentials", {}, socketPath) as Promise<{ hidden: boolean }>;
}

export function setProtectedAccessMode(
  mode: ProtectedAccessMode,
  socketPath?: string
): Promise<{ mode: ProtectedAccessMode }> {
  return daemonRpc("set_protected_access_mode", { mode }, socketPath) as Promise<{
    mode: ProtectedAccessMode;
  }>;
}

export function setUnsupportedBrowserBlockMode(
  mode: ProtectedAccessMode,
  socketPath?: string
): Promise<{ mode: ProtectedAccessMode; active: boolean }> {
  return daemonRpc("set_unsupported_browser_block_mode", { mode }, socketPath) as Promise<{
    mode: ProtectedAccessMode;
    active: boolean;
  }>;
}

export function setChromiumIncognitoMode(
  mode: ChromiumIncognitoMode,
  socketPath?: string
): Promise<{
  mode: ChromiumIncognitoMode;
  url_block_count: number;
  unsupported_pattern_count: number;
}> {
  return daemonRpc("set_chromium_incognito_mode", { mode }, socketPath) as Promise<{
    mode: ChromiumIncognitoMode;
    url_block_count: number;
    unsupported_pattern_count: number;
  }>;
}

export function setChromiumIncognitoDisableScope(
  scope: ChromiumIncognitoDisableScope,
  socketPath?: string
): Promise<{ scope: ChromiumIncognitoDisableScope; private_browsing_disabled: boolean }> {
  return daemonRpc("set_chromium_incognito_disable_scope", { scope }, socketPath) as Promise<{
    scope: ChromiumIncognitoDisableScope;
    private_browsing_disabled: boolean;
  }>;
}

export function setChromiumIncognitoChangeAccessMode(
  mode: ProtectedAccessMode,
  socketPath?: string
): Promise<{ mode: ProtectedAccessMode }> {
  return daemonRpc("set_chromium_incognito_change_access_mode", { mode }, socketPath) as Promise<{
    mode: ProtectedAccessMode;
  }>;
}

export function tier1EditStatus(socketPath?: string): Promise<Tier1EditStatus> {
  return daemonRpc(
    "tier1_edit_status",
    { now: clientNow() },
    socketPath
  ) as Promise<Tier1EditStatus>;
}

export function unlockTier1Edit(
  phrase: string,
  socketPath?: string
): Promise<Tier1EditStatus> {
  return daemonRpc(
    "unlock_tier1_edit",
    { phrase, now: clientNow() },
    socketPath
  ) as Promise<Tier1EditStatus>;
}

export function installationInfo(): Promise<InstallationInfo> {
  return invoke("installation_info");
}

export function openExtensionStore(url: string): Promise<void> {
  return invoke("open_extension_store", { url });
}

export function uninstallBlockuntu(phrase: string): Promise<UninstallResult> {
  return invoke("uninstall_blockuntu", { phrase });
}

export function daemonRpc(method: string, params: unknown, socketPath?: string): Promise<unknown> {
  return invoke("daemon_rpc", { method, params, socketPath });
}

function clientNow(date = new Date()): string {
  const offsetMinutes = -date.getTimezoneOffset();
  const sign = offsetMinutes >= 0 ? "+" : "-";
  const absOffsetMinutes = Math.abs(offsetMinutes);
  const offsetHours = Math.floor(absOffsetMinutes / 60);
  const offsetRemainderMinutes = absOffsetMinutes % 60;

  return `${date.getFullYear()}-${padDatePart(date.getMonth() + 1)}-${padDatePart(
    date.getDate()
  )}T${padDatePart(date.getHours())}:${padDatePart(date.getMinutes())}:${padDatePart(
    date.getSeconds()
  )}${sign}${padDatePart(offsetHours)}:${padDatePart(offsetRemainderMinutes)}`;
}

function padDatePart(value: number): string {
  return value.toString().padStart(2, "0");
}
