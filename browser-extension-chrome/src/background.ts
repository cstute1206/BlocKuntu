"use strict";

const NATIVE_HOST_NAME = "blockuntu_native";
const BLOCKED_PAGE_URL = chrome.runtime.getURL("blocked.html");
const HEARTBEAT_INTERVAL_MS = 5_000;
const HEARTBEAT_TIMEOUT_MS = 75_000;
const RPC_TIMEOUT_MS = 3_000;
const VISIT_HEARTBEAT_INTERVAL_MS = 10_000;
const REVALIDATE_TABS_INTERVAL_MS = 5_000;
const EXISTING_TAB_FAILURE_THRESHOLD = 3;
const EXISTING_TAB_FAILURE_GRACE_MS = 10_000;
const RPC_SLOW_THRESHOLD_MS = 500;
const NATIVE_RECONNECT_INITIAL_MS = 1_000;
const NATIVE_RECONNECT_MAX_MS = 30_000;
const ALARM_PERIOD_MINUTES = 0.5;
const HEARTBEAT_ALARM = "blockuntu-heartbeat";
const VISIT_ALARM = "blockuntu-visit-heartbeat";
const REVALIDATE_ALARM = "blockuntu-revalidate-tabs";
const INTEGRATION_DISABLED_STORAGE_KEY = "blockuntuIntegrationDisabled";
const DIAGNOSTIC_QUEUE_STORAGE_KEY = "blockuntuDiagnosticQueue";
const MAX_DIAGNOSTIC_QUEUE_LENGTH = 200;
const DIAGNOSTIC_BATCH_SIZE = 50;

type JsonObject = Record<string, unknown>;
type ChromiumBrowserName = "chrome" | "chromium" | "brave" | "opera" | "edge" | "vivaldi";

interface BraveNavigator {
  brave?: {
    isBrave?: () => Promise<boolean> | boolean;
  };
}

interface PendingRequest {
  method: string;
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timeoutId: number;
  startedAt: number;
  timeoutMs: number;
}

interface ConsecutiveFailure {
  count: number;
  firstFailedAt: number;
  lastFailedAt: number;
  lastReason: string;
}

interface DiagnosticEvent extends JsonObject {
  id: string;
  component: string;
  severity: "info" | "warn" | "error";
  kind: string;
  message: string;
  observed_at: string;
  request_id?: string;
  method?: string;
  browser_session?: string;
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
  consecutive_failures?: number;
  failure_started_at?: string;
  last_transport_error?: string;
  message?: string;
}

