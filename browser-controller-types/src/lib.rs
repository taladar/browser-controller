//! Shared protocol types for the browser-controller system.
//!
//! This crate defines the data types used in communication between:
//! - The CLI and the mediator (over Unix Domain Socket, newline-delimited JSON)
//! - The mediator and the Firefox extension (via native messaging, length-prefixed JSON)

use serde::{Deserialize, Serialize};

/// Information about a running browser instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserInfo {
    /// Human-readable browser name (e.g. "Firefox", "Chrome").
    pub browser_name: String,
    /// Browser vendor (e.g. "Mozilla").
    ///
    /// `None` when not reported by the browser (non-Firefox browsers or older versions).
    #[serde(default)]
    pub browser_vendor: Option<String>,
    /// Browser version string (e.g. "120.0").
    pub browser_version: String,
    /// PID of the browser's main process.
    pub pid: u32,
    /// The browser profile identifier (directory basename, e.g. `abc123.default-release`).
    ///
    /// `None` when the profile cannot be determined (non-Linux platforms or if
    /// the browser was not launched with an explicit `--profile` flag).
    #[serde(default)]
    pub profile_id: Option<String>,
}

/// The visual state of a browser window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowState {
    /// Window is in its normal state.
    Normal,
    /// Window is minimized.
    Minimized,
    /// Window is maximized.
    Maximized,
    /// Window is in full-screen mode.
    Fullscreen,
}

/// A brief summary of a tab, suitable for embedding in window listings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSummary {
    /// The browser-assigned tab ID.
    pub id: u32,
    /// Zero-based position of the tab within its window.
    pub index: u32,
    /// The tab's title.
    pub title: String,
    /// The URL currently loaded in the tab.
    pub url: String,
    /// Whether this is the currently active (focused) tab in its window.
    pub is_active: bool,
    /// The cookie store (container) ID this tab belongs to.
    ///
    /// Firefox-specific; `None` on browsers that don't support containers.
    #[serde(default)]
    pub cookie_store_id: Option<String>,
}

/// A summary of a browser window including its tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSummary {
    /// The window's unique identifier within the browser.
    pub id: u32,
    /// The full window title as displayed in the title bar.
    pub title: String,
    /// An optional prefix prepended to the window title (Firefox-only, via `titlePreface`).
    pub title_prefix: Option<String>,
    /// Whether this window currently has input focus.
    pub is_focused: bool,
    /// Whether this is the most recently focused window.
    ///
    /// This differs from `is_focused` when no window currently has OS-level focus
    /// (e.g. all browser windows are on an inactive Wayland workspace). Firefox tracks
    /// last-focused state internally and uses it as the fallback target when creating
    /// a tab without a specific window.
    pub is_last_focused: bool,
    /// The current visual state of the window.
    pub state: WindowState,
    /// Brief summaries of the tabs open in this window.
    pub tabs: Vec<TabSummary>,
}

/// The loading status of a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabStatus {
    /// The tab is currently loading.
    Loading,
    /// The tab has finished loading.
    Complete,
}

/// The state of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    /// The download is actively receiving data.
    InProgress,
    /// The download completed successfully.
    Complete,
    /// The download was interrupted (check `error` for the reason).
    Interrupted,
}

impl std::fmt::Display for DownloadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InProgress => write!(f, "in_progress"),
            Self::Complete => write!(f, "complete"),
            Self::Interrupted => write!(f, "interrupted"),
        }
    }
}

/// How to handle filename conflicts when downloading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilenameConflictAction {
    /// Add a number to the filename to make it unique.
    Uniquify,
    /// Overwrite the existing file.
    Overwrite,
    /// Prompt the user.
    Prompt,
}

impl std::fmt::Display for FilenameConflictAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uniquify => write!(f, "uniquify"),
            Self::Overwrite => write!(f, "overwrite"),
            Self::Prompt => write!(f, "prompt"),
        }
    }
}

