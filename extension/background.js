/**
 * Browser Controller — MV3 service worker.
 *
 * Connects to the native messaging host (browser-controller-mediator) and
 * handles commands forwarded by the mediator to control windows and tabs.
 *
 * Supports Firefox (121+) and Chrome/Chromium-based browsers. Firefox-only
 * features (title prefix, sessions API, containers, reader mode, tab warmup)
 * are guarded by `isFirefox` and return errors on Chrome.
 *
 * Protocol (mediator → extension):
 *   Length-prefixed JSON messages, each a CliRequest:
 *   { "request_id": "<uuid>", "type": "<CommandVariant>", ... }
 *
 * Protocol (extension → mediator):
 *   On connect:  { "message_type": "Hello", "browser_name": "...", "browser_vendor": "...", "browser_version": "..." }
 *   On event:    { "message_type": "Event", "event": { "type": "<EventVariant>", ... } }
 *   On response: { "message_type": "Response", "request_id": "<uuid>",
 *                  "outcome": { "status": "ok"|"err", "data": <CliResult>|<string> } }
 */

"use strict";

// Chrome uses the 'chrome' namespace; alias it as 'browser' so all code below
// works unchanged in both Firefox and Chrome.
if (typeof browser === "undefined") {
  globalThis.browser = chrome;
}

/**
 * True when running in Firefox; false for Chrome/Chromium-based browsers.
 * Used to gate Firefox-only API calls (titlePreface, sessions, warmup, etc.).
 */
const isFirefox = typeof browser.runtime.getBrowserInfo === "function";

/** Name registered in the native messaging host manifest. */
const NATIVE_HOST = "browser_controller";

/** Base reconnect delay in milliseconds. Doubles on each failure up to MAX_RECONNECT_DELAY. */
const INITIAL_RECONNECT_DELAY_MS = 1000;
const MAX_RECONNECT_DELAY_MS = 30_000;

/** Name of the Chrome alarm used to reconnect after service worker termination. */
const RECONNECT_ALARM_NAME = "browser-controller-reconnect";

/** Tabs currently awaiting basic HTTP authentication (tracked via webRequest). */
const pendingAuthTabs = new Set();

/**
 * Pending credentials for tabs opened with username/password.
 *
 * Maps `"origin\tcookieStoreId"` → `{ username, password }`.
 * Keyed by origin + cookie store ID so that tabs in different containers
 * opening the same origin with different credentials don't clash.
 * The `cookieStoreId` part is the empty string for the default container.
 *
 * Keyed by origin (rather than tab ID) because the 401 challenge may fire
 * during `tabs.create()` before the tab ID is known. The `onAuthRequired`
 * listener looks up the tab's cookie store ID and matches on the composite
 * key.
 */
const pendingCredentials = new Map();

/**
 * Build the pendingCredentials map key for an origin and cookie store ID.
 * @param {string} origin - URL origin (e.g. `"https://example.com"`)
 * @param {string|null} cookieStoreId - Container cookie store ID, or null/undefined for default.
 * @returns {string}
 */
function credentialKey(origin, cookieStoreId) {
  return `${origin}\t${cookieStoreId ?? ""}`;
}


/** Active port to the native messaging host (null when disconnected). */
let nativePort = null;

/** Current reconnect delay, grows with each failed attempt. */
let reconnectDelayMs = INITIAL_RECONNECT_DELAY_MS;

/**
 * The suffix Firefox appends to every window title, e.g. " — Firefox".
 * Populated once on first connection from browser.runtime.getBrowserInfo().
 * Used to anchor the titlePreface extraction in extractTitlePreface().
 */
let windowTitleSuffix = null;

// ---------------------------------------------------------------------------
// Auth tracking
// ---------------------------------------------------------------------------

// Re-apply any stored titlePreface whenever a window appears (covers both
// newly-created windows and windows restored by session restore).
// Firefox-only: Chrome does not support titlePreface or the sessions API.
browser.windows.onCreated.addListener(async (win) => {
  if (!isFirefox) return;
  const prefix = await browser.sessions.getWindowValue(win.id, "titlePreface");
  if (prefix !== undefined) {
    await browser.windows.update(win.id, { titlePreface: prefix });
  }
});

browser.webRequest.onAuthRequired.addListener(
  async (details) => {
    if (details.tabId >= 0) {
      pendingAuthTabs.add(details.tabId);
    }

    // Match pending credentials by the request URL's origin and the tab's
    // cookie store ID (container). This allows different credentials for
    // the same origin in different containers.
    // Do NOT delete the entry here — multiple requests to the same origin
    // (main page, favicon, subresources) may each trigger a 401 during the
    // initial page load. The entry is cleaned up by cmdOpenTab after the
    // tab finishes loading.
    try {
      const origin = new URL(details.url).origin;
      let storeId = "";
      if (details.tabId >= 0) {
        try {
          const tab = await browser.tabs.get(details.tabId);
          storeId = tab.cookieStoreId ?? "";
        } catch {
          // Tab may not exist yet or be inaccessible; use default key.
        }
      }
      const creds = pendingCredentials.get(credentialKey(origin, storeId))
        // Fall back to the "no container specified" key. This handles the
        // common case where OpenTab was called without --container, storing
        // with an empty cookieStoreId, but the browser assigns a default
        // store like "firefox-default".
        ?? pendingCredentials.get(credentialKey(origin, null));
      if (creds) {
        return { authCredentials: creds };
      }
    } catch {
      // URL parsing failed; ignore and fall through.
    }
    return {};
  },
  { urls: ["<all_urls>"] },
  ["blocking"],
);

browser.webRequest.onCompleted.addListener(
  (details) => {
    pendingAuthTabs.delete(details.tabId);
  },
  { urls: ["<all_urls>"] },
);

browser.webRequest.onErrorOccurred.addListener(
  (details) => {
    pendingAuthTabs.delete(details.tabId);
  },
  { urls: ["<all_urls>"] },
);

// ---------------------------------------------------------------------------
// Browser event forwarding
// ---------------------------------------------------------------------------

