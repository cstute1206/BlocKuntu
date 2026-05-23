export type ViewId =
  | "overview"
  | "blocks"
  | "config"
  | "schedule"
  | "allowances"
  | "statistics"
  | "admin";

export interface DaemonStatus {
  status: string;
  rules: number;
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
  schedules: Schedule[];
  allowances: Allowance[];
  defaults: {
    unlock_policy: UnlockPolicy;
  };
}

export interface ConfigFileResponse {
  path: string;
  toml: string;
}

export interface WriteConfigFileResponse {
  path: string;
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