/// Details about a download.
#[expect(
    clippy::struct_excessive_bools,
    reason = "DownloadItem mirrors the browser's DownloadItem API, which exposes each state as a separate boolean property"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadItem {
    /// Browser-assigned download ID.
    pub id: u32,
    /// The URL that was downloaded.
    pub url: String,
    /// Absolute filesystem path where the file was saved.
    pub filename: String,
    /// Current state of the download.
    pub state: DownloadState,
    /// Bytes received so far.
    pub bytes_received: u64,
    /// Total file size in bytes, or -1 if unknown.
    pub total_bytes: i64,
    /// Final file size in bytes, or -1 if unknown.
    pub file_size: i64,
    /// Error reason if the download was interrupted.
    #[serde(default)]
    pub error: Option<String>,
    /// ISO 8601 timestamp when the download started.
    pub start_time: String,
    /// ISO 8601 timestamp when the download ended.
    #[serde(default)]
    pub end_time: Option<String>,
    /// Whether the download is paused.
    pub paused: bool,
    /// Whether an interrupted download can be resumed.
    pub can_resume: bool,
    /// Whether the downloaded file still exists on disk.
    pub exists: bool,
    /// MIME type of the downloaded file.
    #[serde(default)]
    pub mime: Option<String>,
    /// Whether the download is associated with a private/incognito session.
    pub incognito: bool,
}

/// Full details about a browser tab.
#[expect(
    clippy::struct_excessive_bools,
    reason = "TabDetails mirrors the Firefox tabs.Tab API, which exposes each state as a separate boolean property"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabDetails {
    /// The tab's unique identifier within the browser.
    pub id: u32,
    /// Zero-based position of the tab within its window.
    pub index: u32,
    /// The identifier of the window that contains this tab.
    pub window_id: u32,
    /// The tab's title.
    pub title: String,
    /// The URL currently loaded in the tab.
    pub url: String,
    /// Whether this is the currently active (focused) tab in its window.
    pub is_active: bool,
    /// Whether this tab is pinned.
    pub is_pinned: bool,
    /// Whether this tab has been discarded (unloaded from memory to save resources).
    pub is_discarded: bool,
    /// Whether this tab is currently producing audio.
    pub is_audible: bool,
    /// Whether this tab's audio is muted.
    pub is_muted: bool,
    /// The current loading status of the tab.
    pub status: TabStatus,
    /// Whether this tab is drawing user attention (e.g. a modal dialog is open, including basic auth prompts).
    ///
    /// Corresponds to the `attention` field in the Firefox `tabs.Tab` API.
    #[serde(default)]
    pub has_attention: bool,
    /// Whether this tab is currently waiting for basic HTTP authentication credentials.
    ///
    /// Tracked by the extension via `browser.webRequest.onAuthRequired`.
    #[serde(default)]
    pub is_awaiting_auth: bool,
    /// Whether this tab is currently displayed in Reader Mode.
    ///
    /// Firefox-specific; will be `false` on browsers that do not support Reader Mode.
    #[serde(default)]
    pub is_in_reader_mode: bool,
    /// Whether this tab is open in a private/incognito window.
    pub incognito: bool,
    /// Number of entries in the tab's session history (back/forward stack).
    ///
    /// Populated via `window.history.length`; always available when the tab allows
    /// content script injection. May be 0 for discarded tabs or privileged pages.
    #[serde(default)]
    pub history_length: u32,
    /// Number of steps that can be navigated backward from the current history entry.
    ///
    /// `None` when the Navigation API (`window.navigation`) is unavailable (Firefox < 125
    /// or privileged pages). When `Some`, equals the 0-based index of the current entry
    /// in the history stack.
    #[serde(default)]
    pub history_steps_back: Option<u32>,
    /// Number of steps that can be navigated forward from the current history entry.
    ///
    /// `None` under the same conditions as [`TabDetails::history_steps_back`].
    #[serde(default)]
    pub history_steps_forward: Option<u32>,
    /// Number of history entries that exist but are inaccessible to the current document.
    ///
    /// These are cross-origin entries (or entries from a different document in the same
    /// tab) that appear in the joint session history but are hidden from the Navigation
    /// API for security reasons.  Computed as `window.history.length −
    /// navigation.entries().length` when the Navigation API is available.
    ///
    /// `Some(0)` means the Navigation API is available and all entries are accessible.
    /// `None` means the Navigation API is unavailable so the split cannot be determined;
    /// in that case [`TabDetails::history_length`] already reflects the full total.
    #[serde(default)]
    pub history_hidden_count: Option<u32>,
    /// The cookie store (container) ID this tab belongs to.
    ///
    /// Firefox-specific; `None` on browsers that don't support containers.
    #[serde(default)]
    pub cookie_store_id: Option<String>,
}

