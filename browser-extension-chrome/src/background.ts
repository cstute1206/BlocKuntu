"use strict";

const NATIVE_HOST_NAME = "blockuntu_native";
const EXTENSION_COMPONENT = "chrome_extension";
const BROWSER_NAME = "chrome";
const BLOCKED_PAGE_URL = chrome.runtime.getURL("blocked.html");
const HEARTBEAT_INTERVAL_MS = 5_000;
const HEARTBEAT_TIMEOUT_MS = 75_000;
const RPC_TIMEOUT_MS = 3_000;
const VISIT_HEARTBEAT_INTERVAL_MS = 10_000;
const REVALIDATE_TABS_INTERVAL_MS = 5_000;
const HEARTBEAT_ALARM = "blockuntu-heartbeat";
const VISIT_ALARM = "blockuntu-visit-heartbeat";
const REVALIDATE_ALARM = "blockuntu-revalidate-tabs";
const INTEGRATION_DISABLED_STORAGE_KEY = "blockuntuIntegrationDisabled";

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
let heartbeatPromise: Promise<boolean> | null = null;
let lastHeartbeatOkAt = 0;
let backendHealthy = false;
let blockingDisabled = false;

const pendingRequests = new Map<number, PendingRequest>();
const activeVisits = new Map<number, ActiveVisit>();
const pendingVisitStarts = new Map<number, string>();
const navigationTokens = new Map<number, number>();
const settingsLoaded = loadStoredBlockingMode();

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

