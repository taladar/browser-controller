//! Shared protocol types for the browser-controller system.
//!
//! This crate defines the data types used in communication between:
//! - The CLI and the mediator (over Unix Domain Socket, newline-delimited JSON)
//! - The mediator and the browser extension (via native messaging, length-prefixed JSON)

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Error type for invalid [`WindowId`] values.
///
/// Currently empty — all `u32` values are accepted. The `#[non_exhaustive]`
/// attribute allows adding validation variants in the future without a
/// semver-breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, thiserror::Error)]
pub enum InvalidWindowId {}

/// Browser-assigned window identifier.
///
/// A lightweight newtype around `u32` that prevents accidental misuse of
/// tab or download IDs where a window ID is expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowId(u32);

impl WindowId {
    /// Returns the underlying `u32` value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[expect(
    clippy::infallible_try_from,
    reason = "error type is intentionally empty now but #[non_exhaustive] to allow adding validation later without a semver break"
)]
impl TryFrom<u32> for WindowId {
    type Error = InvalidWindowId;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        Ok(Self(v))
    }
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for WindowId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u32>().map(Self)
    }
}

/// Error type for invalid [`TabId`] values.
///
/// Currently empty — all `u32` values are accepted. The `#[non_exhaustive]`
/// attribute allows adding validation variants in the future without a
/// semver-breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, thiserror::Error)]
pub enum InvalidTabId {}

/// Browser-assigned tab identifier.
///
/// A lightweight newtype around `u32` that prevents accidental misuse of
/// window or download IDs where a tab ID is expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TabId(u32);

impl TabId {
    /// Returns the underlying `u32` value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[expect(
    clippy::infallible_try_from,
    reason = "error type is intentionally empty now but #[non_exhaustive] to allow adding validation later without a semver break"
)]
impl TryFrom<u32> for TabId {
    type Error = InvalidTabId;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        Ok(Self(v))
    }
}

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for TabId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u32>().map(Self)
    }
}

/// Error type for invalid [`DownloadId`] values.
///
/// Currently empty — all `u32` values are accepted. The `#[non_exhaustive]`
/// attribute allows adding validation variants in the future without a
/// semver-breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, thiserror::Error)]
pub enum InvalidDownloadId {}

/// Browser-assigned download identifier.
///
/// A lightweight newtype around `u32` that prevents accidental misuse of
/// window or tab IDs where a download ID is expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DownloadId(u32);

impl DownloadId {
    /// Returns the underlying `u32` value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[expect(
    clippy::infallible_try_from,
    reason = "error type is intentionally empty now but #[non_exhaustive] to allow adding validation later without a semver break"
)]
impl TryFrom<u32> for DownloadId {
    type Error = InvalidDownloadId;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        Ok(Self(v))
    }
}

impl std::fmt::Display for DownloadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for DownloadId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u32>().map(Self)
    }
}

/// Error type for invalid [`CookieStoreId`] values.
///
/// Currently empty — all string values are accepted. The `#[non_exhaustive]`
/// attribute allows adding validation variants in the future without a
/// semver-breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, thiserror::Error)]
pub enum InvalidCookieStoreId {}

/// Firefox container (cookie store) identifier.
///
/// A lightweight newtype around `String` that prevents accidental misuse of
/// other string fields where a cookie store ID is expected.
/// Values are typically of the form `"firefox-container-1"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CookieStoreId(String);

impl CookieStoreId {
    /// Returns the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes `self` and returns the inner `String`.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

#[expect(
    clippy::infallible_try_from,
    reason = "error type is intentionally empty now but #[non_exhaustive] to allow adding validation later without a semver break"
)]
impl TryFrom<String> for CookieStoreId {
    type Error = InvalidCookieStoreId;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        Ok(Self(v))
    }
}

#[expect(
    clippy::infallible_try_from,
    reason = "error type is intentionally empty now but #[non_exhaustive] to allow adding validation later without a semver break"
)]
impl TryFrom<&str> for CookieStoreId {
    type Error = InvalidCookieStoreId;
    fn try_from(v: &str) -> Result<Self, Self::Error> {
        Ok(Self(v.to_owned()))
    }
}

