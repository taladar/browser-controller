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
    /// Zero-based position of the tab within its window.
    pub index: u32,
    /// The tab's title.
    pub title: String,
    /// The URL currently loaded in the tab.
    pub url: String,
    /// Whether this is the currently active (focused) tab in its window.
    pub is_active: bool,
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
    OpenWindow,
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
    /// Close a tab.
    CloseTab {
        /// The ID of the tab to close.
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
            browser_version: "120.0".to_owned(),
        });
        let json = serde_json::to_string(&msg)
            .expect("serialization should not fail for well-formed ExtensionMessage::Hello");
        let decoded: ExtensionMessage = serde_json::from_str(&json)
            .expect("deserialization should not fail for valid ExtensionMessage JSON");
        pretty_assertions::assert_eq!(msg, decoded);
    }
}