/// Information about a Firefox container (contextual identity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerInfo {
    /// The cookie store ID (e.g. `"firefox-container-1"`).
    pub cookie_store_id: String,
    /// Human-readable name (e.g. `"Work"`).
    pub name: String,
    /// Color identifier (e.g. `"blue"`).
    pub color: String,
    /// Hex color code (e.g. `"#37adff"`).
    pub color_code: String,
    /// Icon identifier (e.g. `"briefcase"`).
    pub icon: String,
}

/// An event emitted by the browser extension and broadcast to all event-stream subscribers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BrowserEvent {
    /// A new browser window was opened.
    WindowOpened {
        /// The new window's ID.
        window_id: u32,
        /// The window's title at the time it was created (may be empty).
        title: String,
    },
    /// A browser window was closed.
    WindowClosed {
        /// The ID of the closed window.
        window_id: u32,
    },
    /// The active tab in a window changed.
    TabActivated {
        /// The window containing the newly active tab.
        window_id: u32,
        /// The ID of the newly active tab.
        tab_id: u32,
        /// The ID of the previously active tab, if any.
        #[serde(default)]
        previous_tab_id: Option<u32>,
    },
    /// A new tab was opened.
    TabOpened {
        /// The new tab's ID.
        tab_id: u32,
        /// The window containing the new tab.
        window_id: u32,
        /// Zero-based position of the tab within its window.
        index: u32,
        /// The URL loaded in the tab at creation time (may be empty or `"about:blank"`).
        url: String,
        /// The tab's title at creation time (often empty).
        title: String,
    },
    /// A tab was closed.
    TabClosed {
        /// The ID of the closed tab.
        tab_id: u32,
        /// The window that contained the tab.
        window_id: u32,
        /// Whether the tab was closed because its parent window was also closing.
        is_window_closing: bool,
    },
    /// A tab started loading a new URL.
    TabNavigated {
        /// The ID of the navigating tab.
        tab_id: u32,
        /// The window containing the tab.
        window_id: u32,
        /// The new URL.
        url: String,
    },
    /// A tab's title changed.
    TabTitleChanged {
        /// The ID of the tab.
        tab_id: u32,
        /// The window containing the tab.
        window_id: u32,
        /// The new title.
        title: String,
    },
    /// A tab's loading status changed (e.g. from `loading` to `complete`).
    TabStatusChanged {
        /// The ID of the tab.
        tab_id: u32,
        /// The window containing the tab.
        window_id: u32,
        /// The new loading status.
        status: TabStatus,
    },
    /// A new download was started.
    DownloadCreated {
        /// The download's ID.
        download_id: u32,
        /// The URL being downloaded.
        url: String,
        /// The filename (may be empty until determined).
        filename: String,
        /// The MIME type, if known.
        #[serde(default)]
        mime: Option<String>,
    },
    /// A download's state or properties changed.
    DownloadChanged {
        /// The download's ID.
        download_id: u32,
        /// The new state, if it changed.
        #[serde(default)]
        state: Option<DownloadState>,
        /// The new filename, if it changed.
        #[serde(default)]
        filename: Option<String>,
        /// The error reason, if the download was interrupted.
        #[serde(default)]
        error: Option<String>,
    },
    /// A download was removed from the browser's history.
    DownloadErased {
        /// The download's ID.
        download_id: u32,
    },
}