impl std::fmt::Display for CookieStoreId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for CookieStoreId {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

impl AsRef<str> for CookieStoreId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Error type for invalid [`TabGroupId`] values.
///
/// Currently empty — all `u32` values are accepted. The `#[non_exhaustive]`
/// attribute allows adding validation variants in the future without a
/// semver-breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, thiserror::Error)]
pub enum InvalidTabGroupId {}

/// Chrome-assigned tab group identifier.
///
/// A lightweight newtype around `u32` that prevents accidental misuse of
/// other IDs where a tab group ID is expected. Chrome-only; Firefox does
/// not have a tab groups API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TabGroupId(u32);

impl TabGroupId {
    /// Returns the underlying `u32` value.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[expect(
    clippy::infallible_try_from,
    reason = "error type is intentionally empty now but #[non_exhaustive] to allow adding validation later without a semver break"
)]
impl TryFrom<u32> for TabGroupId {
    type Error = InvalidTabGroupId;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        Ok(Self(v))
    }
}

impl std::fmt::Display for TabGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for TabGroupId {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u32>().map(Self)
    }
}

/// A password that is zeroed from memory on drop.
///
/// The inner string is wrapped in [`Zeroizing`] so it is securely erased
/// when this value is dropped. [`Debug`] output redacts the value.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Password(Zeroizing<String>);

impl std::fmt::Debug for Password {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Password([REDACTED])")
    }
}

impl PartialEq for Password {
    fn eq(&self, other: &Self) -> bool {
        *self.0 == *other.0
    }
}

impl Eq for Password {}

impl From<String> for Password {
    fn from(s: String) -> Self {
        Self(Zeroizing::new(s))
    }
}

impl From<&str> for Password {
    fn from(s: &str) -> Self {
        Self(Zeroizing::new(s.to_owned()))
    }
}

impl std::ops::Deref for Password {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

/// Serde helper: deserializes `-1` as `None` and non-negative values as
/// `Some(u64)`; serializes `None` back to `-1`.
mod neg1_as_none {
    use serde::{Deserialize as _, Deserializer, Serializer};

    /// Serialize `None` as `-1` and `Some(v)` as the integer `v`.
    #[expect(clippy::ref_option, reason = "signature required by serde(with)")]
    pub(crate) fn serialize<S: Serializer>(value: &Option<u64>, ser: S) -> Result<S::Ok, S::Error> {
        match *value {
            Some(v) => ser.serialize_i64(i64::try_from(v).unwrap_or(i64::MAX)),
            None => ser.serialize_i64(-1),
        }
    }

    /// Deserialize a signed integer, mapping negative values to `None`.
    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Option<u64>, D::Error> {
        let v = i64::deserialize(de)?;
        if v < 0 {
            Ok(None)
        } else {
            Ok(Some(u64::try_from(v).unwrap_or(u64::MAX)))
        }
    }
}

/// Information about a running browser instance.
#[non_exhaustive]
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
    /// `None` when the profile cannot be determined from the browser's command line.
    #[serde(default)]
    pub profile_id: Option<String>,
}

impl BrowserInfo {
    /// Create a new `BrowserInfo`.
    #[must_use]
    pub const fn new(
        browser_name: String,
        browser_vendor: Option<String>,
        browser_version: String,
        pid: u32,
        profile_id: Option<String>,
    ) -> Self {
        Self {
            browser_name,
            browser_vendor,
            browser_version,
            pid,
            profile_id,
        }
    }
}

/// The visual state of a browser window.
#[non_exhaustive]
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

/// The type of a browser window.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowType {
    /// A regular browser window.
    Normal,
    /// A popup window (e.g. opened via `window.open()`).
    Popup,
    /// A panel window (Chrome-only, deprecated).
    Panel,
    /// A developer tools window.
    Devtools,
}

impl std::fmt::Display for WindowType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Popup => write!(f, "popup"),
            Self::Panel => write!(f, "panel"),
            Self::Devtools => write!(f, "devtools"),
        }
    }
}

/// The color of a Chrome tab group.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabGroupColor {
    /// Grey color.
    Grey,
    /// Blue color.
    Blue,
    /// Red color.
    Red,
    /// Yellow color.
    Yellow,
    /// Green color.
    Green,
    /// Pink color.
    Pink,
    /// Purple color.
    Purple,
    /// Cyan color.
    Cyan,
    /// Orange color.
    Orange,
}

impl std::fmt::Display for TabGroupColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Grey => write!(f, "grey"),
            Self::Blue => write!(f, "blue"),
            Self::Red => write!(f, "red"),
            Self::Yellow => write!(f, "yellow"),
            Self::Green => write!(f, "green"),
            Self::Pink => write!(f, "pink"),
            Self::Purple => write!(f, "purple"),
            Self::Cyan => write!(f, "cyan"),
            Self::Orange => write!(f, "orange"),
        }
    }
}

