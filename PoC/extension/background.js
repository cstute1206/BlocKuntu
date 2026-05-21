"use strict";

const NATIVE_HOST = "blockuntu_native";
const BLOCKED_PAGE_URL = browser.runtime.getURL("blocked.html");
const HEARTBEAT_INTERVAL_MS = 15_000;

let nativePort = null;
const pendingNavigations = [];

function connectNativeHost() {
  if (nativePort) {
    return nativePort;
  }

  nativePort = browser.runtime.connectNative(NATIVE_HOST);

  nativePort.onMessage.addListener((response) => {
    handleNativeResponse(response);
  });

  nativePort.onDisconnect.addListener(() => {
    const error = browser.runtime.lastError;
    if (error) {
      console.error(`BlocKuntu native host disconnected: ${error.message}`);
    } else {
      console.warn("BlocKuntu native host disconnected.");
    }

    nativePort = null;
    pendingNavigations.length = 0;
  });

  return nativePort;
}

function handleNativeResponse(response) {
  if (response && response.type === "extension_heartbeat") {
    return;
  }

  const navigation = pendingNavigations.shift();
  if (!navigation) {
    console.warn("BlocKuntu received a native response without a pending navigation.");
    return;
  }

  if (!response || typeof response.action !== "string") {
    console.error("BlocKuntu received an invalid native response.", response);
    return;
  }

  if (response.error) {
    console.warn(`BlocKuntu native response carried an error: ${response.error}`);
  }

  if (response.action !== "block") {
    return;
  }

  if (navigation.frameId !== 0 || navigation.tabId < 0) {
    console.warn(`BlocKuntu blocked a non-top-level navigation: ${navigation.url}`);
    return;
  }

  const redirectUrl = `${BLOCKED_PAGE_URL}?url=${encodeURIComponent(navigation.url)}`;
  browser.tabs.update(navigation.tabId, { url: redirectUrl }).catch((error) => {
    console.error(`BlocKuntu failed to redirect tab ${navigation.tabId}: ${error.message}`);
  });
}

function isOwnBlockedPage(url) {
  return typeof url === "string" && url.startsWith(BLOCKED_PAGE_URL);
}

function evaluateNavigation(details) {
  if (!details.url || isOwnBlockedPage(details.url)) {
    return;
  }

  const navigation = {
    tabId: details.tabId,
    frameId: details.frameId,
    url: details.url,
  };

  let queued = false;

  try {
    const port = connectNativeHost();
    pendingNavigations.push(navigation);
    queued = true;
    port.postMessage({ url: details.url });
  } catch (error) {
    if (queued) {
      pendingNavigations.pop();
    }

    console.error(`BlocKuntu failed to post native message: ${error.message}`);
  }
}

function sendHeartbeat() {
  try {
    const port = connectNativeHost();
    port.postMessage({
      type: "extension_heartbeat",
      extensionId: browser.runtime.id,
      extensionVersion: browser.runtime.getManifest().version,
      sentAt: Date.now(),
    });
  } catch (error) {
    console.error(`BlocKuntu failed to post heartbeat: ${error.message}`);
  }
}

browser.webNavigation.onBeforeNavigate.addListener(evaluateNavigation);
sendHeartbeat();
setInterval(sendHeartbeat, HEARTBEAT_INTERVAL_MS);