function sendHeartbeat(): Promise<boolean> {
  refreshHealthState();
  if (heartbeatPromise) {
    return heartbeatPromise;
  }

  heartbeatInFlight = true;
  heartbeatPromise = sendRpc(
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
    .then((result) => {
      applyHeartbeatResult(result);
      lastHeartbeatOkAt = Date.now();
      backendHealthy = true;
      return true;
    })
    .catch((error) => {
      markBackendUnhealthy(`heartbeat failed: ${errorMessage(error)}`);
      return false;
    })
    .finally(() => {
      heartbeatInFlight = false;
      heartbeatPromise = null;
    });

  return heartbeatPromise;
}

function refreshHealthState(): boolean {
  if (blockingDisabled) {
    return true;
  }

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

  void handleNavigation(details);
}

async function handleNavigation(details: BlockuntuChromeExtension.NavigationDetails): Promise<void> {
  const token = nextNavigationToken(details.tabId);

  await settingsLoaded;
  if (blockingDisabled) {
    void endVisitForTab(details.tabId);
    return;
  }

  const backendReady = await ensureBackendReady();
  if (blockingDisabled) {
    void endVisitForTab(details.tabId);
    return;
  }

  if (!backendReady) {
    activeVisits.delete(details.tabId);
    redirectToBlocked(
      details,
      backendBlockReason("backend_unhealthy", "BlocKuntu daemon heartbeat is not healthy")
    );
    return;
  }

  void endVisitForTab(details.tabId);

  sendRpc("evaluate_url", {
    url: details.url,
    now: new Date().toISOString(),
  })
    .then(async (result) => {
      if (!isCurrentNavigation(details.tabId, token)) {
        return;
      }

      if (isBlockDecision(result)) {
        redirectToBlocked(details, blockReasonFromResult(result));
        return;
      }

      if (!isMeteringActive(result)) {
        void endVisitForTab(details.tabId);
        return;
      }

      if (!(await isActiveTab(details.tabId)) || !isCurrentNavigation(details.tabId, token)) {
        void endVisitForTab(details.tabId);
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
    isWebUrl(details.url) &&
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
  return startVisitForTab(details.tabId, details.url)
    .then(() => {
      if (!isCurrentNavigation(details.tabId, token)) {
        void endVisitForTab(details.tabId);
      }
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

function startVisitForTab(tabId: number, url: string): Promise<void> {
  const activeVisit = activeVisits.get(tabId);
  if (activeVisit?.url === url || pendingVisitStarts.get(tabId) === url) {
    return Promise.resolve();
  }

  pendingVisitStarts.set(tabId, url);
  return sendRpc("record_visit_start", {
    url,
    tab_id: `chrome:${tabId}`,
    now: new Date().toISOString(),
  })
    .then((result) => {
      if (pendingVisitStarts.get(tabId) !== url) {
        return;
      }

      const visitId = visitIdFromResult(result);
      if (visitId === null) {
        throw new Error("record_visit_start response did not include an id");
      }

      activeVisits.set(tabId, {
        visitId,
        tabId,
        url,
      });
    })
    .finally(() => {
      if (pendingVisitStarts.get(tabId) === url) {
        pendingVisitStarts.delete(tabId);
      }
    });
}

function endVisitForTab(tabId: number): Promise<void> {
  pendingVisitStarts.delete(tabId);
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
  void heartbeatActiveVisitsAsync();
}

async function heartbeatActiveVisitsAsync(): Promise<void> {
  if (blockingDisabled) {
    return;
  }

  if (!refreshHealthState()) {
    return;
  }

  const activeTabs = await activeTabIds();
  for (const visit of activeVisits.values()) {
    if (!activeTabs.has(visit.tabId)) {
      void endVisitForTab(visit.tabId);
      continue;
    }

    sendRpc("record_visit_heartbeat", {
      visit_id: visit.visitId,
      now: new Date().toISOString(),
    }).catch((error) => {
      markBackendUnhealthy(`visit heartbeat failed: ${errorMessage(error)}`);
    });
  }
}

function queryActiveTabs(): Promise<BlockuntuChromeExtension.Tab[]> {
  return new Promise((resolve) => {
    chrome.tabs.query({ active: true }, (tabs) => {
      const error = chrome.runtime.lastError;
      if (error?.message) {
        console.warn(`BlocKuntu failed to query active tabs: ${error.message}`);
        resolve([]);
        return;
      }
      resolve(tabs);
    });
  });
}

async function activeTabIds(): Promise<Set<number>> {
  const tabs = await queryActiveTabs();
  return new Set(tabs.flatMap((tab) => (typeof tab.id === "number" ? [tab.id] : [])));
}

async function isActiveTab(tabId: number): Promise<boolean> {
  return (await activeTabIds()).has(tabId);
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

function isMeteringActive(result: unknown): boolean {
  return isObject(result) && result.metering_active === true;
}

async function ensureBackendReady(): Promise<boolean> {
  await settingsLoaded;
  if (blockingDisabled) {
    return true;
  }
  if (refreshHealthState()) {
    return true;
  }
  return sendHeartbeat();
}

function applyHeartbeatResult(result: unknown): void {
  if (!isObject(result)) {
    return;
  }

  const extensionMode = stringField(result, "browser_extension_mode");

  if (extensionMode === "uninstalling") {
    setBlockingDisabled(true);
    return;
  }

  if (extensionMode === "active") {
    setBlockingDisabled(false);
  }
}

function setBlockingDisabled(disabled: boolean): void {
  if (blockingDisabled === disabled) {
    return;
  }

  blockingDisabled = disabled;
  chrome.storage.local.set({ [INTEGRATION_DISABLED_STORAGE_KEY]: disabled }, () => {
    const error = chrome.runtime.lastError;
    if (error?.message) {
      console.warn(`BlocKuntu failed to persist integration state: ${error.message}`);
    }
  });

  if (disabled) {
    for (const tabId of activeVisits.keys()) {
      void endVisitForTab(tabId);
    }
  }
}

function loadStoredBlockingMode(): Promise<void> {
  return new Promise((resolve) => {
    chrome.storage.local.get([INTEGRATION_DISABLED_STORAGE_KEY], (items) => {
      const error = chrome.runtime.lastError;
      if (error?.message) {
        console.warn(`BlocKuntu failed to load integration state: ${error.message}`);
      } else {
        blockingDisabled = items[INTEGRATION_DISABLED_STORAGE_KEY] === true;
      }
      resolve();
    });
  });
}

function queryWebTabs(): Promise<BlockuntuChromeExtension.Tab[]> {
  return new Promise((resolve) => {
    chrome.tabs.query({ url: ["http://*/*", "https://*/*"] }, (tabs) => {
      const error = chrome.runtime.lastError;
      if (error?.message) {
        console.warn(`BlocKuntu failed to query tabs for revalidation: ${error.message}`);
        resolve([]);
        return;
      }
      resolve(tabs);
    });
  });
}

function revalidateOpenTabs(): void {
  void revalidateOpenTabsAsync();
}

async function revalidateOpenTabsAsync(): Promise<void> {
  await settingsLoaded;
  if (blockingDisabled) {
    return;
  }

  const tabs = await queryWebTabs();
  for (const tab of tabs) {
    if (typeof tab.id !== "number" || typeof tab.url !== "string") {
      continue;
    }
    if (!tab.active) {
      void endVisitForTab(tab.id);
      continue;
    }
    void revalidateTab(tab.id, tab.url);
  }
}

async function revalidateTab(tabId: number, url: string): Promise<void> {
  if (!isWebUrl(url) || isOwnBlockedPage(url) || isExtensionUrl(url)) {
    return;
  }

  const details = { tabId, frameId: 0, url };
  const backendReady = await ensureBackendReady();
  if (blockingDisabled) {
    void endVisitForTab(tabId);
    return;
  }

  if (!backendReady) {
    activeVisits.delete(tabId);
    redirectToBlocked(
      details,
      backendBlockReason("backend_unhealthy", "BlocKuntu daemon heartbeat is not healthy")
    );
    return;
  }

  try {
    const result = await sendRpc("evaluate_url", {
      url,
      now: new Date().toISOString(),
    });

    if (isBlockDecision(result)) {
      void endVisitForTab(tabId);
      redirectToBlocked(details, blockReasonFromResult(result));
      return;
    }

    if (!isMeteringActive(result)) {
      void endVisitForTab(tabId);
      return;
    }

    const activeVisit = activeVisits.get(tabId);
    if (!activeVisit) {
      void startVisitForTab(tabId, url).catch((error) => {
        markBackendUnhealthy(`visit start failed: ${errorMessage(error)}`);
      });
    } else if (activeVisit.url !== url) {
      void endVisitForTab(tabId).then(() =>
        startVisitForTab(tabId, url).catch((error) => {
          markBackendUnhealthy(`visit start failed: ${errorMessage(error)}`);
        })
      );
    }
  } catch (error) {
    markBackendUnhealthy(`tab revalidation failed: ${errorMessage(error)}`);
    activeVisits.delete(tabId);
    redirectToBlocked(
      details,
      backendBlockReason("backend_unavailable", "BlocKuntu daemon did not revalidate the tab")
    );
  }
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
chrome.webNavigation.onHistoryStateUpdated.addListener(handleBeforeNavigate);
chrome.tabs.onRemoved.addListener((tabId: number) => {
  navigationTokens.delete(tabId);
  void endVisitForTab(tabId);
});
chrome.tabs.onActivated.addListener((activeInfo) => {
  for (const tabId of activeVisits.keys()) {
    if (tabId !== activeInfo.tabId) {
      void endVisitForTab(tabId);
    }
  }
  revalidateOpenTabs();
});
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === HEARTBEAT_ALARM) {
    sendHeartbeat();
  }
  if (alarm.name === VISIT_ALARM) {
    heartbeatActiveVisits();
  }
  if (alarm.name === REVALIDATE_ALARM) {
    revalidateOpenTabs();
  }
});
chrome.alarms.create(HEARTBEAT_ALARM, { periodInMinutes: 1 });
chrome.alarms.create(VISIT_ALARM, { periodInMinutes: 1 });
chrome.alarms.create(REVALIDATE_ALARM, { periodInMinutes: 1 });

sendHeartbeat();
globalThis.setInterval(sendHeartbeat, HEARTBEAT_INTERVAL_MS);
globalThis.setInterval(refreshHealthState, 1_000);
globalThis.setInterval(heartbeatActiveVisits, VISIT_HEARTBEAT_INTERVAL_MS);
globalThis.setInterval(revalidateOpenTabs, REVALIDATE_TABS_INTERVAL_MS);