/// A command sent from the CLI to the mediator, and forwarded to the extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CliCommand {
    /// Retrieve information about the connected browser instance.
    GetBrowserInfo,
    /// List all open windows with their tab summaries.
    ListWindows,
    /// Open a new browser window.
    OpenWindow {
        /// Optional title prefix (Firefox `titlePreface`) to set on the new window.
        ///
        /// When set, the extension calls `browser.windows.update` immediately after
        /// creation with `{ titlePreface: title_prefix }`.
        #[serde(default)]
        title_prefix: Option<String>,
        /// If `true`, open the window in private/incognito browsing mode.
        ///
        /// The extension must be allowed to run in private windows for this to work.
        #[serde(default)]
        incognito: bool,
    },
    /// Close an existing browser window.
    CloseWindow {
        /// The ID of the window to close.
        window_id: u32,
    },
    /// Set the title prefix (Firefox `titlePreface`) for a window.
    SetWindowTitlePrefix {
        /// The ID of the window whose prefix to set.
        window_id: u32,
        /// The prefix string to prepend to the window title.
        prefix: String,
    },
    /// Remove the title prefix from a window, restoring the default title.
    RemoveWindowTitlePrefix {
        /// The ID of the window whose prefix to remove.
        window_id: u32,
    },
    /// List all tabs in a window with full details.
    ListTabs {
        /// The ID of the window whose tabs to list.
        window_id: u32,
    },
    /// Open a new tab in a window.
    OpenTab {
        /// The ID of the window in which to open the tab.
        window_id: u32,
        /// If set, the new tab will be inserted immediately before the tab with this ID.
        insert_before_tab_id: Option<u32>,
        /// If set, the new tab will be inserted immediately after the tab with this ID.
        insert_after_tab_id: Option<u32>,
        /// The URL to load in the new tab, or the browser's default new-tab page if absent.
        url: Option<String>,
        /// Optional username for HTTP authentication.
        ///
        /// When set (together with `password`), the extension opens the URL without
        /// embedded credentials and responds to the server's 401 challenge via the
        /// `webRequest.onAuthRequired` API, injecting the credentials into the browser's
        /// built-in authentication cache. Subsequent requests to the same realm reuse the
        /// cached credentials automatically. Requires `url` to be set.
        #[serde(default)]
        username: Option<String>,
        /// Optional password for HTTP authentication.
        ///
        /// Used together with `username`. Requires `url` to be set.
        #[serde(default)]
        password: Option<String>,
        /// If `true`, the new tab is created in the background and the currently active tab
        /// in the window remains active.
        #[serde(default)]
        background: bool,
        /// Firefox container (cookie store) ID to open the tab in.
        ///
        /// E.g. `"firefox-container-1"`. Ignored on browsers without container support.
        #[serde(default)]
        cookie_store_id: Option<String>,
    },
    /// Activate a tab, making it the focused tab in its window.
    ActivateTab {
        /// The ID of the tab to activate.
        tab_id: u32,
    },
    /// Navigate an existing tab to a new URL.
    NavigateTab {
        /// The ID of the tab to navigate.
        tab_id: u32,
        /// The URL to load in the tab.
        url: String,
    },
    /// Reload a tab.
    ReloadTab {
        /// The ID of the tab to reload.
        tab_id: u32,
        /// If `true`, bypass the browser cache (hard refresh).
        #[serde(default)]
        bypass_cache: bool,
    },
    /// Close a tab.
    CloseTab {
        /// The ID of the tab to close.
        tab_id: u32,
    },
    /// Pin a tab.
    PinTab {
        /// The ID of the tab to pin.
        tab_id: u32,
    },
    /// Unpin a tab.
    UnpinTab {
        /// The ID of the tab to unpin.
        tab_id: u32,
    },
    /// Toggle Reader Mode for a tab.
    ///
    /// Firefox-only. Switches the tab into or out of Reader Mode. The tab
    /// must be displaying a page that Firefox considers reader-mode compatible.
    ToggleReaderMode {
        /// The ID of the tab whose Reader Mode to toggle.
        tab_id: u32,
    },
    /// Discard a tab, unloading its content from memory without closing it.
    ///
    /// The tab remains in the tab strip but its content is freed. It will be
    /// reloaded when activated. Cannot discard the active tab.
    DiscardTab {
        /// The ID of the tab to discard.
        tab_id: u32,
    },
    /// Warm up a discarded tab, loading its content into memory without activating it.
    WarmupTab {
        /// The ID of the tab to warm up.
        tab_id: u32,
    },
    /// Mute a tab, suppressing any audio it produces.
    MuteTab {
        /// The ID of the tab to mute.
        tab_id: u32,
    },
    /// Unmute a tab, allowing it to produce audio again.
    UnmuteTab {
        /// The ID of the tab to unmute.
        tab_id: u32,
    },
    /// Move a tab to a new position within its window.
    MoveTab {
        /// The ID of the tab to move.
        tab_id: u32,
        /// The new zero-based index for the tab within its window.
        new_index: u32,
    },
    /// Navigate backward in a tab's session history.
    ///
    /// Returns a [`CliResult::Tab`] with the details of the page navigated to,
    /// or the current tab state if the history boundary was already reached.
    GoBack {
        /// The ID of the tab to navigate.
        tab_id: u32,
        /// Number of steps to go back (default 1).
        steps: u32,
    },
    /// Navigate forward in a tab's session history.
    ///
    /// Returns a [`CliResult::Tab`] with the details of the page navigated to,
    /// or the current tab state if the history boundary was already reached.
    GoForward {
        /// The ID of the tab to navigate.
        tab_id: u32,
        /// Number of steps to go forward (default 1).
        steps: u32,
    },
    /// Subscribe to a live stream of browser events.
    ///
    /// After sending this command the mediator streams [`BrowserEvent`] objects as
    /// newline-delimited JSON on the same connection until the client disconnects.
    /// No [`CliResponse`] is sent; events arrive directly as [`BrowserEvent`] JSON.
    SubscribeEvents,
    /// List all Firefox containers (contextual identities).
    ///
    /// Returns an empty list on browsers that don't support containers.
    ListContainers,
    /// Close a tab and reopen its URL in a different container.
    ///
    /// Firefox-only. The tab is closed and a new tab is created in the target
    /// container with the same URL.
    ReopenTabInContainer {
        /// The ID of the tab to reopen.
        tab_id: u32,
        /// The target container's cookie store ID.
        cookie_store_id: String,
    },
    /// List downloads, optionally filtered by state.
    ListDownloads {
        /// Filter by download state.
        #[serde(default)]
        state: Option<DownloadState>,
        /// Maximum number of results to return.
        #[serde(default)]
        limit: Option<u32>,
        /// Free-text search query matching URL and filename.
        #[serde(default)]
        query: Option<String>,
    },
    /// Start a new download.
    StartDownload {
        /// The URL to download.
        url: String,
        /// Filename relative to the downloads folder.
        #[serde(default)]
        filename: Option<String>,
        /// If `true`, show the Save As dialog.
        #[serde(default)]
        save_as: bool,
        /// How to handle filename conflicts.
        #[serde(default)]
        conflict_action: Option<FilenameConflictAction>,
    },
    /// Cancel an active download.
    CancelDownload {
        /// The download ID to cancel.
        download_id: u32,
    },
    /// Pause an active download.
    PauseDownload {
        /// The download ID to pause.
        download_id: u32,
    },
    /// Resume a paused download.
    ResumeDownload {
        /// The download ID to resume.
        download_id: u32,
    },
    /// Retry an interrupted download by re-downloading from the same URL.
    RetryDownload {
        /// The download ID to retry.
        download_id: u32,
    },
    /// Remove a download from the browser's download history (the file stays on disk).
    EraseDownload {
        /// The download ID to erase.
        download_id: u32,
    },
    /// Clear all downloads from the browser's history, optionally filtered by state.
    EraseAllDownloads {
        /// Only erase downloads in this state.
        #[serde(default)]
        state: Option<DownloadState>,
    },
}

