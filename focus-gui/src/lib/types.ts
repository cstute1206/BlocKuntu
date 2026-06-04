export type ViewId =
  | "overview"
  | "blocks"
  | "apps"
  | "schedule"
  | "statistics"
  | "admin";

export interface DaemonStatus {
  status: string;
  enforcement_state?: "active" | "stopped";
  rules: number;
  app_rules: number;
  schedules: number;
  allowances: number;
}

export interface RulePattern {
  kind: "domain" | "exact_url" | "url_prefix" | "path_prefix";
  value: string;
  match_subdomains: boolean;
}

export interface Rule {
  id: string;
  name: string;
  tier: "hard" | "controlled_access";
  enabled: boolean;
  patterns: RulePattern[];
  schedule_ids: string[];
  allowance_id?: string | null;
  unlock_policy?: UnlockPolicy | null;
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
  tier: "hard" | "controlled_access";
  enabled: boolean;
  matchers: AppMatcher[];
  schedule_ids: string[];
  allowance_id?: string | null;
  unlock_policy?: UnlockPolicy | null;
}

export interface UnlockPolicy {
  max_session_minutes: number;
  cooldown_minutes: number;
  max_unlocks_per_hour: number;
}

export interface ScheduleWindow {
  weekday: "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";
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
  defaults: {
    unlock_policy: UnlockPolicy;
  };
}

export interface ConfigMutationResponse {
  status: string;
  config: ConfigSnapshot;
  updated_at: string;
}

export interface RecentEvent {
  id: number;
  kind: string;
  target?: string | null;
  details?: string | null;
  created_at: string;
}

export interface EventsResponse {
  events: RecentEvent[];
}

export interface HealthCheck {
  key: string;
  label: string;
  state: "ok" | "warn" | "error" | "unknown";
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
  enforcement_state: "active" | "stopped";
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

export interface UninstallConfirmation {
  phrase: string;
}

export interface UninstallResult {
  status: string;
  detail: string;
}
