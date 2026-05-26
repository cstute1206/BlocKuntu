import { invoke } from "@tauri-apps/api/core";
import type {
  ConfigSnapshot,
  ConfigMutationResponse,
  DaemonStatus,
  DecisionResult,
  EventsResponse,
  Rule,
  Schedule,
  SystemHealth,
  UnlockResult
} from "./types";

export function daemonStatus(socketPath?: string): Promise<DaemonStatus> {
  return invoke("daemon_status", { socketPath });
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

export function daemonRpc(method: string, params: unknown, socketPath?: string): Promise<unknown> {
  return invoke("daemon_rpc", { method, params, socketPath });
}