/// Information about a Chrome tab group.
///
/// Chrome-only; Firefox does not support tab groups.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabGroupInfo {
    /// The tab group's unique identifier.
    pub id: TabGroupId,
    /// The display title of the group (may be empty).
    pub title: String,
    /// The color of the group.
    pub color: TabGroupColor,
    /// Whether the group is visually collapsed.
    pub collapsed: bool,
    /// The window this group belongs to.
    pub window_id: WindowId,
}

impl TabGroupInfo {
    /// Create a new `TabGroupInfo`.
    #[must_use]
    pub const fn new(
        id: TabGroupId,
        title: String,
        color: TabGroupColor,
        collapsed: bool,
        window_id: WindowId,
    ) -> Self {
        Self {
            id,
            title,
            color,
            collapsed,
            window_id,
        }
    }
}

/// A brief summary of a tab, suitable for embedding in window listings.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSummary {
    /// The browser-assigned tab ID.
    pub id: TabId,
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
    pub cookie_store_id: Option<CookieStoreId>,
    /// The human-readable container name (e.g. "Work", "Personal").
    ///
    /// Firefox-specific; `None` on browsers that don't support containers.
    #[serde(default)]
    pub container_name: Option<String>,
    /// Whether this tab is open in a private/incognito window.
    #[serde(default)]
    pub incognito: bool,
}

impl TabSummary {
    /// Create a new `TabSummary`.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the browser's tabs.Tab API fields"
    )]
    #[must_use]
    pub const fn new(
        id: TabId,
        index: u32,
        title: String,
        url: String,
        is_active: bool,
        cookie_store_id: Option<CookieStoreId>,
        container_name: Option<String>,
        incognito: bool,
    ) -> Self {
        Self {
            id,
            index,
            title,
            url,
            is_active,
            cookie_store_id,
            container_name,
            incognito,
        }
    }
}

/// A summary of a browser window including its tabs.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSummary {
    /// The window's unique identifier within the browser.
    pub id: WindowId,
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
    /// The type of this window (normal, popup, panel, devtools).
    #[serde(default)]
    pub window_type: Option<WindowType>,
    /// Whether this window is in private/incognito mode.
    #[serde(default)]
    pub incognito: bool,
    /// Window width in pixels.
    #[serde(default)]
    pub width: Option<u32>,
    /// Window height in pixels.
    #[serde(default)]
    pub height: Option<u32>,
    /// Left edge of the window in pixels from the screen left.
    ///
    /// May be negative on multi-monitor setups where a monitor is to the left
    /// of the primary monitor's origin.
    #[serde(default)]
    pub left: Option<i32>,
    /// Top edge of the window in pixels from the screen top.
    ///
    /// May be negative on multi-monitor setups where a monitor is above the
    /// primary monitor's origin.
    #[serde(default)]
    pub top: Option<i32>,
    /// Brief summaries of the tabs open in this window.
    pub tabs: Vec<TabSummary>,
}

impl WindowSummary {
    /// Create a new `WindowSummary`.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the browser's windows.Window API fields"
    )]
    #[must_use]
    pub const fn new(
        id: WindowId,
        title: String,
        title_prefix: Option<String>,
        is_focused: bool,
        is_last_focused: bool,
        state: WindowState,
        window_type: Option<WindowType>,
        incognito: bool,
        width: Option<u32>,
        height: Option<u32>,
        left: Option<i32>,
        top: Option<i32>,
        tabs: Vec<TabSummary>,
    ) -> Self {
        Self {
            id,
            title,
            title_prefix,
            is_focused,
            is_last_focused,
            state,
            window_type,
            incognito,
            width,
            height,
            left,
            top,
            tabs,
        }
    }
}

/// The loading status of a tab.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabStatus {
    /// The tab is currently loading.
    Loading,
    /// The tab has finished loading.
    Complete,
    /// The tab has been discarded (unloaded from memory). Chrome-only.
    Unloaded,
}

