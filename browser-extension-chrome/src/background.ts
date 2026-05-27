"use strict";

const NATIVE_HOST_NAME = "blockuntu_native";
const EXTENSION_COMPONENT = "chrome_extension";
const BROWSER_NAME = "chrome";
const BLOCKED_PAGE_URL = chrome.runtime.getURL("blocked.html");
const HEARTBEAT_INTERVAL_MS = 5_000;
const HEARTBEAT_TIMEOUT_MS = 75_000;
const RPC_TIMEOUT_MS = 3_000;
const VISIT_HEARTBEAT_INTERVAL_MS = 10_000;
const HEARTBEAT_ALARM = "blockuntu-heartbeat";
const VISIT_ALARM = "blockuntu-visit-heartbeat";

type JsonObject = Record<string, unknown>;

interface PendingRequest {
  method: string;
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timeoutId: number;
}

interface ActiveVisit {
  visitId: number;
  tabId: number;
  url: string;
}

interface BlockNavigationReason extends JsonObject {
  kind: string;
  tier?: string;
  rule_id?: string;
  rule_name?: string;
  controlled_reason?: string;
  blocked_by?: string;
  summary?: string;
  detail?: string;
  free_at?: string;
  allowance_reset_at?: string;
  last_heartbeat_ok_at?: string;
  heartbeat_timeout_seconds?: number;
  message?: string;
}

let nativePort: BlockuntuChromeExtension.RuntimePort | null = null;
let nextRequestId = 1;
let heartbeatInFlight = false;
let lastHeartbeatOkAt = 0;
let backendHealthy = false;

const pendingRequests = new Map<number, PendingRequest>();
const activeVisits = new Map<number, ActiveVisit>();
const navigationTokens = new Map<number, number>();

function connectNativeHost(): BlockuntuChromeExtension.RuntimePort {
  if (nativePort) {
    return nativePort;
  }

  const port = chrome.runtime.connectNative(NATIVE_HOST_NAME);
  port.onMessage.addListener(handleNativeMessage);
  port.onDisconnect.addListener(() => {
    const error = chrome.runtime.lastError;
    const message = error?.message
      ? `native host disconnected: ${error.message}`
      : "native host disconnected";

    nativePort = null;
    markBackendUnhealthy(message);
    rejectAllPending(message);
  });

  nativePort = port;
  return port;
}

function handleNativeMessage(message: unknown): void {
  if (!isObject(message)) {
    rejectAllPending("native host returned a non-object response");
    return;
  }

  const id = message.id;
  if (typeof id === "number") {
    const pending = pendingRequests.get(id);
    if (!pending) {
      console.warn(`BlocKuntu received a response for unknown request ${id}.`);
      return;
    }

    pendingRequests.delete(id);
    globalThis.clearTimeout(pending.timeoutId);

    if (isObject(message.error)) {
      pending.reject(new Error(JSON.stringify(message.error)));
    } else {
      pending.resolve(message.result);
    }
    return;
  }

  if (isObject(message.error)) {
    rejectAllPending(JSON.stringify(message.error));
  }
}

