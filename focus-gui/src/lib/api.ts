import { invoke } from "@tauri-apps/api/core";
import type {
  ConfigSnapshot,
  ConfigFileResponse,
  DaemonStatus,
  DecisionResult,
  EventsResponse,
  SystemHealth,
  UnlockResult,
  WriteConfigFileResponse
} from "./types";

export function daemonStatus(socketPath?: string): Promise<DaemonStatus> {
  return invoke("daemon_status", { socketPath });
}

export function configSnapshot(socketPath?: string): Promise<ConfigSnapshot> {
  return invoke("config_snapshot", { socketPath });
}

export function configFile(socketPath?: string): Promise<ConfigFileResponse> {
  return daemonRpc("config_file", {}, socketPath) as Promise<ConfigFileResponse>;
}

export function writeConfigFile(
  toml: string,
  socketPath?: string
): Promise<WriteConfigFileResponse> {
  return daemonRpc("write_config_file", { toml }, socketPath) as Promise<WriteConfigFileResponse>;
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