/// The state of a download.
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
#[expect(
    clippy::struct_excessive_bools,
    reason = "DownloadItem mirrors the browser's DownloadItem API, which exposes each state as a separate boolean property"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadItem {
    /// Browser-assigned download ID.
    pub id: DownloadId,
    /// The URL that was downloaded.
    pub url: String,
    /// Absolute filesystem path where the file was saved.
    pub filename: String,
    /// Current state of the download.
    pub state: DownloadState,
    /// Bytes received so far.
    pub bytes_received: u64,
    /// Total file size in bytes, or `None` if unknown.
    #[serde(with = "neg1_as_none")]
    pub total_bytes: Option<u64>,
    /// Final file size in bytes, or `None` if unknown.
    #[serde(with = "neg1_as_none")]
    pub file_size: Option<u64>,
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
    /// Predicted completion time as an ISO 8601 timestamp string.
    #[serde(default)]
    pub estimated_end_time: Option<String>,
    /// Danger classification of the download (e.g. "safe", "file", "url", "uncommon", "malware").
    #[serde(default)]
    pub danger: Option<String>,
}

impl DownloadItem {
    /// Create a new `DownloadItem`.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the browser's DownloadItem API fields"
    )]
    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "mirrors the browser's DownloadItem API booleans"
    )]
    #[must_use]
    pub const fn new(
        id: DownloadId,
        url: String,
        filename: String,
        state: DownloadState,
        bytes_received: u64,
        total_bytes: Option<u64>,
        file_size: Option<u64>,
        error: Option<String>,
        start_time: String,
        end_time: Option<String>,
        paused: bool,
        can_resume: bool,
        exists: bool,
        mime: Option<String>,
        incognito: bool,
        estimated_end_time: Option<String>,
        danger: Option<String>,
    ) -> Self {
        Self {
            id,
            url,
            filename,
            state,
            bytes_received,
            total_bytes,
            file_size,
            error,
            start_time,
            end_time,
            paused,
            can_resume,
            exists,
            mime,
            incognito,
            estimated_end_time,
            danger,
        }
    }
}

/// Full details about a browser tab.
#[non_exhaustive]
#[expect(
    clippy::struct_excessive_bools,
    reason = "TabDetails mirrors the Firefox tabs.Tab API, which exposes each state as a separate boolean property"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabDetails {
    /// The tab's unique identifier within the browser.
    pub id: TabId,
    /// Zero-based position of the tab within its window.
    pub index: u32,
    /// The identifier of the window that contains this tab.
    pub window_id: WindowId,
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
    pub cookie_store_id: Option<CookieStoreId>,
    /// The human-readable container name (e.g. "Work", "Personal").
    ///
    /// Firefox-specific; `None` on browsers that don't support containers.
    #[serde(default)]
    pub container_name: Option<String>,
    /// The ID of the tab that opened this one, if any.
    ///
    /// `None` when the tab was not opened by another tab (e.g. opened via the
    /// address bar, bookmarks, or the `tabs open` command).
    #[serde(default)]
    pub opener_tab_id: Option<TabId>,
    /// Timestamp (milliseconds since epoch) of the last user interaction.
    ///
    /// Firefox-specific; `None` on browsers that don't track this.
    #[serde(default)]
    pub last_accessed: Option<u64>,
    /// Whether the browser can auto-discard this tab to save memory.
    ///
    /// Chrome-specific; `None` on browsers that don't support this.
    #[serde(default)]
    pub auto_discardable: Option<bool>,
    /// Tab group ID, or `None` if this tab is not in a group.
    ///
    /// Chrome-specific; `None` on browsers that don't support tab groups.
    #[serde(default)]
    pub group_id: Option<TabGroupId>,
}

impl TabDetails {
    /// Create a new `TabDetails`.
    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the browser's tabs.Tab API fields"
    )]
    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "mirrors the browser's tabs.Tab API booleans"
    )]
    #[must_use]
    pub const fn new(
        id: TabId,
        index: u32,
        window_id: WindowId,
        title: String,
        url: String,
        is_active: bool,
        is_pinned: bool,
        is_discarded: bool,
        is_audible: bool,
        is_muted: bool,
        status: TabStatus,
        has_attention: bool,
        is_awaiting_auth: bool,
        is_in_reader_mode: bool,
        incognito: bool,
        history_length: u32,
        history_steps_back: Option<u32>,
        history_steps_forward: Option<u32>,
        history_hidden_count: Option<u32>,
        cookie_store_id: Option<CookieStoreId>,
        container_name: Option<String>,
        opener_tab_id: Option<TabId>,
        last_accessed: Option<u64>,
        auto_discardable: Option<bool>,
        group_id: Option<TabGroupId>,
    ) -> Self {
        Self {
            id,
            index,
            window_id,
            title,
            url,
            is_active,
            is_pinned,
            is_discarded,
            is_audible,
            is_muted,
            status,
            has_attention,
            is_awaiting_auth,
            is_in_reader_mode,
            incognito,
            history_length,
            history_steps_back,
            history_steps_forward,
            history_hidden_count,
            cookie_store_id,
            container_name,
            opener_tab_id,
            last_accessed,
            auto_discardable,
            group_id,
        }
    }
}

