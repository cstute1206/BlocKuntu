import { invoke } from "@tauri-apps/api/core";
import type {
  ConfigSnapshot,
  ConfigMutationResponse,
  DaemonStatus,
  DecisionResult,
  EnforcementStatus,
  EventsResponse,
  AppRule,
  Allowance,
  Rule,
  Schedule,
  SystemHealth,
  UninstallConfirmation,
  UninstallResult,
  UnlockResult
} from "./types";

export function daemonStatus(socketPath?: string): Promise<DaemonStatus> {
  return invoke("daemon_status", { socketPath });
}

export function enforcementStatus(socketPath?: string): Promise<EnforcementStatus> {
  return invoke("enforcement_status", { socketPath });
}

export function startEnforcement(socketPath?: string): Promise<EnforcementStatus> {
  return daemonRpc("start_enforcement", {}, socketPath) as Promise<EnforcementStatus>;
}

export function stopEnforcement(socketPath?: string): Promise<EnforcementStatus> {
  return daemonRpc("stop_enforcement", {}, socketPath) as Promise<EnforcementStatus>;
}

export function configSnapshot(socketPath?: string): Promise<ConfigSnapshot> {
  return invoke("config_snapshot", { socketPath });
}

export function upsertSiteList(rule: Rule, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "upsert_site_list",
    { rule, now: new Date().toISOString() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function deleteSiteList(id: string, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "delete_site_list",
    { id, now: new Date().toISOString() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function upsertAllowance(
  allowance: Allowance,
  socketPath?: string
): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "upsert_allowance",
    { allowance, now: new Date().toISOString() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function deleteAllowance(id: string, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "delete_allowance",
    { id, now: new Date().toISOString() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function upsertAppRule(
  rule: AppRule,
  socketPath?: string
): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "upsert_app_rule",
    { rule, now: new Date().toISOString() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function deleteAppRule(id: string, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "delete_app_rule",
    { id, now: new Date().toISOString() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function upsertSchedule(
  schedule: Schedule,
  socketPath?: string
): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "upsert_schedule",
    { schedule, now: new Date().toISOString() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function deleteSchedule(id: string, socketPath?: string): Promise<ConfigMutationResponse> {
  return daemonRpc(
    "delete_schedule",
    { id, now: new Date().toISOString() },
    socketPath
  ) as Promise<ConfigMutationResponse>;
}

export function recentEvents(limit = 50, socketPath?: string): Promise<EventsResponse> {
  return invoke("recent_events", { limit, socketPath });
}

export function systemHealth(socketPath?: string): Promise<SystemHealth> {
  return invoke("system_health", { socketPath });
}

export function evaluateUrl(url: string, socketPath?: string): Promise<DecisionResult> {
  return invoke("evaluate_url", { url, socketPath });
}

export function requestUnlock(
  target: string,
  minutes: number,
  reason: string,
  socketPath?: string
): Promise<UnlockResult> {
  return invoke("request_unlock", {
    request: { target, minutes, reason },
    socketPath
  });
}

export function uninstallConfirmationPhrase(): Promise<UninstallConfirmation> {
  return invoke("uninstall_confirmation_phrase");
}

export function uninstallBlockuntu(phrase: string): Promise<UninstallResult> {
  return invoke("uninstall_blockuntu", { phrase });
}

export function daemonRpc(method: string, params: unknown, socketPath?: string): Promise<unknown> {
  return invoke("daemon_rpc", { method, params, socketPath });
}