function sendRpc(method: string, params: JsonObject, timeoutMs = RPC_TIMEOUT_MS): Promise<unknown> {
  return new Promise((resolve, reject) => {
    let port: BlockuntuChromeExtension.RuntimePort;
    try {
      port = connectNativeHost();
    } catch (error) {
      const message = errorMessage(error);
      markBackendUnhealthy(message);
      reject(new Error(message));
      return;
    }

    const id = nextRequestId++;
    if (nextRequestId > Number.MAX_SAFE_INTEGER - 1) {
      nextRequestId = 1;
    }

    const timeoutId = globalThis.setTimeout(() => {
      pendingRequests.delete(id);
      reject(new Error(`${method} timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    pendingRequests.set(id, {
      method,
      resolve,
      reject,
      timeoutId,
    });

    try {
      port.postMessage({
        jsonrpc: "2.0",
        id,
        method,
        params,
      });
    } catch (error) {
      globalThis.clearTimeout(timeoutId);
      pendingRequests.delete(id);
      const message = errorMessage(error);
      markBackendUnhealthy(message);
      reject(new Error(message));
    }
  });
}

function rejectAllPending(reason: string): void {
  for (const [id, pending] of pendingRequests) {
    globalThis.clearTimeout(pending.timeoutId);
    pending.reject(new Error(`${pending.method} failed: ${reason}`));
    pendingRequests.delete(id);
  }
}

function sendHeartbeat(): void {
  refreshHealthState();
  if (heartbeatInFlight) {
    return;
  }

  heartbeatInFlight = true;
  sendRpc(
    "extension_heartbeat",
    {
      browser: BROWSER_NAME,
      component: EXTENSION_COMPONENT,
      extension_id: chrome.runtime.id,
      extension_version: chrome.runtime.getManifest().version,
      now: new Date().toISOString(),
    },
    Math.min(RPC_TIMEOUT_MS, HEARTBEAT_INTERVAL_MS)
  )
    .then(() => {
      lastHeartbeatOkAt = Date.now();
      backendHealthy = true;
    })
    .catch((error) => {
      markBackendUnhealthy(`heartbeat failed: ${errorMessage(error)}`);
    })
    .finally(() => {
      heartbeatInFlight = false;
    });
}

function refreshHealthState(): boolean {
  if (lastHeartbeatOkAt === 0) {
    backendHealthy = false;
    return false;
  }

  if (Date.now() - lastHeartbeatOkAt > HEARTBEAT_TIMEOUT_MS) {
    markBackendUnhealthy("heartbeat timed out");
    return false;
  }

  return backendHealthy;
}

function markBackendUnhealthy(reason: string): void {
  if (backendHealthy) {
    console.warn(`BlocKuntu backend unhealthy: ${reason}`);
  }
  backendHealthy = false;
}

function handleBeforeNavigate(details: BlockuntuChromeExtension.NavigationDetails): void {
  if (!shouldHandleNavigation(details)) {
    return;
  }

  const token = nextNavigationToken(details.tabId);
  void endVisitForTab(details.tabId);

  if (!refreshHealthState()) {
    redirectToBlocked(
      details,
      backendBlockReason("backend_unhealthy", "BlocKuntu daemon heartbeat is not healthy")
    );
    return;
  }

  if (!isWebUrl(details.url)) {
    return;
  }

  sendRpc("evaluate_url", {
    url: details.url,
    now: new Date().toISOString(),
  })
    .then((result) => {
      if (!isCurrentNavigation(details.tabId, token)) {
        return;
      }

      if (isBlockDecision(result)) {
        redirectToBlocked(details, blockReasonFromResult(result));
        return;
      }

      void startVisitForNavigation(details, token);
    })
    .catch((error) => {
      markBackendUnhealthy(`URL evaluation failed: ${errorMessage(error)}`);
      if (isCurrentNavigation(details.tabId, token)) {
        redirectToBlocked(
          details,
          backendBlockReason("backend_unavailable", "BlocKuntu daemon did not evaluate the navigation")
        );
      }
    });
}

function shouldHandleNavigation(details: BlockuntuChromeExtension.NavigationDetails): boolean {
  return (
    details.frameId === 0 &&
    details.tabId >= 0 &&
    !isOwnBlockedPage(details.url) &&
    !isExtensionUrl(details.url)
  );
}

function redirectToBlocked(
  details: BlockuntuChromeExtension.NavigationDetails,
  reason: BlockNavigationReason
): void {
  if (details.frameId !== 0 || details.tabId < 0 || isOwnBlockedPage(details.url)) {
    return;
  }

  const redirectUrl = new URL(BLOCKED_PAGE_URL);
  redirectUrl.searchParams.set("url", details.url);
  redirectUrl.searchParams.set("reason", reason.kind);
  redirectUrl.searchParams.set("reason_json", JSON.stringify(reason));

  setOptionalSearchParam(redirectUrl, "tier", reason.tier);
  setOptionalSearchParam(redirectUrl, "rule_id", reason.rule_id);
  setOptionalSearchParam(redirectUrl, "rule_name", reason.rule_name);
  setOptionalSearchParam(redirectUrl, "controlled_reason", reason.controlled_reason);
  setOptionalSearchParam(redirectUrl, "blocked_by", reason.blocked_by);
  setOptionalSearchParam(redirectUrl, "summary", reason.summary);
  setOptionalSearchParam(redirectUrl, "detail", reason.detail);
  setOptionalSearchParam(redirectUrl, "free_at", reason.free_at);
  setOptionalSearchParam(redirectUrl, "allowance_reset_at", reason.allowance_reset_at);
  setOptionalSearchParam(redirectUrl, "last_heartbeat_ok_at", reason.last_heartbeat_ok_at);
  setOptionalSearchParam(redirectUrl, "message", reason.message);

  chrome.tabs.update(details.tabId, { url: redirectUrl.toString() }, () => {
    const error = chrome.runtime.lastError;
    if (error?.message) {
      console.error(`BlocKuntu failed to redirect tab ${details.tabId}: ${error.message}`);
    }
  });
}

function setOptionalSearchParam(url: URL, key: string, value: unknown): void {
  if (typeof value === "string" && value.length > 0) {
    url.searchParams.set(key, value);
  }
}

function startVisitForNavigation(
  details: BlockuntuChromeExtension.NavigationDetails,
  token: number
): Promise<void> {
  return sendRpc("record_visit_start", {
    url: details.url,
    tab_id: `chrome:${details.tabId}`,
    now: new Date().toISOString(),
  })
    .then((result) => {
      if (!isCurrentNavigation(details.tabId, token)) {
        return;
      }

      const visitId = visitIdFromResult(result);
      if (visitId === null) {
        throw new Error("record_visit_start response did not include an id");
      }

      activeVisits.set(details.tabId, {
        visitId,
        tabId: details.tabId,
        url: details.url,
      });
    })
    .catch((error) => {
      markBackendUnhealthy(`visit start failed: ${errorMessage(error)}`);
      if (isCurrentNavigation(details.tabId, token)) {
        redirectToBlocked(
          details,
          backendBlockReason("backend_unavailable", "BlocKuntu daemon did not record the visit")
        );
      }
    });
}

function endVisitForTab(tabId: number): Promise<void> {
  const visit = activeVisits.get(tabId);
  if (!visit) {
    return Promise.resolve();
  }

  activeVisits.delete(tabId);
  return sendRpc("record_visit_end", {
    visit_id: visit.visitId,
    now: new Date().toISOString(),
  })
    .then(() => undefined)
    .catch((error) => {
      markBackendUnhealthy(`visit end failed: ${errorMessage(error)}`);
    });
}

function heartbeatActiveVisits(): void {
  if (!refreshHealthState()) {
    return;
  }

  for (const visit of activeVisits.values()) {
    sendRpc("record_visit_heartbeat", {
      visit_id: visit.visitId,
      now: new Date().toISOString(),
    }).catch((error) => {
      markBackendUnhealthy(`visit heartbeat failed: ${errorMessage(error)}`);
    });
  }
}

function nextNavigationToken(tabId: number): number {
  const token = (navigationTokens.get(tabId) ?? 0) + 1;
  navigationTokens.set(tabId, token);
  return token;
}

function isCurrentNavigation(tabId: number, token: number): boolean {
  return navigationTokens.get(tabId) === token;
}

function isBlockDecision(result: unknown): boolean {
  return isObject(result) && result.decision === "block";
}

function blockReasonFromResult(result: unknown): BlockNavigationReason {
  if (!isObject(result) || !isObject(result.reason)) {
    return { kind: "blocked" };
  }

  const reason = result.reason;
  return {
    ...reason,
    kind: stringField(reason, "kind") ?? "blocked",
    tier: stringField(reason, "tier") ?? undefined,
    rule_id: stringField(reason, "rule_id") ?? undefined,
    rule_name: stringField(reason, "rule_name") ?? undefined,
    controlled_reason: stringField(reason, "controlled_reason") ?? undefined,
    blocked_by: stringField(reason, "blocked_by") ?? undefined,
    summary: stringField(reason, "summary") ?? undefined,
    detail: stringField(reason, "detail") ?? undefined,
    free_at: stringField(reason, "free_at") ?? undefined,
    allowance_reset_at: stringField(reason, "allowance_reset_at") ?? undefined,
    message: stringField(reason, "message") ?? undefined,
  };
}

function backendBlockReason(
  kind: "backend_unhealthy" | "backend_unavailable",
  message: string
): BlockNavigationReason {
  const reason: BlockNavigationReason = {
    kind,
    message,
    summary: "BlocKuntu cannot verify the daemon heartbeat.",
    detail:
      "Browsing is blocked fail-closed until the Chrome extension, native host, and daemon heartbeat chain is healthy again.",
    heartbeat_timeout_seconds: HEARTBEAT_TIMEOUT_MS / 1000,
  };

  if (lastHeartbeatOkAt > 0) {
    reason.last_heartbeat_ok_at = new Date(lastHeartbeatOkAt).toISOString();
  }

  return reason;
}

function visitIdFromResult(result: unknown): number | null {
  if (!isObject(result) || typeof result.id !== "number" || !Number.isSafeInteger(result.id)) {
    return null;
  }
  return result.id;
}

function isWebUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

function isOwnBlockedPage(url: string): boolean {
  return url.startsWith(BLOCKED_PAGE_URL);
}

function isExtensionUrl(value: string): boolean {
  try {
    return new URL(value).protocol === "chrome-extension:";
  } catch {
    return false;
  }
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringField(value: JsonObject, key: string): string | null {
  const field = value[key];
  return typeof field === "string" ? field : null;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

chrome.webNavigation.onBeforeNavigate.addListener(handleBeforeNavigate);
chrome.tabs.onRemoved.addListener((tabId: number) => {
  navigationTokens.delete(tabId);
  void endVisitForTab(tabId);
});
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === HEARTBEAT_ALARM) {
    sendHeartbeat();
  }
  if (alarm.name === VISIT_ALARM) {
    heartbeatActiveVisits();
  }
});
chrome.alarms.create(HEARTBEAT_ALARM, { periodInMinutes: 1 });
chrome.alarms.create(VISIT_ALARM, { periodInMinutes: 1 });

sendHeartbeat();
globalThis.setInterval(sendHeartbeat, HEARTBEAT_INTERVAL_MS);
globalThis.setInterval(refreshHealthState, 1_000);
globalThis.setInterval(heartbeatActiveVisits, VISIT_HEARTBEAT_INTERVAL_MS);