/// Information about a Firefox container (contextual identity).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerInfo {
    /// The cookie store ID (e.g. `"firefox-container-1"`).
    pub cookie_store_id: CookieStoreId,
    /// Human-readable name (e.g. `"Work"`).
    pub name: String,
    /// Color identifier (e.g. `"blue"`).
    pub color: String,
    /// Hex color code (e.g. `"#37adff"`).
    pub color_code: String,
    /// Icon identifier (e.g. `"briefcase"`).
    pub icon: String,
}

impl ContainerInfo {
    /// Create a new `ContainerInfo`.
    #[must_use]
    pub const fn new(
        cookie_store_id: CookieStoreId,
        name: String,
        color: String,
        color_code: String,
        icon: String,
    ) -> Self {
        Self {
            cookie_store_id,
            name,
            color,
            color_code,
            icon,
        }
    }
}

/// An event emitted by the browser extension and broadcast to all event-stream subscribers.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BrowserEvent {
    /// A new browser window was opened.
    WindowOpened {
        /// The new window's ID.
        window_id: WindowId,
        /// The window's title at the time it was created (may be empty).
        title: String,
    },
    /// A browser window was closed.
    WindowClosed {
        /// The ID of the closed window.
        window_id: WindowId,
    },
    /// The active tab in a window changed.
    TabActivated {
        /// The window containing the newly active tab.
        window_id: WindowId,
        /// The ID of the newly active tab.
        tab_id: TabId,
        /// The ID of the previously active tab, if any.
        #[serde(default)]
        previous_tab_id: Option<TabId>,
    },
    /// A new tab was opened.
    TabOpened {
        /// The new tab's ID.
        tab_id: TabId,
        /// The window containing the new tab.
        window_id: WindowId,
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
        tab_id: TabId,
        /// The window that contained the tab.
        window_id: WindowId,
        /// Whether the tab was closed because its parent window was also closing.
        is_window_closing: bool,
    },
    /// A tab started loading a new URL.
    TabNavigated {
        /// The ID of the navigating tab.
        tab_id: TabId,
        /// The window containing the tab.
        window_id: WindowId,
        /// The new URL.
        url: String,
    },
    /// A tab's title changed.
    TabTitleChanged {
        /// The ID of the tab.
        tab_id: TabId,
        /// The window containing the tab.
        window_id: WindowId,
        /// The new title.
        title: String,
    },
    /// A tab's loading status changed (e.g. from `loading` to `complete`).
    TabStatusChanged {
        /// The ID of the tab.
        tab_id: TabId,
        /// The window containing the tab.
        window_id: WindowId,
        /// The new loading status.
        status: TabStatus,
    },
    /// A new download was started.
    DownloadCreated {
        /// The download's ID.
        download_id: DownloadId,
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
        download_id: DownloadId,
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
        download_id: DownloadId,
    },
    /// A tab was moved to a new position within its window.
    TabMoved {
        /// The ID of the moved tab.
        tab_id: TabId,
        /// The window containing the tab.
        window_id: WindowId,
        /// The previous zero-based index.
        from_index: u32,
        /// The new zero-based index.
        to_index: u32,
    },
    /// A tab was attached to a window (moved from another window).
    TabAttached {
        /// The ID of the attached tab.
        tab_id: TabId,
        /// The window the tab was attached to.
        new_window_id: WindowId,
        /// The tab's new zero-based index in the window.
        new_index: u32,
    },
    /// A tab was detached from a window (being moved to another window).
    TabDetached {
        /// The ID of the detached tab.
        tab_id: TabId,
        /// The window the tab was detached from.
        old_window_id: WindowId,
        /// The tab's old zero-based index in the window.
        old_index: u32,
    },
    /// The focused window changed.
    WindowFocusChanged {
        /// The newly focused window ID, or `None` if all windows lost focus
        /// (e.g. the user switched to another application).
        #[serde(default)]
        window_id: Option<WindowId>,
    },
    /// A tab group was created. Chrome-only.
    TabGroupCreated {
        /// The new group's ID.
        group_id: TabGroupId,
        /// The window containing the group.
        window_id: WindowId,
        /// The group's display title.
        title: String,
        /// The group's color.
        color: String,
        /// Whether the group is collapsed.
        collapsed: bool,
    },
    /// A tab group's properties changed. Chrome-only.
    TabGroupUpdated {
        /// The updated group's ID.
        group_id: TabGroupId,
        /// The window containing the group.
        window_id: WindowId,
        /// The group's display title.
        title: String,
        /// The group's color.
        color: String,
        /// Whether the group is collapsed.
        collapsed: bool,
    },
    /// A tab group was removed. Chrome-only.
    TabGroupRemoved {
        /// The removed group's ID.
        group_id: TabGroupId,
        /// The window that contained the group.
        window_id: WindowId,
    },
    /// An uncaught error or unhandled promise rejection occurred in the
    /// extension's service worker.
    ///
    /// These are forwarded from the extension's global error handlers so they
    /// are visible in the mediator log and event stream.
    ExtensionError {
        /// Error category (e.g. `"uncaught_error"`, `"unhandled_rejection"`).
        kind: String,
        /// Human-readable error message.
        message: String,
        /// Stack trace or additional context.
        #[serde(default)]
        detail: String,
    },
    /// Some events were lost because the consumer could not keep up.
    ///
    /// This is a synthetic event generated by the mediator, not the browser.
    /// The consumer should assume that any cached state may be stale and
    /// re-query if needed.
    EventsLost {
        /// The number of events that were dropped.
        count: u64,
    },
}