/// A request sent from the CLI to the mediator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliRequest {
    /// A unique identifier (UUID v4 string) used to correlate requests with responses.
    pub request_id: String,
    /// The command to execute.
    ///
    /// Flattened so the command's `type` tag and fields appear at the top level of the JSON
    /// object alongside `request_id`, e.g. `{"request_id":"…","type":"ListWindows"}`.
    #[serde(flatten)]
    pub command: CliCommand,
}

/// The result payload of a successful command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CliResult {
    /// Browser information returned by `GetBrowserInfo`.
    BrowserInfo(BrowserInfo),
    /// Window list returned by `ListWindows`.
    Windows {
        /// The list of windows.
        windows: Vec<WindowSummary>,
    },
    /// ID of a newly created window returned by `OpenWindow`.
    WindowId {
        /// The new window's ID.
        window_id: u32,
    },
    /// Detailed tab list returned by `ListTabs`.
    Tabs {
        /// The list of tabs.
        tabs: Vec<TabDetails>,
    },
    /// Details of a newly created or moved tab.
    Tab(TabDetails),
    /// Container list returned by `ListContainers`.
    Containers {
        /// The list of containers.
        containers: Vec<ContainerInfo>,
    },
    /// Download list returned by `ListDownloads`.
    Downloads {
        /// The list of downloads.
        downloads: Vec<DownloadItem>,
    },
    /// ID of a newly started download returned by `StartDownload`.
    DownloadId {
        /// The new download's ID.
        download_id: u32,
    },
    /// Returned by commands that have no meaningful output.
    Unit,
}

