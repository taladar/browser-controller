/**
 * Browser Controller — MV3 service worker.
 *
 * Connects to the native messaging host (browser-controller-mediator) and
 * handles commands forwarded by the mediator to control windows and tabs.
 *
 * Supports Firefox (121+) and Chrome/Chromium-based browsers. Firefox-only
 * features (title prefix, sessions API, tab warmup) are guarded by `isFirefox`
 * and degrade to no-ops on Chrome.
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

/** Tabs currently awaiting basic HTTP authentication (tracked via webRequest). */
const pendingAuthTabs = new Set();


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
  (details) => {
    if (details.tabId >= 0) {
      pendingAuthTabs.add(details.tabId);
    }
  },
  { urls: ["<all_urls>"] },
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
  if (ua.includes("Edg/")) {
    const edgeVersion = ua.match(/Edg\/([\d.]+)/)?.[1] ?? chromeVersion;
    return { name: "Edge", vendor: "Microsoft", version: edgeVersion };
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
    setTimeout(() => {
      reconnectDelayMs = Math.min(reconnectDelayMs * 2, MAX_RECONNECT_DELAY_MS);
      connect();
    }, reconnectDelayMs);
  });

  // Reset backoff on successful connection (assumed when first message arrives).
  reconnectDelayMs = INITIAL_RECONNECT_DELAY_MS;
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
      return cmdOpenWindow();
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
        params.strip_credentials ?? false,
        params.background ?? false,
      );
    case "ActivateTab":
      return cmdActivateTab(params.tab_id);
    case "NavigateTab":
      return cmdNavigateTab(params.tab_id, params.url);
    case "GoBack":
      return cmdGoBack(params.tab_id, params.steps);
    case "GoForward":
      return cmdGoForward(params.tab_id, params.steps);
    case "CloseTab":
      return cmdCloseTab(params.tab_id);
    case "PinTab":
      return cmdPinTab(params.tab_id);
    case "UnpinTab":
      return cmdUnpinTab(params.tab_id);
    case "WarmupTab":
      return cmdWarmupTab(params.tab_id);
    case "MuteTab":
      return cmdMuteTab(params.tab_id);
    case "UnmuteTab":
      return cmdUnmuteTab(params.tab_id);
    case "MoveTab":
      return cmdMoveTab(params.tab_id, params.new_index);
    default:
      throw new Error(`Unknown command type: ${commandType}`);
  }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

/** Returns a BrowserInfo-shaped CliResult. */
async function cmdGetBrowserInfo() {
  const info = await browser.runtime.getBrowserInfo();
  // pid is the browser's PID; we don't have direct access from the extension,
  // so the mediator fills this in — return 0 as a sentinel.
  return {
    type: "BrowserInfo",
    browser_name: info.name,
    browser_version: info.version,
    pid: 0,
  };
}

/** Returns a Windows-shaped CliResult with tab summaries. */
async function cmdListWindows() {
  const windows = await browser.windows.getAll({ populate: true });

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
    windows: windows.map(serializeWindowSummary),
  };
}

/** Opens a new browser window and returns its ID. */
async function cmdOpenWindow() {
  const win = await browser.windows.create({});
  return { type: "WindowId", window_id: win.id };
}

/** Closes a browser window. */
async function cmdCloseWindow(windowId) {
  await browser.windows.remove(windowId);
  return { type: "Unit" };
}

/** Sets the titlePreface (Firefox title prefix) for a window. */
async function cmdSetWindowTitlePrefix(windowId, prefix) {
  if (!isFirefox) return { type: "Unit" };
  await browser.windows.update(windowId, { titlePreface: prefix });
  await browser.sessions.setWindowValue(windowId, "titlePreface", prefix);
  return { type: "Unit" };
}

/** Removes the titlePreface from a window. */
async function cmdRemoveWindowTitlePrefix(windowId) {
  if (!isFirefox) return { type: "Unit" };
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

/** Opens a new tab and returns its details. */
async function cmdOpenTab(windowId, insertBeforeTabId, insertAfterTabId, url, stripCredentials, background) {
  const createProps = { windowId, active: !background };
  if (insertBeforeTabId !== null) {
    const refTab = await browser.tabs.get(insertBeforeTabId);
    createProps.index = refTab.index;
  } else if (insertAfterTabId !== null) {
    const refTab = await browser.tabs.get(insertAfterTabId);
    createProps.index = refTab.index + 1;
  }
  if (url !== null) {
    createProps.url = url;
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

  if (stripCredentials && url !== null) {
    const parsed = new URL(url);
    if (parsed.username !== "" || parsed.password !== "") {
      parsed.username = "";
      parsed.password = "";
      const cleanUrl = parsed.href;
      await waitForTabComplete(tab.id);
      await browser.tabs.update(tab.id, { url: cleanUrl });
      await waitForTabComplete(tab.id);
      tab = await browser.tabs.get(tab.id);
    }
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

/** Warms up a discarded tab, loading its content into memory without activating it. */
async function cmdWarmupTab(tabId) {
  if (!browser.tabs.warmup) {
    return { type: "Unit" };
  }
  await browser.tabs.warmup(tabId);
  return { type: "Unit" };
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
  // waiting for any events.
  if (currentDetails.history_steps_back !== null) {
    const stepsAvailable = delta < 0
      ? currentDetails.history_steps_back
      : currentDetails.history_steps_forward;
    if (stepsAvailable === 0) {
      // Already at the boundary; no navigation will occur.
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

    // A fallback timer is only needed when the Navigation API is unavailable
    // and we cannot guarantee that a URL-change event will arrive.
    const fallbackTimer = currentDetails.history_steps_back === null
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
 * @param {browser.windows.Window} win
 * @returns {object}
 */
function serializeWindowSummary(win) {
  return {
    id: win.id,
    title: win.title ?? "",
    title_prefix: extractTitlePreface(win),
    is_focused: win.focused,
    state: win.state ?? "normal",
    tabs: (win.tabs ?? []).map(serializeTabSummary),
  };
}

/**
 * Serialize a browser `tabs.Tab` to a `TabSummary` (brief view for window listings).
 *
 * @param {browser.tabs.Tab} tab
 * @returns {object}
 */
function serializeTabSummary(tab) {
  return {
    index: tab.index,
    title: tab.title ?? "",
    url: tab.url ?? "",
    is_active: tab.active,
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
// Start
// ---------------------------------------------------------------------------

connect();
restoreTitlePrefaces();
