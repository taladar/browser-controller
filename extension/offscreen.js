/**
 * Offscreen document keepalive for Chrome MV3.
 *
 * Chrome can terminate service workers after 30 seconds of inactivity.
 * This offscreen document sends a periodic keepalive message to the
 * service worker, resetting its idle timer and preventing termination
 * while the native messaging connection is active.
 *
 * This file is only loaded on Chrome — Firefox does not need it because
 * Firefox MV3 background scripts are persistent.
 */

"use strict";

setInterval(() => {
  chrome.runtime.sendMessage({ type: "keepalive" }).catch(() => {
    // Service worker may not be ready yet; ignore and retry on next tick.
  });
}, 20000);