impl BrowserEvent {
    /// Returns `true` if this is a download-related event.
    #[must_use]
    pub const fn is_download_event(&self) -> bool {
        matches!(
            self,
            Self::DownloadCreated { .. }
                | Self::DownloadChanged { .. }
                | Self::DownloadErased { .. }
        )
    }

    /// Returns `true` if this event passes the given subscription filter.
    ///
    /// When both `include_windows_tabs` and `include_downloads` are `false`,
    /// all events pass (backward compatible "no filter" mode).
    #[must_use]
    pub const fn matches_filter(
        &self,
        include_windows_tabs: bool,
        include_downloads: bool,
    ) -> bool {
        // No filter flags → deliver everything.
        if !include_windows_tabs && !include_downloads {
            return true;
        }
        if self.is_download_event() {
            include_downloads
        } else {
            include_windows_tabs
        }
    }
}

/// A command sent from the CLI to the mediator, and forwarded to the extension.
#[non_exhaustive]
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
        /// Firefox-only; returns an error on other browsers when set.
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
        window_id: WindowId,
    },
    /// Set the title prefix (Firefox `titlePreface`) for a window.
    ///
    /// Firefox-only. Returns an error on browsers that do not support
    /// `titlePreface`.
    SetWindowTitlePrefix {
        /// The ID of the window whose prefix to set.
        window_id: WindowId,
        /// The prefix string to prepend to the window title.
        prefix: String,
    },
    /// Remove the title prefix from a window, restoring the default title.
    ///
    /// Firefox-only. Returns an error on browsers that do not support
    /// `titlePreface`.
    RemoveWindowTitlePrefix {
        /// The ID of the window whose prefix to remove.
        window_id: WindowId,
    },
    /// List all tabs in a window with full details.
    ListTabs {
        /// The ID of the window whose tabs to list.
        window_id: WindowId,
    },
    /// Open a new tab in a window.
    OpenTab {
        /// The ID of the window in which to open the tab.
        window_id: WindowId,
        /// If set, the new tab will be inserted immediately before the tab with this ID.
        insert_before_tab_id: Option<TabId>,
        /// If set, the new tab will be inserted immediately after the tab with this ID.
        insert_after_tab_id: Option<TabId>,
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
        password: Option<Password>,
        /// If `true`, the new tab is created in the background and the currently active tab
        /// in the window remains active.
        #[serde(default)]
        background: bool,
        /// Firefox container (cookie store) ID to open the tab in.
        /// Firefox-only; returns an error on browsers without container support.
        ///
        /// E.g. `"firefox-container-1"`.
        #[serde(default)]
        cookie_store_id: Option<CookieStoreId>,
        /// Optional timeout in milliseconds to wait for the tab to finish loading
        /// before returning.
        ///
        /// When set, the extension waits for `tabs.onUpdated` to report
        /// `status: "complete"` for this tab, up to the given timeout. If the
        /// timeout elapses, the tab details are returned in whatever state they
        /// are in. When `None`, the tab details are returned immediately after
        /// creation without waiting.
        #[serde(default)]
        wait_for_load_timeout_ms: Option<u32>,
    },
    /// Activate a tab, making it the focused tab in its window.
    ActivateTab {
        /// The ID of the tab to activate.
        tab_id: TabId,
    },
    /// Navigate an existing tab to a new URL.
    NavigateTab {
        /// The ID of the tab to navigate.
        tab_id: TabId,
        /// The URL to load in the tab.
        url: String,
    },
    /// Reload a tab.
    ReloadTab {
        /// The ID of the tab to reload.
        tab_id: TabId,
        /// If `true`, bypass the browser cache (hard refresh).
        #[serde(default)]
        bypass_cache: bool,
    },
    /// Close a tab.
    CloseTab {
        /// The ID of the tab to close.
        tab_id: TabId,
    },
    /// Pin a tab.
    PinTab {
        /// The ID of the tab to pin.
        tab_id: TabId,
    },
    /// Unpin a tab.
    UnpinTab {
        /// The ID of the tab to unpin.
        tab_id: TabId,
    },
    /// Toggle Reader Mode for a tab.
    ///
    /// Firefox-only. Switches the tab into or out of Reader Mode. The tab
    /// must be displaying a page that Firefox considers reader-mode compatible.
    ToggleReaderMode {
        /// The ID of the tab whose Reader Mode to toggle.
        tab_id: TabId,
    },
    /// Discard a tab, unloading its content from memory without closing it.
    ///
    /// The tab remains in the tab strip but its content is freed. It will be
    /// reloaded when activated. Cannot discard the active tab.
    DiscardTab {
        /// The ID of the tab to discard.
        tab_id: TabId,
    },
    /// Warm up a discarded tab, loading its content into memory without activating it.
    ///
    /// Firefox-only. Uses `tabs.warmup()` which is not available on Chrome.
    WarmupTab {
        /// The ID of the tab to warm up.
        tab_id: TabId,
    },
    /// Mute a tab, suppressing any audio it produces.
    MuteTab {
        /// The ID of the tab to mute.
        tab_id: TabId,
    },
    /// Unmute a tab, allowing it to produce audio again.
    UnmuteTab {
        /// The ID of the tab to unmute.
        tab_id: TabId,
    },
    /// Move a tab to a new position within its window.
    MoveTab {
        /// The ID of the tab to move.
        tab_id: TabId,
        /// The new zero-based index for the tab within its window.
        new_index: u32,
    },
    /// Navigate backward in a tab's session history.
    ///
    /// Returns a [`CliResult::Tab`] with the details of the page navigated to,
    /// or the current tab state if the history boundary was already reached.
    GoBack {
        /// The ID of the tab to navigate.
        tab_id: TabId,
        /// Number of steps to go back (default 1).
        steps: u32,
    },
    /// Navigate forward in a tab's session history.
    ///
    /// Returns a [`CliResult::Tab`] with the details of the page navigated to,
    /// or the current tab state if the history boundary was already reached.
    GoForward {
        /// The ID of the tab to navigate.
        tab_id: TabId,
        /// Number of steps to go forward (default 1).
        steps: u32,
    },
    /// Subscribe to a live stream of browser events.
    ///
    /// After sending this command the mediator streams [`BrowserEvent`] objects as
    /// newline-delimited JSON on the same connection until the client disconnects.
    /// No [`CliResponse`] is sent; events arrive directly as [`BrowserEvent`] JSON.
    ///
    /// When both `include_windows_tabs` and `include_downloads` are `false`
    /// (the default), all event categories are delivered (backward compatible).
    SubscribeEvents {
        /// Include window and tab events (`WindowOpened`, `WindowClosed`,
        /// `TabOpened`, `TabClosed`, `TabActivated`, `TabNavigated`,
        /// `TabTitleChanged`, `TabStatusChanged`).
        #[serde(default)]
        include_windows_tabs: bool,
        /// Include download events (`DownloadCreated`, `DownloadChanged`,
        /// `DownloadErased`).
        #[serde(default)]
        include_downloads: bool,
    },
    /// List all Firefox containers (contextual identities).
    ///
    /// Firefox-only. Returns an error on browsers that do not support
    /// contextual identities.
    ListContainers,
    /// Close a tab and reopen its URL in a different container.
    ///
    /// Firefox-only. The tab is closed and a new tab is created in the target
    /// container with the same URL.
    ReopenTabInContainer {
        /// The ID of the tab to reopen.
        tab_id: TabId,
        /// The target container's cookie store ID.
        cookie_store_id: CookieStoreId,
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
        download_id: DownloadId,
    },
    /// Pause an active download.
    PauseDownload {
        /// The download ID to pause.
        download_id: DownloadId,
    },
    /// Resume a paused download.
    ResumeDownload {
        /// The download ID to resume.
        download_id: DownloadId,
    },
    /// Retry an interrupted download by re-downloading from the same URL.
    RetryDownload {
        /// The download ID to retry.
        download_id: DownloadId,
    },
    /// Remove a download from the browser's download history (the file stays on disk).
    EraseDownload {
        /// The download ID to erase.
        download_id: DownloadId,
    },
    /// Clear all downloads from the browser's history, optionally filtered by state.
    EraseAllDownloads {
        /// Only erase downloads in this state.
        #[serde(default)]
        state: Option<DownloadState>,
    },
    /// List all tab groups, optionally filtered by window.
    ///
    /// Chrome-only. Returns an error on browsers that do not support tab groups.
    ListTabGroups {
        /// Only list groups in this window.
        #[serde(default)]
        window_id: Option<WindowId>,
    },
    /// Get a single tab group by ID.
    ///
    /// Chrome-only.
    GetTabGroup {
        /// The ID of the group to retrieve.
        group_id: TabGroupId,
    },
    /// Update a tab group's properties.
    ///
    /// Chrome-only.
    UpdateTabGroup {
        /// The ID of the group to update.
        group_id: TabGroupId,
        /// New title for the group.
        #[serde(default)]
        title: Option<String>,
        /// New color for the group.
        #[serde(default)]
        color: Option<TabGroupColor>,
        /// New collapsed state for the group.
        #[serde(default)]
        collapsed: Option<bool>,
    },
    /// Move a tab group to a new position.
    ///
    /// Chrome-only.
    MoveTabGroup {
        /// The ID of the group to move.
        group_id: TabGroupId,
        /// The new zero-based index for the group.
        index: u32,
        /// Move the group to a different window.
        #[serde(default)]
        window_id: Option<WindowId>,
    },
    /// Add tabs to a tab group, optionally creating a new group.
    ///
    /// Chrome-only.
    GroupTabs {
        /// The tab IDs to group.
        tab_ids: Vec<TabId>,
        /// The group to add the tabs to. If `None`, a new group is created.
        #[serde(default)]
        group_id: Option<TabGroupId>,
    },
    /// Remove tabs from their tab groups.
    ///
    /// Chrome-only.
    UngroupTabs {
        /// The tab IDs to ungroup.
        tab_ids: Vec<TabId>,
    },
}