let nativePort: BlockuntuChromeExtension.RuntimePort | null = null;
let nextRequestId = 1;
let heartbeatPromise: Promise<boolean> | null = null;
let lastHeartbeatOkAt = 0;
let heartbeatFailures: ConsecutiveFailure | null = null;
let nativeReconnectTimerId: number | null = null;
let nativeReconnectDelayMs = NATIVE_RECONNECT_INITIAL_MS;
let blockingDisabled = false;
let browserIdentityPromise: Promise<ChromiumBrowserName> | null = null;
let diagnosticQueue: DiagnosticEvent[] = [];
let diagnosticSequence = 1;
let diagnosticFlushPromise: Promise<void> | null = null;
const extensionSessionId = `chromium-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;

const pendingRequests = new Map<number, PendingRequest>();
const activeVisits = new Map<number, ActiveVisit>();
const pendingVisitStarts = new Map<number, string>();
const navigationTokens = new Map<number, number>();
const revalidationFailures = new Map<number, ConsecutiveFailure>();
const revalidationsInFlight = new Set<number>();
const settingsLoaded = loadStoredBlockingMode();
const diagnosticQueueLoaded = loadDiagnosticQueue();

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
    console.warn(
      `BlocKuntu native host disconnected; pending=${pendingRequests.size}; reason=${message}`
    );
    recordExtensionDiagnostic("warn", "native_host_disconnected", message);
    rejectAllPending(message);
    scheduleNativeReconnect();
  });

  nativePort = port;
  return port;
}

function handleNativeMessage(message: unknown): void {
  if (!isObject(message)) {
    recordExtensionDiagnostic(
      "error",
      "native_response_invalid",
      "native host returned a non-object response"
    );
    rejectAllPending("native host returned a non-object response");
    return;
  }

  const id = message.id;
  if (typeof id === "number") {
    const pending = pendingRequests.get(id);
    if (!pending) {
      console.warn(`BlocKuntu received a response for unknown request ${id}.`);
      recordExtensionDiagnostic(
        "warn",
        "rpc_response_unknown",
        `received a response for unknown request id=${id}`,
        String(id)
      );
      return;
    }

    pendingRequests.delete(id);
    globalThis.clearTimeout(pending.timeoutId);
    const elapsedMs = Math.round(performance.now() - pending.startedAt);
    resetNativeReconnectBackoff();

    if (isObject(message.error)) {
      const reason = JSON.stringify(message.error);
      console.warn(
        `BlocKuntu RPC failed id=${id} method=${pending.method} elapsed_ms=${elapsedMs} error=${reason}`
      );
      const nativeDiagnostic = message.error.blockuntu_diagnostic;
      if (isObject(nativeDiagnostic)) {
        queueImportedDiagnostic(nativeDiagnostic);
      }
      recordRpcDiagnostic("error", "rpc_failed", pending.method, reason, String(id));
      pending.reject(new Error(reason));
    } else {
      if (elapsedMs >= RPC_SLOW_THRESHOLD_MS) {
        const detail = `id=${id} method=${pending.method} elapsed_ms=${elapsedMs}`;
        console.warn(`BlocKuntu RPC slow ${detail}`);
        recordRpcDiagnostic("warn", "rpc_slow", pending.method, detail, String(id));
      }
      pending.resolve(message.result);
      if (pending.method !== "record_diagnostics") {
        void flushDiagnostics();
      }
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
      console.warn(`BlocKuntu RPC connect failed method=${method} error=${message}`);
      recordRpcDiagnostic("error", "rpc_connect_failed", method, message);
      reject(new Error(message));
      return;
    }

    const id = nextRequestId++;
    if (nextRequestId > Number.MAX_SAFE_INTEGER - 1) {
      nextRequestId = 1;
    }

    const startedAt = performance.now();
    const timeoutId = globalThis.setTimeout(() => {
      pendingRequests.delete(id);
      const elapsedMs = Math.round(performance.now() - startedAt);
      const message = `${method} timed out after ${timeoutMs}ms`;
      console.warn(
        `BlocKuntu RPC timeout id=${id} method=${method} elapsed_ms=${elapsedMs} pending=${pendingRequests.size}`
      );
      recordRpcDiagnostic(
        "error",
        "rpc_timeout",
        method,
        `elapsed_ms=${elapsedMs} pending=${pendingRequests.size}`,
        String(id)
      );
      reject(new Error(message));
    }, timeoutMs);

    pendingRequests.set(id, {
      method,
      resolve,
      reject,
      timeoutId,
      startedAt,
      timeoutMs,
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
      console.warn(`BlocKuntu RPC send failed id=${id} method=${method} error=${message}`);
      recordRpcDiagnostic("error", "rpc_send_failed", method, message, String(id));
      reject(new Error(message));
    }
  });
}

function rejectAllPending(reason: string): void {
  for (const [id, pending] of pendingRequests) {
    globalThis.clearTimeout(pending.timeoutId);
    const elapsedMs = Math.round(performance.now() - pending.startedAt);
    console.warn(
      `BlocKuntu RPC abandoned id=${id} method=${pending.method} elapsed_ms=${elapsedMs} timeout_ms=${pending.timeoutMs} reason=${reason}`
    );
    recordRpcDiagnostic(
      "error",
      "rpc_abandoned",
      pending.method,
      `elapsed_ms=${elapsedMs} timeout_ms=${pending.timeoutMs} reason=${reason}`,
      String(id)
    );
    pending.reject(new Error(`${pending.method} failed: ${reason}`));
    pendingRequests.delete(id);
  }
}

function scheduleNativeReconnect(): void {
  if (blockingDisabled || nativeReconnectTimerId !== null) {
    return;
  }

  const delayMs = nativeReconnectDelayMs;
  nativeReconnectDelayMs = Math.min(nativeReconnectDelayMs * 2, NATIVE_RECONNECT_MAX_MS);
  nativeReconnectTimerId = globalThis.setTimeout(() => {
    nativeReconnectTimerId = null;
    void sendHeartbeat();
  }, delayMs);
}

function resetNativeReconnectBackoff(): void {
  nativeReconnectDelayMs = NATIVE_RECONNECT_INITIAL_MS;
  if (nativeReconnectTimerId !== null) {
    globalThis.clearTimeout(nativeReconnectTimerId);
    nativeReconnectTimerId = null;
  }
}

function browserIdentity(): Promise<ChromiumBrowserName> {
  if (!browserIdentityPromise) {
    browserIdentityPromise = detectBrowserIdentity();
  }
  return browserIdentityPromise;
}

async function detectBrowserIdentity(): Promise<ChromiumBrowserName> {
  const brave = (navigator as Navigator & BraveNavigator).brave;
  if (typeof brave?.isBrave === "function") {
    try {
      if (await brave.isBrave()) {
        return "brave";
      }
    } catch {
      // Continue with the user agent if Brave's optional detection API fails.
    }
  }

  const userAgent = navigator.userAgent;
  if (/\bEdg\//i.test(userAgent)) {
    return "edge";
  }
  if (/\bOPR\//i.test(userAgent)) {
    return "opera";
  }
  if (/\bVivaldi\//i.test(userAgent)) {
    return "vivaldi";
  }
  if (/\bChromium\//i.test(userAgent)) {
    return "chromium";
  }
  return "chrome";
}

function extensionComponent(browser: ChromiumBrowserName): string {
  return `${browser}_extension`;
}

function sendHeartbeat(): Promise<boolean> {
  refreshHealthState();
  if (heartbeatPromise) {
    return heartbeatPromise;
  }

  heartbeatPromise = browserIdentity()
    .then((browser) =>
      sendRpc(
        "extension_heartbeat",
        {
          browser,
          component: extensionComponent(browser),
          extension_id: chrome.runtime.id,
          extension_version: chrome.runtime.getManifest().version,
          now: new Date().toISOString(),
        },
        Math.min(RPC_TIMEOUT_MS, HEARTBEAT_INTERVAL_MS)
      )
    )
    .then((result) => {
      applyHeartbeatResult(result);
      lastHeartbeatOkAt = Date.now();
      if (heartbeatFailures) {
        const message = `heartbeat recovered after ${heartbeatFailures.count} failure(s); outage_ms=${Date.now() - heartbeatFailures.firstFailedAt}`;
        console.info(`BlocKuntu ${message}`);
        recordExtensionDiagnostic("info", "heartbeat_recovered", message, undefined, "extension_heartbeat");
      }
      heartbeatFailures = null;
      resetNativeReconnectBackoff();
      void flushDiagnostics();
      return true;
    })
    .catch((error) => {
      recordHeartbeatFailure(`heartbeat failed: ${errorMessage(error)}`);
      if (nativePort === null) {
        scheduleNativeReconnect();
      }
      return false;
    })
    .finally(() => {
      heartbeatPromise = null;
    });

  return heartbeatPromise;
}

function refreshHealthState(): boolean {
  if (blockingDisabled) {
    return true;
  }

  return lastHeartbeatOkAt > 0 && Date.now() - lastHeartbeatOkAt <= HEARTBEAT_TIMEOUT_MS;
}

function recordHeartbeatFailure(reason: string): void {
  const now = Date.now();
  heartbeatFailures = nextFailure(heartbeatFailures, reason, now);
  const heartbeatAgeMs = lastHeartbeatOkAt > 0 ? now - lastHeartbeatOkAt : null;
  console.warn(
    `BlocKuntu heartbeat failed count=${heartbeatFailures.count} outage_ms=${now - heartbeatFailures.firstFailedAt} last_ok_age_ms=${heartbeatAgeMs ?? "never"} reason=${reason}`
  );
  recordExtensionDiagnostic(
    "error",
    "heartbeat_failed",
    `count=${heartbeatFailures.count} outage_ms=${now - heartbeatFailures.firstFailedAt} last_ok_age_ms=${heartbeatAgeMs ?? "never"} reason=${reason}`,
    undefined,
    "extension_heartbeat"
  );
}

function recordTransportFailure(method: string, reason: string): void {
  console.warn(
    `BlocKuntu transport request failed method=${method} heartbeat_fresh=${refreshHealthState()} reason=${reason}`
  );
  recordRpcDiagnostic(
    "error",
    "transport_request_failed",
    method,
    `heartbeat_fresh=${refreshHealthState()} reason=${reason}`
  );
}

function handleBeforeNavigate(details: BlockuntuChromeExtension.NavigationDetails): void {
  if (!shouldHandleNavigation(details)) {
    return;
  }

  void handleNavigation(details);
}

async function handleNavigation(details: BlockuntuChromeExtension.NavigationDetails): Promise<void> {
  const token = nextNavigationToken(details.tabId);
  revalidationFailures.delete(details.tabId);

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

      revalidationFailures.delete(details.tabId);

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
      recordTransportFailure("evaluate_url", errorMessage(error));
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
      recordTransportFailure("record_visit_start", errorMessage(error));
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
      recordTransportFailure("record_visit_end", errorMessage(error));
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
      recordTransportFailure("record_visit_heartbeat", errorMessage(error));
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
    revalidationFailures.clear();
    revalidationsInFlight.clear();
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

function recordRpcDiagnostic(
  severity: DiagnosticEvent["severity"],
  kind: string,
  method: string,
  message: string,
  requestId?: string
): void {
  if (method === "record_diagnostics") {
    return;
  }
  recordExtensionDiagnostic(severity, kind, message, requestId, method);
}

function recordExtensionDiagnostic(
  severity: DiagnosticEvent["severity"],
  kind: string,
  message: string,
  requestId?: string,
  method?: string
): void {
  void browserIdentity().then((browser) =>
    queueDiagnostic({
      id: `extension:${extensionSessionId}:${diagnosticSequence++}`,
      component: extensionComponent(browser),
      severity,
      kind,
      message: message.slice(0, 4_000),
      observed_at: new Date().toISOString(),
      request_id: requestId,
      method,
      browser_session: extensionSessionId,
    })
  );
}

function queueImportedDiagnostic(value: JsonObject): void {
  const id = stringField(value, "id");
  const component = stringField(value, "component");
  const severity = stringField(value, "severity");
  const kind = stringField(value, "kind");
  const message = stringField(value, "message");
  if (
    !id ||
    !component ||
    !kind ||
    !message ||
    (severity !== "info" && severity !== "warn" && severity !== "error")
  ) {
    return;
  }
  queueDiagnostic({
    id,
    component,
    severity,
    kind,
    message: message.slice(0, 4_000),
    observed_at: stringField(value, "observed_at") ?? new Date().toISOString(),
    request_id: stringField(value, "request_id") ?? undefined,
    method: stringField(value, "method") ?? undefined,
    browser_session: stringField(value, "browser_session") ?? extensionSessionId,
  });
}

function queueDiagnostic(event: DiagnosticEvent): void {
  void diagnosticQueueLoaded.then(async () => {
    if (diagnosticQueue.some((queued) => queued.id === event.id)) {
      return;
    }
    diagnosticQueue.push(event);
    if (diagnosticQueue.length > MAX_DIAGNOSTIC_QUEUE_LENGTH) {
      diagnosticQueue.splice(0, diagnosticQueue.length - MAX_DIAGNOSTIC_QUEUE_LENGTH);
    }
    await persistDiagnosticQueue();
  });
}

function loadDiagnosticQueue(): Promise<void> {
  return new Promise((resolve) => {
    chrome.storage.local.get([DIAGNOSTIC_QUEUE_STORAGE_KEY], (items) => {
      const error = chrome.runtime.lastError;
      if (error?.message) {
        console.warn(`BlocKuntu failed to load diagnostic queue: ${error.message}`);
      } else {
        const stored = items[DIAGNOSTIC_QUEUE_STORAGE_KEY];
        if (Array.isArray(stored)) {
          diagnosticQueue = stored
            .filter(isDiagnosticEvent)
            .slice(-MAX_DIAGNOSTIC_QUEUE_LENGTH);
        }
      }
      resolve();
    });
  });
}

function persistDiagnosticQueue(): Promise<void> {
  return new Promise((resolve) => {
    chrome.storage.local.set({ [DIAGNOSTIC_QUEUE_STORAGE_KEY]: diagnosticQueue }, () => {
      const error = chrome.runtime.lastError;
      if (error?.message) {
        console.warn(`BlocKuntu failed to persist diagnostic queue: ${error.message}`);
      }
      resolve();
    });
  });
}

async function flushDiagnostics(): Promise<void> {
  await diagnosticQueueLoaded;
  if (diagnosticFlushPromise) {
    return diagnosticFlushPromise;
  }
  if (diagnosticQueue.length === 0 || blockingDisabled) {
    return;
  }

  diagnosticFlushPromise = (async () => {
    while (diagnosticQueue.length > 0) {
      const batch = diagnosticQueue.slice(0, DIAGNOSTIC_BATCH_SIZE);
      await sendRpc("record_diagnostics", { events: batch });
      const acceptedIds = new Set(batch.map((event) => event.id));
      diagnosticQueue = diagnosticQueue.filter((event) => !acceptedIds.has(event.id));
      await persistDiagnosticQueue();
    }
  })()
    .catch((error: unknown) => {
      console.warn(`BlocKuntu diagnostic flush failed: ${errorMessage(error)}`);
    })
    .finally(() => {
      diagnosticFlushPromise = null;
    });
  return diagnosticFlushPromise;
}

function isDiagnosticEvent(value: unknown): value is DiagnosticEvent {
  if (!isObject(value)) {
    return false;
  }
  const severity = stringField(value, "severity");
  return Boolean(
    stringField(value, "id") &&
      stringField(value, "component") &&
      stringField(value, "kind") &&
      stringField(value, "message") &&
      stringField(value, "observed_at") &&
      (severity === "info" || severity === "warn" || severity === "error")
  );
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
  if (revalidationsInFlight.has(tabId)) {
    return;
  }

  revalidationsInFlight.add(tabId);
  try {
    await revalidateTabOnce(tabId, url);
  } finally {
    revalidationsInFlight.delete(tabId);
  }
}

async function revalidateTabOnce(tabId: number, url: string): Promise<void> {
  const details = { tabId, frameId: 0, url };
  const backendReady = await ensureBackendReady();
  if (blockingDisabled) {
    void endVisitForTab(tabId);
    return;
  }

  if (!backendReady) {
    handleExistingTabFailure(
      details,
      "backend_unhealthy",
      "BlocKuntu daemon heartbeat is not healthy",
      heartbeatFailures?.lastReason ?? "heartbeat is missing or stale"
    );
    return;
  }

  try {
    const result = await sendRpc("evaluate_url", {
      url,
      now: new Date().toISOString(),
    });
    revalidationFailures.delete(tabId);

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
        recordTransportFailure("record_visit_start", errorMessage(error));
      });
    } else if (activeVisit.url !== url) {
      void endVisitForTab(tabId).then(() =>
        startVisitForTab(tabId, url).catch((error) => {
          recordTransportFailure("record_visit_start", errorMessage(error));
        })
      );
    }
  } catch (error) {
    const reason = errorMessage(error);
    recordTransportFailure("evaluate_url", reason);
    handleExistingTabFailure(
      details,
      "backend_unavailable",
      "BlocKuntu daemon did not revalidate the tab",
      reason
    );
  }
}

function handleExistingTabFailure(
  details: BlockuntuChromeExtension.NavigationDetails,
  kind: "backend_unhealthy" | "backend_unavailable",
  message: string,
  reason: string
): void {
  const now = Date.now();
  const failure = nextFailure(revalidationFailures.get(details.tabId) ?? null, reason, now);
  revalidationFailures.set(details.tabId, failure);
  const elapsedMs = now - failure.firstFailedAt;
  const failClosed =
    failure.count >= EXISTING_TAB_FAILURE_THRESHOLD &&
    elapsedMs >= EXISTING_TAB_FAILURE_GRACE_MS;

  const diagnosticMessage = `tab_id=${details.tabId} count=${failure.count} elapsed_ms=${elapsedMs} action=${failClosed ? "redirect" : "retry"} reason=${reason}`;
  console.warn(`BlocKuntu existing-tab revalidation failed ${diagnosticMessage}`);
  recordExtensionDiagnostic(
    failClosed ? "error" : "warn",
    "existing_tab_revalidation_failed",
    diagnosticMessage,
    undefined,
    "evaluate_url"
  );

  if (!failClosed) {
    return;
  }

  activeVisits.delete(details.tabId);
  revalidationFailures.delete(details.tabId);
  const blockReason = backendBlockReason(kind, message);
  blockReason.consecutive_failures = failure.count;
  blockReason.failure_started_at = new Date(failure.firstFailedAt).toISOString();
  blockReason.last_transport_error = failure.lastReason;
  redirectToBlocked(details, blockReason);
}

function nextFailure(
  previous: ConsecutiveFailure | null,
  reason: string,
  now: number
): ConsecutiveFailure {
  return {
    count: (previous?.count ?? 0) + 1,
    firstFailedAt: previous?.firstFailedAt ?? now,
    lastFailedAt: now,
    lastReason: reason,
  };
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

function ensureLifecycleAlarm(name: string): void {
  chrome.alarms.get(name, (alarm) => {
    const error = chrome.runtime.lastError;
    if (error?.message) {
      console.warn(`BlocKuntu failed to inspect lifecycle alarm ${name}: ${error.message}`);
      return;
    }
    if (alarm) {
      return;
    }
    chrome.alarms.create(name, {
      delayInMinutes: ALARM_PERIOD_MINUTES,
      periodInMinutes: ALARM_PERIOD_MINUTES,
    });
  });
}

chrome.webNavigation.onBeforeNavigate.addListener(handleBeforeNavigate);
chrome.webNavigation.onHistoryStateUpdated.addListener(handleBeforeNavigate);
chrome.tabs.onRemoved.addListener((tabId: number) => {
  navigationTokens.delete(tabId);
  revalidationFailures.delete(tabId);
  revalidationsInFlight.delete(tabId);
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
    void sendHeartbeat();
  }
  if (alarm.name === VISIT_ALARM) {
    heartbeatActiveVisits();
  }
  if (alarm.name === REVALIDATE_ALARM) {
    revalidateOpenTabs();
  }
});
ensureLifecycleAlarm(HEARTBEAT_ALARM);
ensureLifecycleAlarm(VISIT_ALARM);
ensureLifecycleAlarm(REVALIDATE_ALARM);

void sendHeartbeat();
globalThis.setInterval(sendHeartbeat, HEARTBEAT_INTERVAL_MS);
globalThis.setInterval(refreshHealthState, 1_000);
globalThis.setInterval(heartbeatActiveVisits, VISIT_HEARTBEAT_INTERVAL_MS);
globalThis.setInterval(revalidateOpenTabs, REVALIDATE_TABS_INTERVAL_MS);