/**
 * Post a browser event message to the mediator if connected.
 *
 * @param {object} payload - Object with `type` and event-specific fields.
 */
function pushEvent(payload) {
  if (nativePort) {
    try {
      nativePort.postMessage({ message_type: "Event", event: payload });
    } catch (err) {
      console.error(`[browser-controller] Failed to push event ${payload.type}`, {
        error: String(err),
        stack: err?.stack,
        payload,
      });
    }
  }
}

browser.windows.onCreated.addListener((win) => {
  pushEvent({ type: "WindowOpened", window_id: win.id, title: win.title ?? "" });
});

browser.windows.onRemoved.addListener((windowId) => {
  pushEvent({ type: "WindowClosed", window_id: windowId });
});

browser.tabs.onActivated.addListener((activeInfo) => {
  pushEvent({
    type: "TabActivated",
    window_id: activeInfo.windowId,
    tab_id: activeInfo.tabId,
    previous_tab_id: activeInfo.previousTabId ?? null,
  });
});

browser.tabs.onCreated.addListener((tab) => {
  pushEvent({
    type: "TabOpened",
    tab_id: tab.id,
    window_id: tab.windowId,
    index: tab.index,
    url: tab.url ?? "",
    title: tab.title ?? "",
  });
});

browser.tabs.onRemoved.addListener((tabId, removeInfo) => {
  pendingAuthTabs.delete(tabId);
  pushEvent({
    type: "TabClosed",
    tab_id: tabId,
    window_id: removeInfo.windowId,
    is_window_closing: removeInfo.isWindowClosing,
  });
});

browser.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.url !== undefined) {
    pushEvent({
      type: "TabNavigated",
      tab_id: tabId,
      window_id: tab.windowId,
      url: changeInfo.url,
    });
  }
  if (changeInfo.title !== undefined) {
    pushEvent({
      type: "TabTitleChanged",
      tab_id: tabId,
      window_id: tab.windowId,
      title: changeInfo.title,
    });
  }
  if (changeInfo.status !== undefined) {
    pushEvent({
      type: "TabStatusChanged",
      tab_id: tabId,
      window_id: tab.windowId,
      status: changeInfo.status,
    });
  }
});

// ---------------------------------------------------------------------------
// Download event forwarding
// ---------------------------------------------------------------------------

browser.downloads.onCreated.addListener((downloadItem) => {
  pushEvent({
    type: "DownloadCreated",
    download_id: downloadItem.id,
    url: downloadItem.url ?? "",
    filename: downloadItem.filename ?? "",
    mime: downloadItem.mime || null,
  });
});

browser.downloads.onChanged.addListener((downloadDelta) => {
  const event = {
    type: "DownloadChanged",
    download_id: downloadDelta.id,
  };
  if (downloadDelta.state) {
    event.state = downloadDelta.state.current;
  }
  if (downloadDelta.filename) {
    event.filename = downloadDelta.filename.current;
  }
  if (downloadDelta.error) {
    event.error = downloadDelta.error.current;
  }
  pushEvent(event);
});

browser.downloads.onErased.addListener((downloadId) => {
  pushEvent({
    type: "DownloadErased",
    download_id: downloadId,
  });
});

// ---------------------------------------------------------------------------
// Native messaging connection
// ---------------------------------------------------------------------------

/**
 * Return browser identity information.
 *
 * In Firefox, delegates to `browser.runtime.getBrowserInfo()`.
 * In Chrome/Chromium-based browsers, parses the user-agent string since no
 * equivalent API exists.
 *
 * @returns {Promise<{name: string, vendor: string|null, version: string}>}
 */
async function fetchBrowserInfo() {
  if (isFirefox) {
    return browser.runtime.getBrowserInfo();
  }
  const ua = navigator.userAgent;
  const chromeVersion = ua.match(/Chrome\/([\d.]+)/)?.[1] ?? "unknown";
  // Brave deliberately mimics Chrome's UA string but exposes navigator.brave.
  if (navigator.brave && typeof navigator.brave.isBrave === "function") {
    return { name: "Brave", vendor: "Brave Software", version: chromeVersion };
  }
  if (ua.includes("Edg/")) {
    const edgeVersion = ua.match(/Edg\/([\d.]+)/)?.[1] ?? chromeVersion;
    return { name: "Edge", vendor: "Microsoft", version: edgeVersion };
  }
  if (ua.includes("OPR/")) {
    const operaVersion = ua.match(/OPR\/([\d.]+)/)?.[1] ?? chromeVersion;
    return { name: "Opera", vendor: "Opera Software", version: operaVersion };
  }
  if (ua.includes("Vivaldi/")) {
    const vivaldiVersion = ua.match(/Vivaldi\/([\d.]+)/)?.[1] ?? chromeVersion;
    return { name: "Vivaldi", vendor: "Vivaldi Technologies", version: vivaldiVersion };
  }
  return { name: "Chrome", vendor: "Google", version: chromeVersion };
}