/// The outcome of a command: either a successful result or an error message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "lowercase")]
pub enum CliOutcome {
    /// The command succeeded.
    Ok(CliResult),
    /// The command failed with this error message.
    Err(String),
}

/// A response sent from the mediator to the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliResponse {
    /// The request ID from the corresponding [`CliRequest`].
    pub request_id: String,
    /// The outcome of the command.
    pub outcome: CliOutcome,
}

/// Initial hello message sent from the Firefox extension to the mediator upon connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionHello {
    /// The name of the browser (e.g. "Firefox").
    pub browser_name: String,
    /// The browser vendor (e.g. "Mozilla").
    ///
    /// `None` on browsers that do not implement `browser.runtime.getBrowserInfo()`.
    #[serde(default)]
    pub browser_vendor: Option<String>,
    /// The browser version string (e.g. "120.0").
    pub browser_version: String,
}

/// A message received by the mediator from the Firefox extension via native messaging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "message_type")]
pub enum ExtensionMessage {
    /// Sent once by the extension when it connects, providing browser identity information.
    Hello(ExtensionHello),
    /// A response to a previously forwarded [`CliRequest`].
    Response(CliResponse),
    /// An unsolicited browser event pushed by the extension.
    Event {
        /// The browser event payload.
        event: BrowserEvent,
    },
}

impl std::fmt::Display for WindowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Minimized => write!(f, "minimized"),
            Self::Maximized => write!(f, "maximized"),
            Self::Fullscreen => write!(f, "fullscreen"),
        }
    }
}

impl std::fmt::Display for TabStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading => write!(f, "loading"),
            Self::Complete => write!(f, "complete"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::{CliCommand, CliOutcome, CliRequest, CliResponse, CliResult, ExtensionMessage};

    /// Verify that a `ListWindows` request round-trips through JSON correctly.
    #[test]
    #[expect(
        clippy::expect_used,
        reason = "panicking on unexpected failure is acceptable in tests"
    )]
    fn cli_request_list_windows_round_trip() {
        let request = CliRequest {
            request_id: "test-id-1".to_owned(),
            command: CliCommand::ListWindows,
        };
        let json = serde_json::to_string(&request)
            .expect("serialization should not fail for well-formed CliRequest");
        let decoded: CliRequest = serde_json::from_str(&json)
            .expect("deserialization should not fail for valid CliRequest JSON");
        pretty_assertions::assert_eq!(request, decoded);
    }

    /// Verify that an `Ok(Unit)` response round-trips through JSON correctly.
    #[test]
    #[expect(
        clippy::expect_used,
        reason = "panicking on unexpected failure is acceptable in tests"
    )]
    fn cli_response_ok_unit_round_trip() {
        let response = CliResponse {
            request_id: "test-id-2".to_owned(),
            outcome: CliOutcome::Ok(CliResult::Unit),
        };
        let json = serde_json::to_string(&response)
            .expect("serialization should not fail for well-formed CliResponse");
        let decoded: CliResponse = serde_json::from_str(&json)
            .expect("deserialization should not fail for valid CliResponse JSON");
        pretty_assertions::assert_eq!(response, decoded);
    }

    /// Verify that an `ExtensionMessage::Hello` round-trips through JSON correctly.
    #[test]
    #[expect(
        clippy::expect_used,
        reason = "panicking on unexpected failure is acceptable in tests"
    )]
    fn extension_hello_round_trip() {
        let msg = ExtensionMessage::Hello(super::ExtensionHello {
            browser_name: "Firefox".to_owned(),
            browser_vendor: Some("Mozilla".to_owned()),
            browser_version: "120.0".to_owned(),
        });
        let json = serde_json::to_string(&msg)
            .expect("serialization should not fail for well-formed ExtensionMessage::Hello");
        let decoded: ExtensionMessage = serde_json::from_str(&json)
            .expect("deserialization should not fail for valid ExtensionMessage JSON");
        pretty_assertions::assert_eq!(msg, decoded);
    }
}
