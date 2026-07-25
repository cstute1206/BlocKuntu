export type ViewId =
  | "overview"
  | "blocks"
  | "apps"
  | "detox"
  | "schedule"
  | "statistics"
  | "admin";

export interface DaemonStatus {
  status: string;
  enforcement_state?: "active" | "uninstalling";
  rules: number;
  app_rules: number;
  schedules: number;
  allowances: number;
}

export interface RulePattern {
  kind: "domain" | "exact_url" | "url_prefix" | "url_contains" | "path_prefix";
  value: string;
  match_subdomains: boolean;
}

export interface Rule {
  id: string;
  name: string;
  tier: "hard" | "scheduled_block" | "controlled_access";
  enabled: boolean;
  patterns: RulePattern[];
  schedule_ids: string[];
  allowance_id?: string | null;
}

export interface AppMatcher {
  kind:
    | "executable_path"
    | "executable_basename"
    | "command_name"
    | "desktop_id"
    | "window_title_exact"
    | "window_title_contains";
  value: string;
}

export interface AppRule {
  id: string;
  name: string;
  tier: "hard" | "scheduled_block" | "controlled_access";
  enabled: boolean;
  matchers: AppMatcher[];
  schedule_ids: string[];
  allowance_id?: string | null;
}

export interface RunningApp {
  pid: number;
  display_name: string;
  executable_path?: string | null;
  executable_basename?: string | null;
  command_name?: string | null;
  desktop_id?: string | null;
  window_titles: string[];
  decision: "allow" | "block";
  blocking_rule_id?: string | null;
  blocking_rule_name?: string | null;
}

export interface WindowDetectionStatus {
  available: boolean;
  provider?: string | null;
  session_type?: string | null;
  detail: string;
}

export type ScheduleDay =
  | "everyday"
  | "workdays"
  | "weekend"
  | "mon"
  | "tue"
  | "wed"
  | "thu"
  | "fri"
  | "sat"
  | "sun";

export interface ScheduleWindow {
  weekday: ScheduleDay;
  start: string;
  end: string;
}

export interface Schedule {
  id: string;
  name?: string | null;
  windows: ScheduleWindow[];
}

export interface Allowance {
  id: string;
  name?: string | null;
  daily_minutes: number;
}

export interface ConfigSnapshot {
  rules: Rule[];
  app_rules: AppRule[];
  schedules: Schedule[];
  allowances: Allowance[];
}

export interface ConfigMutationResponse {
  status: string;
  config: ConfigSnapshot;
  updated_at: string;
}

export interface PolicyFileResult {
  status: "ok" | "cancelled";
  detail: string;
  path?: string | null;
  config?: ConfigSnapshot | null;
}

export interface DetoxSession {
  id: string;
  name?: string | null;
  starts_at: string;
  ends_at: string;
  cancelled_at?: string | null;
  site_rule_ids: string[];
  app_rule_ids: string[];
  status: "scheduled" | "active" | "expired" | "cancelled";
  remaining_seconds?: number | null;
}

export interface DetoxSessionsResponse {
  sessions: DetoxSession[];
}

export interface DetoxMutationResponse {
  status: string;
  session: DetoxSession;
}

export type DetoxDurationUnit = "minutes" | "hours" | "days" | "weeks";

export interface LogSummary {
  path: string;
  total_events: number;
  event_counts: Record<string, number>;
}

export interface NotificationPreferences {
  enabled: boolean;
  website_blocked: boolean;
  application_blocked: boolean;
  allowance_warnings: boolean;
  allowance_warning_minutes: number[];
  schedule_started: boolean;
  schedule_ended: boolean;
  detox_started: boolean;
  detox_ended: boolean;
}

export interface ScheduleActivityTotal {
  id: string;
  name?: string | null;
  total_active_seconds: number;
}

export interface ScheduleActivitySummary {
  tracked_at: string;
  schedules: ScheduleActivityTotal[];
}

export interface RunningAppsResponse {
  apps: RunningApp[];
  window_detection: WindowDetectionStatus;
}

export interface HealthCheck {
  key: string;
  label: string;
  state: "ok" | "inactive" | "pending" | "warn" | "error" | "unknown";
  detail: string;
}

export interface SystemHealth {
  checked_at: string;
  socket_path: string;
  checks: HealthCheck[];
}

export interface FirefoxPolicyStatus {
  path: string;
  extension_id: string;
  extension_xpi: string;
  extension_xpi_exists: boolean;
  policy_exists: boolean;
  valid_json: boolean;
  compliant: boolean;
  managed?: boolean;
  deferred_until_heartbeat?: boolean;
  active_after_heartbeat?: boolean;
  private_browsing_enabled: boolean;
  private_browsing_available: boolean;
  install_url?: string | null;
  detail: string;
}

export interface ChromePolicyStatus {
  path: string;
  update_manifest_path: string;
  extension_id: string;
  extension_version: string;
  extension_crx_url: string;
  update_url: string;
  policy_exists: boolean;
  update_manifest_exists: boolean;
  valid_json: boolean;
  compliant: boolean;
  managed?: boolean;
  deferred_until_heartbeat?: boolean;
  active_after_heartbeat?: boolean;
  update_manifest_compliant: boolean;
  force_install_configured: boolean;
  override_update_url: boolean;
  detail: string;
}

export interface HostsFileStatus {
  path: string;
  expected_domain_count: number;
  managed_block_present: boolean;
  managed_block_compliant: boolean;
  immutable_required: boolean;
  immutable_state: "enabled" | "disabled" | "not_required" | "unknown";
  immutable_detail: string;
  detail: string;
}

export interface EnforcementStatus {
  status: string;
  enforcement_state: "active" | "uninstalling";
  firefox_policy: FirefoxPolicyStatus;
  chrome_policy: ChromePolicyStatus;
  hosts_file: HostsFileStatus;
}

export interface DecisionResult {
  decision: "allow" | "block";
  reason?: {
    kind: string;
    rule_id?: string;
    rule_name?: string;
    controlled_reason?: string;
    message?: string;
  };
}

export interface UnlockResult {
  id: number;
  target: string;
  rule_id: string;
  minutes: number;
  reason: string;
  started_at: string;
  expires_at: string;
}

export interface Tier1EditKey {
  key: string;
}

export interface Tier1EditStatus {
  active: boolean;
  expires_at?: string | null;
  remaining_seconds?: number | null;
  operator_window_open?: boolean;
  operator_window_label?: string;
}

export interface UninstallConfirmation {
  phrase: string;
}

export interface InstallationInfo {
  installation_serial: string | null;
  build_number: string;
}

export interface UninstallResult {
  status: string;
  detail: string;
}