/** Connect to the mediator and send the initial Hello message. */
function connect() {
  console.info(`[browser-controller] Connecting to native host: ${NATIVE_HOST}`);
  nativePort = browser.runtime.connectNative(NATIVE_HOST);

  // Send Hello immediately so the mediator knows what browser is connected.
  // Also construct an initial windowTitleSuffix from the browser name so that
  // extractTitlePreface() can anchor its search before any ListWindows call.
  // The suffix is derived from vendor + name (e.g. "Mozilla" + "Firefox" →
  // " — Mozilla Firefox"); empty parts are skipped to handle forks cleanly.
  fetchBrowserInfo().then((info) => {
    const brand = [info.vendor, info.name].filter(Boolean).join(" ");
    windowTitleSuffix = ` \u2014 ${brand}`;
    const hello = {
      message_type: "Hello",
      browser_name: info.name,
      browser_vendor: info.vendor || null,
      browser_version: info.version,
    };
    console.info(`[browser-controller] Sending Hello`, hello);
    nativePort.postMessage(hello);
  }).catch((err) => {
    console.error(`[browser-controller] Failed to get browser info for Hello`, {
      error: String(err),
      stack: err?.stack,
    });
  });

  nativePort.onMessage.addListener(handleNativeMessage);

  nativePort.onDisconnect.addListener(() => {
    nativePort = null;
    const err = browser.runtime.lastError;
    console.warn(
      `[browser-controller] Disconnected from mediator${err ? ": " + err.message : ""}. Reconnecting in ${reconnectDelayMs}ms.`,
    );
    if (!isFirefox && chrome.alarms) {
      // On Chrome, use chrome.alarms instead of setTimeout. Alarms persist
      // across service worker restarts, so if Chrome kills the worker before
      // the timeout fires, the alarm will wake it back up.
      const delayMinutes = Math.max(reconnectDelayMs / 60_000, 0.1);
      chrome.alarms.create(RECONNECT_ALARM_NAME, { delayInMinutes: delayMinutes });
    } else {
      // Firefox: setTimeout is fine — background scripts are persistent.
      setTimeout(() => {
        reconnectDelayMs = Math.min(reconnectDelayMs * 2, MAX_RECONNECT_DELAY_MS);
        connect();
      }, reconnectDelayMs);
    }
  });

  // Reset backoff on successful connection (assumed when first message arrives).
  reconnectDelayMs = INITIAL_RECONNECT_DELAY_MS;

  // Clear any pending reconnect alarm now that we're connected.
  if (!isFirefox && chrome.alarms) {
    chrome.alarms.clear(RECONNECT_ALARM_NAME);
  }

  // Ensure the offscreen keepalive document is running (Chrome only).
  if (!isFirefox) {
    ensureOffscreenDocument();
  }
}

// ---------------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------------

/**
 * Handle an incoming native message (a CliRequest forwarded by the mediator).
 *
 * @param {object} msg - Deserialized CliRequest from the mediator.
 */
function handleNativeMessage(msg) {
  const { request_id, type: commandType, ...params } = msg;

  console.debug(
    `[browser-controller] Received command: ${commandType}`,
    { request_id, params },
  );

  dispatch(commandType, params)
    .then((data) => {
      console.debug(
        `[browser-controller] Command succeeded: ${commandType}`,
        { request_id, result_type: data?.type },
      );
      sendResponse(request_id, { status: "ok", data });
    })
    .catch((err) => {
      console.error(
        `[browser-controller] Command failed: ${commandType}`,
        { request_id, params, error: String(err), stack: err?.stack },
      );
      sendResponse(request_id, { status: "err", data: String(err) });
    });
}

/**
 * Send a response back to the mediator.
 *
 * @param {string} requestId - The correlation ID from the original request.
 * @param {{ status: "ok"|"err", data: unknown }} outcome
 */
function sendResponse(requestId, outcome) {
  if (!nativePort) {
    console.error(
      `[browser-controller] Cannot send response for ${requestId}: not connected`,
    );
    return;
  }
  try {
    nativePort.postMessage({
      message_type: "Response",
      request_id: requestId,
      outcome,
    });
  } catch (err) {
    console.error(
      `[browser-controller] Failed to postMessage response for ${requestId}`,
      { error: String(err), stack: err?.stack, outcome },
    );
  }
}

/**
 * Dispatch a command to the appropriate browser API handler.
 *
 * @param {string} commandType - The `type` field from the CliRequest.
 * @param {object} params - Remaining fields of the CliRequest.
 * @returns {Promise<object>} Resolves with the CliResult payload.
 */