/// A request sent from the CLI to the mediator.
#[non_exhaustive]
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

impl CliRequest {
    /// Create a new `CliRequest`.
    #[must_use]
    pub const fn new(request_id: String, command: CliCommand) -> Self {
        Self {
            request_id,
            command,
        }
    }
}

/// The result payload of a successful command.
#[non_exhaustive]
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
        window_id: WindowId,
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
        download_id: DownloadId,
    },
    /// Tab group list returned by `ListTabGroups`.
    TabGroups {
        /// The list of tab groups.
        tab_groups: Vec<TabGroupInfo>,
    },
    /// Details of a single tab group.
    TabGroup(TabGroupInfo),
    /// Returned by commands that have no meaningful output.
    Unit,
}

/// The outcome of a command: either a successful result or an error message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "lowercase")]
pub enum CliOutcome {
    /// The command succeeded.
    Ok(CliResult),
    /// The command failed with this error message.
    Err(String),
}

/// A response sent from the mediator to the CLI.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliResponse {
    /// The request ID from the corresponding [`CliRequest`].
    pub request_id: String,
    /// The outcome of the command.
    pub outcome: CliOutcome,
}

impl CliResponse {
    /// Create a new `CliResponse`.
    #[must_use]
    pub const fn new(request_id: String, outcome: CliOutcome) -> Self {
        Self {
            request_id,
            outcome,
        }
    }
}

/// Initial hello message sent from the browser extension to the mediator upon connection.
#[non_exhaustive]
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

impl ExtensionHello {
    /// Create a new `ExtensionHello`.
    #[must_use]
    pub const fn new(
        browser_name: String,
        browser_vendor: Option<String>,
        browser_version: String,
    ) -> Self {
        Self {
            browser_name,
            browser_vendor,
            browser_version,
        }
    }
}

/// A message received by the mediator from the browser extension via native messaging.
#[non_exhaustive]
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
            Self::Unloaded => write!(f, "unloaded"),
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