async function dispatch(commandType, params) {
  switch (commandType) {
    case "GetBrowserInfo":
      return cmdGetBrowserInfo();
    case "ListWindows":
      return cmdListWindows();
    case "OpenWindow":
      return cmdOpenWindow(params.title_prefix ?? null, params.incognito ?? false);
    case "CloseWindow":
      return cmdCloseWindow(params.window_id);
    case "SetWindowTitlePrefix":
      return cmdSetWindowTitlePrefix(params.window_id, params.prefix);
    case "RemoveWindowTitlePrefix":
      return cmdRemoveWindowTitlePrefix(params.window_id);
    case "ListTabs":
      return cmdListTabs(params.window_id);
    case "OpenTab":
      return cmdOpenTab(
        params.window_id,
        params.insert_before_tab_id ?? null,
        params.insert_after_tab_id ?? null,
        params.url ?? null,
        params.username ?? null,
        params.password ?? null,
        params.background ?? false,
        params.cookie_store_id ?? null,
      );
    case "ActivateTab":
      return cmdActivateTab(params.tab_id);
    case "NavigateTab":
      return cmdNavigateTab(params.tab_id, params.url);
    case "GoBack":
      return cmdGoBack(params.tab_id, params.steps);
    case "GoForward":
      return cmdGoForward(params.tab_id, params.steps);
    case "ReloadTab":
      return cmdReloadTab(params.tab_id, params.bypass_cache ?? false);
    case "CloseTab":
      return cmdCloseTab(params.tab_id);
    case "PinTab":
      return cmdPinTab(params.tab_id);
    case "UnpinTab":
      return cmdUnpinTab(params.tab_id);
    case "ToggleReaderMode":
      return cmdToggleReaderMode(params.tab_id);
    case "DiscardTab":
      return cmdDiscardTab(params.tab_id);
    case "WarmupTab":
      return cmdWarmupTab(params.tab_id);
    case "MuteTab":
      return cmdMuteTab(params.tab_id);
    case "UnmuteTab":
      return cmdUnmuteTab(params.tab_id);
    case "MoveTab":
      return cmdMoveTab(params.tab_id, params.new_index);
    case "ListContainers":
      return cmdListContainers();
    case "ReopenTabInContainer":
      return cmdReopenTabInContainer(params.tab_id, params.cookie_store_id);
    case "ListDownloads":
      return cmdListDownloads(params.state ?? null, params.limit ?? null, params.query ?? null);
    case "StartDownload":
      return cmdStartDownload(params.url, params.filename ?? null, params.save_as ?? false, params.conflict_action ?? null);
    case "CancelDownload":
      return cmdCancelDownload(params.download_id);
    case "PauseDownload":
      return cmdPauseDownload(params.download_id);
    case "ResumeDownload":
      return cmdResumeDownload(params.download_id);
    case "RetryDownload":
      return cmdRetryDownload(params.download_id);
    case "EraseDownload":
      return cmdEraseDownload(params.download_id);
    case "EraseAllDownloads":
      return cmdEraseAllDownloads(params.state ?? null);
    default:
      throw new Error(`Unknown command type: ${commandType}`);
  }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

/** Returns a BrowserInfo-shaped CliResult. */
async function cmdGetBrowserInfo() {
  const info = await fetchBrowserInfo();
  // pid is the browser's PID; we don't have direct access from the extension,
  // so the mediator fills this in — return 0 as a sentinel.
  return {
    type: "BrowserInfo",
    browser_name: info.name,
    browser_vendor: info.vendor ?? null,
    browser_version: info.version,
    pid: 0,
  };
}

/** Returns a Windows-shaped CliResult with tab summaries. */
async function cmdListWindows() {
  const [windows, lastFocused] = await Promise.all([
    browser.windows.getAll({ populate: true }),
    browser.windows.getLastFocused(),
  ]);

  // Self-calibrate windowTitleSuffix from any window that has no prefix:
  // if win.title starts with the active tab's title, the remainder is the suffix.
  // This is authoritative — it doesn't depend on getBrowserInfo() name formatting.
  if (!windowTitleSuffix) {
    for (const win of windows) {
      const activeTab = (win.tabs ?? []).find((t) => t.active);
      if (activeTab?.title && win.title?.startsWith(activeTab.title)) {
        windowTitleSuffix = win.title.substring(activeTab.title.length);
        break;
      }
    }
  }

  return {
    type: "Windows",
    windows: await Promise.all(
      windows.map((win) => serializeWindowSummary(win, lastFocused.id)),
    ),
  };
}

/** Opens a new browser window and returns its ID.
 * @param {string|null} titlePrefix - Optional title prefix to set via `titlePreface`.
 * @param {boolean} incognito - Whether to open the window in private/incognito mode.
 */
async function cmdOpenWindow(titlePrefix, incognito) {
  if (titlePrefix !== null && !isFirefox) {
    throw new Error("Window title prefix is only supported on Firefox");
  }
  const createProps = {};
  if (incognito) {
    createProps.incognito = true;
  }
  const win = await browser.windows.create(createProps);
  if (titlePrefix !== null) {
    await browser.windows.update(win.id, { titlePreface: titlePrefix });
    await browser.sessions.setWindowValue(win.id, "titlePreface", titlePrefix);
  }
  return { type: "WindowId", window_id: win.id };
}

/** Closes a browser window. */
async function cmdCloseWindow(windowId) {
  await browser.windows.remove(windowId);
  return { type: "Unit" };
}

/** Sets the titlePreface (Firefox title prefix) for a window. */
async function cmdSetWindowTitlePrefix(windowId, prefix) {
  if (!isFirefox) throw new Error("SetWindowTitlePrefix is only supported on Firefox");
  await browser.windows.update(windowId, { titlePreface: prefix });
  await browser.sessions.setWindowValue(windowId, "titlePreface", prefix);
  return { type: "Unit" };
}

/** Removes the titlePreface from a window. */
async function cmdRemoveWindowTitlePrefix(windowId) {
  if (!isFirefox) throw new Error("RemoveWindowTitlePrefix is only supported on Firefox");
  await browser.windows.update(windowId, { titlePreface: "" });
  await browser.sessions.removeWindowValue(windowId, "titlePreface");
  return { type: "Unit" };
}

/** Returns a Tabs-shaped CliResult with full tab details for all tabs in a window. */
async function cmdListTabs(windowId) {
  const tabs = await browser.tabs.query({ windowId });
  return {
    type: "Tabs",
    tabs: await Promise.all(tabs.map(serializeTabDetails)),
  };
}

/**
 * Wait for a specific tab to reach "complete" status.
 *
 * Resolves immediately if the tab is already complete; otherwise waits for
 * the next tabs.onUpdated event that sets status to "complete" for this tab.
 *
 * @param {number} tabId
 * @returns {Promise<void>}
 */
async function waitForTabComplete(tabId) {
  const current = await browser.tabs.get(tabId);
  if (current.status === "complete") {
    return;
  }
  await new Promise((resolve) => {
    function onUpdated(updatedTabId, changeInfo) {
      if (updatedTabId === tabId && changeInfo.status === "complete") {
        browser.tabs.onUpdated.removeListener(onUpdated);
        resolve();
      }
    }
    browser.tabs.onUpdated.addListener(onUpdated, { tabId, properties: ["status"] });
  });
}

/** Opens a new tab and returns its details.
 *
 * When `username` and `password` are provided, any embedded credentials in the
 * URL are stripped and the credentials are stored in `pendingCredentials` so
 * the `onAuthRequired` listener can provide them to the browser's auth
 * challenge. The browser then caches the credentials for the realm, so
 * subsequent requests work automatically.
 */
async function cmdOpenTab(windowId, insertBeforeTabId, insertAfterTabId, url, username, password, background, cookieStoreId) {
  const createProps = { windowId, active: !background };
  if (cookieStoreId !== null) {
    if (!isFirefox) throw new Error("Opening a tab in a container is only supported on Firefox");
    createProps.cookieStoreId = cookieStoreId;
  }
  if (insertBeforeTabId !== null) {
    const refTab = await browser.tabs.get(insertBeforeTabId);
    createProps.index = refTab.index;
  } else if (insertAfterTabId !== null) {
    const refTab = await browser.tabs.get(insertAfterTabId);
    createProps.index = refTab.index + 1;
  }

  // If credentials are provided, ensure the URL does not contain them and
  // register the credentials for the onAuthRequired listener BEFORE creating
  // the tab, because the 401 challenge may fire during tabs.create() itself.
  let cleanUrl = url;
  if (url !== null && username !== null && password !== null) {
    const parsed = new URL(url);
    parsed.username = "";
    parsed.password = "";
    cleanUrl = parsed.href;
    // Store by origin + cookieStoreId so onAuthRequired can match before we
    // know the tab ID, while allowing different credentials per container.
    pendingCredentials.set(credentialKey(parsed.origin, cookieStoreId), { username, password });
  }

  if (cleanUrl !== null) {
    createProps.url = cleanUrl;
  }
  let tab = await browser.tabs.create(createProps);

  // On Wayland, the compositor may block Firefox's attempt to activate the target
  // window during tabs.create, causing Firefox to fall back to the active window.
  // Detect this and move the tab to the correct window; tabs.move is a pure
  // internal operation that does not require compositor activation.
  if (tab.windowId !== windowId) {
    let moveIndex = -1;
    if (insertBeforeTabId !== null) {
      const refTab = await browser.tabs.get(insertBeforeTabId);
      moveIndex = refTab.index;
    } else if (insertAfterTabId !== null) {
      const refTab = await browser.tabs.get(insertAfterTabId);
      moveIndex = refTab.index + 1;
    }
    const moved = await browser.tabs.move(tab.id, { windowId, index: moveIndex });
    tab = Array.isArray(moved) ? moved[0] : moved;
  }

  // When credentials are provided, schedule cleanup of the pending
  // credentials entry.  The onAuthRequired listener will use the
  // credentials when the server responds with 401 (or the browser may
  // use its own credential cache for repeat visits).  We do NOT wait
  // for the page to finish loading — the auth exchange happens
  // asynchronously and some pages never reach "complete" status.
  if (url !== null && username !== null && password !== null) {
    const key = credentialKey(new URL(url).origin, cookieStoreId);
    // Clean up after 30 s — by then the auth exchange has either
    // succeeded or the user has dismissed the prompt.
    setTimeout(() => { pendingCredentials.delete(key); }, 30_000);
  }

  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Closes a tab. */
async function cmdCloseTab(tabId) {
  await browser.tabs.remove(tabId);
  return { type: "Unit" };
}

/** Pins a tab and returns its updated details. */
async function cmdPinTab(tabId) {
  const tab = await browser.tabs.update(tabId, { pinned: true });
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Unpins a tab and returns its updated details. */
async function cmdUnpinTab(tabId) {
  const tab = await browser.tabs.update(tabId, { pinned: false });
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Toggles Reader Mode for a tab (Firefox-only). */
async function cmdToggleReaderMode(tabId) {
  if (!isFirefox) throw new Error("ToggleReaderMode is only supported on Firefox");
  await browser.tabs.toggleReaderMode(tabId);
  const tab = await browser.tabs.get(tabId);
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Discards a tab, unloading its content from memory without closing it. */
async function cmdDiscardTab(tabId) {
  await browser.tabs.discard(tabId);
  return { type: "Unit" };
}

/** Warms up a discarded tab, loading its content into memory without activating it. */
async function cmdWarmupTab(tabId) {
  if (!isFirefox) throw new Error("WarmupTab is only supported on Firefox");
  await browser.tabs.warmup(tabId);
  const tab = await browser.tabs.get(tabId);
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Mutes a tab and returns its updated details. */
async function cmdMuteTab(tabId) {
  const tab = await browser.tabs.update(tabId, { muted: true });
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Unmutes a tab and returns its updated details. */
async function cmdUnmuteTab(tabId) {
  const tab = await browser.tabs.update(tabId, { muted: false });
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/**
 * Navigate a tab's session history by `delta` steps and return the resulting
 * tab details (including the new URL).
 *
 * First fetches the current tab state (including Navigation API history position
 * where available).  If the Navigation API reports that the boundary in the
 * requested direction is already reached (0 steps available), the current tab
 * state is returned immediately — no event listener or timer is needed.
 *
 * When navigation will occur, a one-time tabs.onUpdated listener (filtered to
 * URL changes for this tab) is registered before triggering history.go() so the
 * URL change event cannot be missed.  The listener is removed once the URL
 * changes.
 *
 * A 5 s fallback timer is set only when the Navigation API is unavailable
 * (history_steps_back === null) and we therefore cannot guarantee that a URL
 * change will occur.  When the Navigation API is available and steps > 0 the
 * navigation is guaranteed to produce a URL change, so no timer is needed.
 *
 * @param {number} tabId
 * @param {number} delta  Negative to go back, positive to go forward.
 * @returns {Promise<object>}
 */
async function navigateHistory(tabId, delta) {
  // Fetch the current tab state up-front.  serializeTabDetails queries the
  // Navigation API (if available) giving us the current history position.
  const tab = await browser.tabs.get(tabId);
  const currentDetails = await serializeTabDetails(tab);

  // When the Navigation API provided position data, check the boundary without
  // waiting for any events.  The Navigation API only sees same-document entries;
  // cross-document entries are hidden.  We can only be sure the boundary is
  // reached when there are no visible steps AND no hidden entries that might
  // contain cross-document history in the requested direction.
  if (currentDetails.history_steps_back !== null) {
    const stepsAvailable = delta < 0
      ? currentDetails.history_steps_back
      : currentDetails.history_steps_forward;
    const hiddenCount = currentDetails.history_hidden_count ?? 0;
    if (stepsAvailable === 0 && hiddenCount === 0) {
      // Truly at the boundary; no navigation will occur.
      return { type: "Tab", ...currentDetails };
    }
  }

  // Navigation will proceed; wait for the URL-change event.
  return new Promise((resolve, reject) => {
    let settled = false;

    async function finish() {
      if (settled) return;
      settled = true;
      browser.tabs.onUpdated.removeListener(onUpdated);
      if (fallbackTimer !== null) {
        clearTimeout(fallbackTimer);
      }
      try {
        const updatedTab = await browser.tabs.get(tabId);
        resolve({ type: "Tab", ...await serializeTabDetails(updatedTab) });
      } catch (err) {
        reject(err);
      }
    }

    function onUpdated(updatedTabId, changeInfo) {
      if (updatedTabId === tabId && changeInfo.url !== undefined) {
        finish();
      }
    }

    // A fallback timer is needed when the Navigation API is unavailable OR
    // when there are hidden (cross-document) history entries whose position
    // we cannot determine — history.go() may or may not produce a URL change.
    const hiddenCount = currentDetails.history_hidden_count ?? 0;
    const needsFallback = currentDetails.history_steps_back === null || hiddenCount > 0;
    const fallbackTimer = needsFallback
      ? setTimeout(finish, 5000)
      : null;

    browser.tabs.onUpdated.addListener(onUpdated, { tabId, properties: ["url"] });

    browser.scripting.executeScript({
      target: { tabId },
      func: (n) => { window.history.go(n); },
      args: [delta],
    }).catch((err) => {
      if (!settled) {
        settled = true;
        browser.tabs.onUpdated.removeListener(onUpdated);
        if (fallbackTimer !== null) {
          clearTimeout(fallbackTimer);
        }
        reject(err);
      }
    });
  });
}

/**
 * Navigates backward in a tab's session history by the given number of steps
 * and returns the resulting tab details.
 *
 * Uses window.history.go(-steps) so that all steps are skipped atomically,
 * which is useful when intermediate pages redirect immediately forward again.
 */
async function cmdGoBack(tabId, steps) {
  return navigateHistory(tabId, -steps);
}

/**
 * Navigates forward in a tab's session history by the given number of steps
 * and returns the resulting tab details.
 *
 * Uses window.history.go(steps) so that all steps are skipped atomically,
 * which is useful when intermediate pages redirect immediately backward again.
 */
async function cmdGoForward(tabId, steps) {
  return navigateHistory(tabId, steps);
}

/** Reloads a tab, optionally bypassing the cache. */
async function cmdReloadTab(tabId, bypassCache) {
  await browser.tabs.reload(tabId, { bypassCache });
  const tab = await browser.tabs.get(tabId);
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Navigates an existing tab to a new URL and returns its updated details. */
async function cmdNavigateTab(tabId, url) {
  const tab = await browser.tabs.update(tabId, { url });
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Activates a tab, making it the focused tab in its window. */
async function cmdActivateTab(tabId) {
  const tab = await browser.tabs.update(tabId, { active: true });
  return { type: "Tab", ...await serializeTabDetails(tab) };
}

/** Moves a tab to a new index within its window and returns its updated details. */
async function cmdMoveTab(tabId, newIndex) {
  const [moved] = await browser.tabs.move(tabId, { index: newIndex });
  return { type: "Tab", ...await serializeTabDetails(moved) };
}

// ---------------------------------------------------------------------------
// Download command implementations
// ---------------------------------------------------------------------------

/** Serialize a browser DownloadItem to the wire format. */
function serializeDownloadItem(item) {
  return {
    id: item.id,
    url: item.url ?? "",
    filename: item.filename ?? "",
    state: item.state ?? "in_progress",
    bytes_received: item.bytesReceived ?? 0,
    total_bytes: item.totalBytes ?? -1,
    file_size: item.fileSize ?? -1,
    error: item.error || null,
    start_time: item.startTime ?? "",
    end_time: item.endTime || null,
    paused: item.paused ?? false,
    can_resume: item.canResume ?? false,
    exists: item.exists ?? false,
    mime: item.mime || null,
    incognito: item.incognito ?? false,
  };
}

/** List downloads, optionally filtered by state. */
async function cmdListDownloads(state, limit, query) {
  const searchQuery = {};
  if (state !== null) {
    searchQuery.state = state;
  }
  if (limit !== null) {
    searchQuery.limit = limit;
  }
  if (query !== null) {
    searchQuery.query = [query];
  }
  const items = await browser.downloads.search(searchQuery);
  return {
    type: "Downloads",
    downloads: items.map(serializeDownloadItem),
  };
}

/** Start a new download and return its ID. */
async function cmdStartDownload(url, filename, saveAs, conflictAction) {
  const opts = { url, saveAs };
  if (filename !== null) {
    opts.filename = filename;
  }
  if (conflictAction !== null) {
    opts.conflictAction = conflictAction;
  }
  const downloadId = await browser.downloads.download(opts);
  return { type: "DownloadId", download_id: downloadId };
}

/** Cancel an active download. */
async function cmdCancelDownload(downloadId) {
  await browser.downloads.cancel(downloadId);
  return { type: "Unit" };
}

/** Pause an active download. */
async function cmdPauseDownload(downloadId) {
  await browser.downloads.pause(downloadId);
  return { type: "Unit" };
}

/** Resume a paused download. */
async function cmdResumeDownload(downloadId) {
  await browser.downloads.resume(downloadId);
  return { type: "Unit" };
}

/** Retry an interrupted download by re-downloading from the same URL. */
async function cmdRetryDownload(downloadId) {
  const [item] = await browser.downloads.search({ id: downloadId });
  if (!item) {
    throw new Error(`Download ${downloadId} not found`);
  }
  if (item.state !== "interrupted") {
    throw new Error(`Download ${downloadId} is not interrupted (state: ${item.state})`);
  }
  const newId = await browser.downloads.download({ url: item.url });
  return { type: "DownloadId", download_id: newId };
}

/** Remove a single download from the browser's history. */
async function cmdEraseDownload(downloadId) {
  await browser.downloads.erase({ id: downloadId });
  return { type: "Unit" };
}

/** Clear all downloads from history, optionally filtered by state. */
async function cmdEraseAllDownloads(state) {
  const query = {};
  if (state !== null) {
    query.state = state;
  }
  await browser.downloads.erase(query);
  return { type: "Unit" };
}

// ---------------------------------------------------------------------------
// Container command implementations
// ---------------------------------------------------------------------------

/** List all Firefox containers (contextual identities). */
async function cmdListContainers() {
  if (!isFirefox) throw new Error("ListContainers is only supported on Firefox");
  const identities = await browser.contextualIdentities.query({});
  return {
    type: "Containers",
    containers: identities.map((ci) => ({
      cookie_store_id: ci.cookieStoreId,
      name: ci.name,
      color: ci.color,
      color_code: ci.colorCode,
      icon: ci.icon,
    })),
  };
}

/** Close a tab and reopen its URL in a different container. */
async function cmdReopenTabInContainer(tabId, cookieStoreId) {
  if (!isFirefox) throw new Error("ReopenTabInContainer is only supported on Firefox");
  const tab = await browser.tabs.get(tabId);
  const url = tab.url;
  const windowId = tab.windowId;
  const index = tab.index;
  await browser.tabs.remove(tabId);
  const newTab = await browser.tabs.create({
    url,
    windowId,
    index,
    cookieStoreId,
  });
  return { type: "Tab", ...await serializeTabDetails(newTab) };
}

// ---------------------------------------------------------------------------
// Container name resolution
// ---------------------------------------------------------------------------

/**
 * Resolve a cookieStoreId to a human-readable container name.
 *
 * Returns null on Chrome or when the identity is not found.
 *
 * @param {string|undefined|null} cookieStoreId
 * @returns {Promise<string|null>}
 */
async function resolveContainerName(cookieStoreId) {
  if (!cookieStoreId || !isFirefox || !browser.contextualIdentities) {
    return null;
  }
  try {
    const identity = await browser.contextualIdentities.get(cookieStoreId);
    return identity?.name ?? null;
  } catch {
    // cookieStoreId is "firefox-default" or "firefox-private" which are not
    // contextual identities — contextualIdentities.get() throws for them.
    return null;
  }
}

// ---------------------------------------------------------------------------
// Serialization helpers
// ---------------------------------------------------------------------------

/**
 * Recover the titlePreface from a populated window object.
 *
 * Firefox window titles normally have the form:
 *   {titlePreface}{activeTab.title}[{browserSuffix}]
 *
 * However, Firefox sometimes omits the tab title from the window title entirely
 * (e.g. a newly opened window whose active tab is blank). In that case the title
 * has the form:
 *   {titlePreface}{browserSuffix}   — or just {titlePreface}
 *
 * Since titlePreface is write-only in the API (not returned by windows.getAll),
 * we extract it by finding where the active tab's title ends in the window title.
 * Four strategies are tried in order, from most to least anchored:
 *
 * 1. endsWith(tabTitle + browserSuffix) — most precise; rules out false matches
 *    where the tab title also appears inside the prefix.
 * 2. endsWith(tabTitle) — handles the case where win.title does not include the
 *    browser suffix (the Firefox extension API may strip it).
 * 3. lastIndexOf(tabTitle) — last resort for unusual title formats; the rightmost
 *    occurrence is almost always the real tab title, not something in the prefix.
 * 4. endsWith(browserSuffix) — used when the tab title is absent from the window
 *    title altogether; strips the suffix to recover the bare prefix.
 *
 * Returns null when no prefix is present or it cannot be determined.
 *
 * @param {browser.windows.Window} win - Must be populated (tabs present).
 * @returns {string|null}
 */
function extractTitlePreface(win) {
  if (!isFirefox) return null;
  const activeTab = (win.tabs ?? []).find((t) => t.active);
  const tabTitle = activeTab?.title;
  if (!win.title) return null;

  if (tabTitle) {
    // Strategy 1: anchored to browser suffix (e.g. " — Firefox").
    if (windowTitleSuffix) {
      const needle = tabTitle + windowTitleSuffix;
      if (win.title.endsWith(needle)) {
        const len = win.title.length - needle.length;
        return len > 0 ? win.title.substring(0, len) : null;
      }
    }

    // Strategy 2: win.title ends directly with the tab title (no browser suffix).
    if (win.title.endsWith(tabTitle)) {
      const len = win.title.length - tabTitle.length;
      return len > 0 ? win.title.substring(0, len) : null;
    }

    // Strategy 3: rightmost occurrence of the tab title anywhere in win.title.
    const idx = win.title.lastIndexOf(tabTitle);
    if (idx > 0) return win.title.substring(0, idx);
  }

  // Strategy 4: Firefox omitted the tab title from the window title entirely
  // (e.g. new window with a blank tab). Strip the browser suffix if known to
  // recover the bare prefix.
  if (windowTitleSuffix && win.title.endsWith(windowTitleSuffix)) {
    const len = win.title.length - windowTitleSuffix.length;
    return len > 0 ? win.title.substring(0, len) : null;
  }

  return null;
}

/**
 * Serialize a browser `windows.Window` object to a `WindowSummary`.
 *
 * On Firefox, the stored titlePreface value from sessions storage is preferred
 * over extractTitlePreface() because Firefox may strip trailing whitespace from
 * the titlePreface when composing the window title, making extraction lossy.
 *
 * @param {browser.windows.Window} win
 * @param {number} lastFocusedId - ID of the most recently focused window.
 * @returns {Promise<object>}
 */
async function serializeWindowSummary(win, lastFocusedId) {
  // Prefer the stored prefix (exact value the user set) over extraction from
  // the window title, which may lose trailing whitespace.
  let titlePrefix = null;
  if (isFirefox) {
    const stored = await browser.sessions.getWindowValue(win.id, "titlePreface");
    titlePrefix = stored !== undefined ? stored : extractTitlePreface(win);
  }

  return {
    id: win.id,
    title: win.title ?? "",
    title_prefix: titlePrefix,
    is_focused: win.focused,
    is_last_focused: win.id === lastFocusedId,
    state: win.state ?? "normal",
    tabs: await Promise.all((win.tabs ?? []).map(serializeTabSummary)),
  };
}

/**
 * Serialize a browser `tabs.Tab` to a `TabSummary` (brief view for window listings).
 *
 * @param {browser.tabs.Tab} tab
 * @returns {Promise<object>}
 */
async function serializeTabSummary(tab) {
  return {
    id: tab.id,
    index: tab.index,
    title: tab.title ?? "",
    url: tab.url ?? "",
    is_active: tab.active,
    cookie_store_id: tab.cookieStoreId ?? null,
    container_name: await resolveContainerName(tab.cookieStoreId),
  };
}

/**
 * Retrieve session history metrics for a tab by injecting a content script.
 *
 * Always returns `history_length` (from `window.history.length`).
 * Also returns `history_steps_back` and `history_steps_forward` when the
 * Navigation API (`window.navigation`) is available (Firefox 125+); both are
 * `null` on older Firefox or privileged pages.
 *
 * Returns all-zero/null for discarded tabs or any tab that does not permit
 * content script injection.
 *
 * @param {number} tabId
 * @param {boolean} isDiscarded
 * @returns {Promise<{history_length: number, history_steps_back: number|null, history_steps_forward: number|null, history_hidden_count: number|null}>}
 */
async function getTabHistoryInfo(tabId, isDiscarded) {
  if (isDiscarded) {
    return { history_length: 0, history_steps_back: null, history_steps_forward: null, history_hidden_count: null };
  }
  try {
    const results = await browser.scripting.executeScript({
      target: { tabId },
      func: () => {
        const jointTotal = window.history.length;
        if (window.navigation?.entries) {
          const entries = window.navigation.entries();
          const pos = window.navigation.currentEntry.index;
          return {
            total: entries.length,
            back: pos,
            forward: entries.length - 1 - pos,
            hidden: jointTotal - entries.length,
          };
        }
        return { total: jointTotal, back: null, forward: null, hidden: null };
      },
    });
    const [first] = results;
    const info = first?.result;
    return {
      history_length: info?.total ?? 0,
      history_steps_back: info?.back ?? null,
      history_steps_forward: info?.forward ?? null,
      history_hidden_count: info?.hidden ?? null,
    };
  } catch (_err) {
    return { history_length: 0, history_steps_back: null, history_steps_forward: null, history_hidden_count: null };
  }
}

/**
 * Serialize a browser `tabs.Tab` to a `TabDetails` (full view).
 *
 * @param {browser.tabs.Tab} tab
 * @returns {Promise<object>}
 */
async function serializeTabDetails(tab) {
  const { history_length, history_steps_back, history_steps_forward, history_hidden_count } =
    await getTabHistoryInfo(tab.id, tab.discarded ?? false);
  return {
    id: tab.id,
    index: tab.index,
    window_id: tab.windowId,
    title: tab.title ?? "",
    url: tab.url ?? "",
    is_active: tab.active,
    is_pinned: tab.pinned,
    is_discarded: tab.discarded ?? false,
    is_audible: tab.audible ?? false,
    is_muted: tab.mutedInfo?.muted ?? false,
    status: tab.status ?? "complete",
    has_attention: tab.attention ?? false,
    is_awaiting_auth: pendingAuthTabs.has(tab.id),
    is_in_reader_mode: tab.isInReaderMode ?? false,
    incognito: tab.incognito,
    history_length,
    history_steps_back,
    history_steps_forward,
    history_hidden_count,
    cookie_store_id: tab.cookieStoreId ?? null,
    container_name: await resolveContainerName(tab.cookieStoreId),
  };
}

/**
 * Re-apply stored titlePreface values to all currently open windows.
 *
 * Called once on startup to handle windows that already existed before the
 * extension started (e.g. the extension was reloaded while Firefox was running).
 * Windows that appear later — including those created during ongoing session
 * restore — are handled by the windows.onCreated listener registered above.
 */
async function restoreTitlePrefaces() {
  if (!isFirefox) return;
  const windows = await browser.windows.getAll();
  for (const win of windows) {
    const prefix = await browser.sessions.getWindowValue(win.id, "titlePreface");
    if (prefix !== undefined) {
      await browser.windows.update(win.id, { titlePreface: prefix });
    }
  }
}

// ---------------------------------------------------------------------------
// Chrome service worker keepalive
// ---------------------------------------------------------------------------

/**
 * Ensure the offscreen keepalive document is running (Chrome only).
 *
 * Chrome MV3 can terminate service workers after 30 seconds of inactivity.
 * The offscreen document sends a periodic keepalive message that resets the
 * idle timer, preventing termination while the native messaging connection
 * is active.
 */
async function ensureOffscreenDocument() {
  if (isFirefox || typeof chrome === "undefined" || !chrome.offscreen) return;
  try {
    const contexts = await chrome.runtime.getContexts({
      contextTypes: ["OFFSCREEN_DOCUMENT"],
    });
    if (contexts.length === 0) {
      await chrome.offscreen.createDocument({
        url: "offscreen.html",
        reasons: ["BLOBS"],
        justification: "Keep service worker alive for native messaging connection",
      });
      console.info("[browser-controller] Offscreen keepalive document created.");
    }
  } catch (err) {
    console.warn("[browser-controller] Failed to create offscreen document", {
      error: String(err),
      stack: err?.stack,
    });
  }
}

// Handle keepalive messages from the offscreen document. Just receiving
// the message resets the service worker's idle timer — no action needed.
if (!isFirefox) {
  chrome.runtime.onMessage.addListener((msg) => {
    if (msg.type === "keepalive") return;
  });
}

// Chrome alarms-based reconnect safety net. If the service worker is
// terminated despite the offscreen keepalive (known Chrome bugs), the
// alarm wakes it back up and re-establishes the native messaging connection.
if (!isFirefox && typeof chrome !== "undefined" && chrome.alarms) {
  chrome.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === RECONNECT_ALARM_NAME) {
      console.info("[browser-controller] Alarm-based reconnect triggered.");
      if (nativePort === null) {
        reconnectDelayMs = Math.min(reconnectDelayMs * 2, MAX_RECONNECT_DELAY_MS);
        connect();
      }
    }
  });
}

// ---------------------------------------------------------------------------
// Start
// ---------------------------------------------------------------------------

connect();
restoreTitlePrefaces();
