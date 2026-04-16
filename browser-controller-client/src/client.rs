//! The main `Client` struct for communicating with a browser-controller mediator.

use std::path::{Path, PathBuf};
use std::time::Duration;

use browser_controller_types::{
    BrowserInfo, CliCommand, CliOutcome, CliRequest, CliResponse, CliResult, ContainerInfo,
    CookieStoreId, DownloadId, DownloadItem, DownloadState, FilenameConflictAction, Password,
    TabDetails, TabId, WindowId, WindowSummary,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

use crate::error::CommandError;
use crate::event_stream::{EventStream, EventStreamError};
use crate::matchers::{
    MatchError, MatchWith as _, MultipleMatchBehavior, TabMatcher, WindowMatcher,
};

/// Error from a simple command (no method-specific failure modes).
type CmdResult<T> = Result<T, CommandError<std::convert::Infallible>>;

/// Error from a resolve operation (adds [`MatchError`]).
type ResolveResult<T> = Result<T, CommandError<MatchError>>;

/// Errors that can occur when sending a command to the mediator.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum SendCommandError {
    /// Failed to connect to the mediator socket.
    #[error("failed to connect to mediator socket: {0}")]
    Connect(std::io::Error),
    /// Failed to send the command to the mediator.
    #[error("failed to send command to mediator: {0}")]
    Send(std::io::Error),
    /// Failed to read the response from the mediator.
    #[error("failed to read response from mediator: {0}")]
    Receive(std::io::Error),
    /// Failed to serialize the command.
    #[error("failed to serialize command: {0}")]
    Serialize(serde_json::Error),
    /// Failed to deserialize the response.
    #[error("failed to deserialize response {raw:?}: {source}")]
    Deserialize {
        /// The JSON parse error.
        source: serde_json::Error,
        /// The raw response string that failed to parse.
        raw: String,
    },
    /// The command returned an error from the browser/extension.
    #[error("command returned an error: {0}")]
    CommandRejected(String),
    /// The response contained an unknown outcome variant.
    #[error("unknown response outcome variant")]
    UnknownOutcome,
}

/// An async client for communicating with a browser-controller mediator.
///
/// Create a `Client` with the path to a mediator's Unix Domain Socket and a
/// command timeout, then use its typed methods to control the browser.
#[derive(Debug, Clone)]
pub struct Client {
    /// Path to the mediator's UDS socket (Unix) or pipe marker file (Windows).
    socket_path: PathBuf,
    /// Maximum time to wait for a command response.
    timeout: Duration,
}

/// Parameters for opening a new tab.
///
/// Use [`OpenTabParams::builder`] to construct, then pass to [`Client::open_tab`].
#[derive(Debug, Clone, derive_builder::Builder)]
#[builder(setter(into, strip_option))]
pub struct OpenTabParams {
    /// The window in which to open the tab.
    pub(crate) window_id: WindowId,
    /// Insert the new tab immediately before the tab with this ID.
    #[builder(default)]
    pub(crate) insert_before_tab_id: Option<TabId>,
    /// Insert the new tab immediately after the tab with this ID.
    #[builder(default)]
    pub(crate) insert_after_tab_id: Option<TabId>,
    /// URL to load in the new tab (defaults to the browser's new-tab page).
    #[builder(default)]
    pub(crate) url: Option<String>,
    /// Username for HTTP authentication.
    #[builder(default)]
    pub(crate) username: Option<String>,
    /// Password for HTTP authentication.
    #[builder(default)]
    pub(crate) password: Option<Password>,
    /// If `true`, the tab opens in the background.
    #[builder(default)]
    pub(crate) background: bool,
    /// Firefox container (cookie store) ID.
    #[builder(default)]
    pub(crate) cookie_store_id: Option<CookieStoreId>,
}

impl OpenTabParams {
    /// Create a builder for opening a tab in the given window.
    ///
    /// The `window_id` is required; all other fields default to `None`/`false`.
    #[must_use]
    pub fn builder(window_id: WindowId) -> OpenTabParamsBuilder {
        let mut b = OpenTabParamsBuilder::default();
        b.window_id(window_id);
        b
    }
}

impl Client {
    /// Create a new client connected to the mediator at the given socket path.
    ///
    /// Every command sent through this client will time out after `timeout`.
    #[must_use]
    pub const fn new(socket_path: PathBuf, timeout: Duration) -> Self {
        Self {
            socket_path,
            timeout,
        }
    }

    /// Return the socket path this client is configured to use.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Return the command timeout.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Send a command and apply the configured timeout.
    async fn execute(
        &self,
        command: CliCommand,
    ) -> Result<CliResult, CommandError<std::convert::Infallible>> {
        tokio::time::timeout(self.timeout, send_command(&self.socket_path, command))
            .await
            .map_err(|_elapsed| CommandError::Timeout)?
            .map_err(CommandError::from)
    }

    /// Send a command that is expected to return [`CliResult::Unit`].
    async fn execute_unit(
        &self,
        command: CliCommand,
    ) -> Result<(), CommandError<std::convert::Infallible>> {
        match self.execute(command).await? {
            CliResult::Unit => Ok(()),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Unit",
                actual: Box::new(other),
            }),
        }
    }

    /// Send a command that is expected to return [`CliResult::Tab`].
    async fn execute_expect_tab(
        &self,
        command: CliCommand,
    ) -> Result<TabDetails, CommandError<std::convert::Infallible>> {
        match self.execute(command).await? {
            CliResult::Tab(details) => Ok(details),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Tab",
                actual: Box::new(other),
            }),
        }
    }

    // ------------------------------------------------------------------
    // Browser info
    // ------------------------------------------------------------------

    /// Retrieve information about the connected browser instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn browser_info(&self) -> CmdResult<BrowserInfo> {
        match self.execute(CliCommand::GetBrowserInfo).await? {
            CliResult::BrowserInfo(info) => Ok(info),
            other => Err(CommandError::UnexpectedResponse {
                expected: "BrowserInfo",
                actual: Box::new(other),
            }),
        }
    }

    // ------------------------------------------------------------------
    // Windows
    // ------------------------------------------------------------------

    /// List all open browser windows with their tab summaries.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn list_windows(&self) -> CmdResult<Vec<WindowSummary>> {
        match self.execute(CliCommand::ListWindows).await? {
            CliResult::Windows { windows } => Ok(windows),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Windows",
                actual: Box::new(other),
            }),
        }
    }

    /// Open a new browser window.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn open_window(
        &self,
        title_prefix: Option<String>,
        incognito: bool,
    ) -> CmdResult<WindowId> {
        match self
            .execute(CliCommand::OpenWindow {
                title_prefix,
                incognito,
            })
            .await?
        {
            CliResult::WindowId { window_id } => Ok(window_id),
            other => Err(CommandError::UnexpectedResponse {
                expected: "WindowId",
                actual: Box::new(other),
            }),
        }
    }

    /// Close a browser window.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn close_window(&self, window_id: WindowId) -> CmdResult<()> {
        self.execute_unit(CliCommand::CloseWindow { window_id })
            .await
    }

    /// Set the title prefix (Firefox `titlePreface`) for a window.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn set_window_title_prefix(
        &self,
        window_id: WindowId,
        prefix: String,
    ) -> CmdResult<()> {
        self.execute_unit(CliCommand::SetWindowTitlePrefix { window_id, prefix })
            .await
    }

    /// Remove the title prefix from a window.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn remove_window_title_prefix(&self, window_id: WindowId) -> CmdResult<()> {
        self.execute_unit(CliCommand::RemoveWindowTitlePrefix { window_id })
            .await
    }

    // ------------------------------------------------------------------
    // Tabs
    // ------------------------------------------------------------------

    /// List all tabs in a window with full details.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn list_tabs(&self, window_id: WindowId) -> CmdResult<Vec<TabDetails>> {
        match self.execute(CliCommand::ListTabs { window_id }).await? {
            CliResult::Tabs { tabs } => Ok(tabs),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Tabs",
                actual: Box::new(other),
            }),
        }
    }

    /// Open a new tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn open_tab(&self, params: OpenTabParams) -> CmdResult<TabDetails> {
        match self
            .execute(CliCommand::OpenTab {
                window_id: params.window_id,
                insert_before_tab_id: params.insert_before_tab_id,
                insert_after_tab_id: params.insert_after_tab_id,
                url: params.url,
                username: params.username,
                password: params.password,
                background: params.background,
                cookie_store_id: params.cookie_store_id,
            })
            .await?
        {
            CliResult::Tab(details) => Ok(details),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Tab",
                actual: Box::new(other),
            }),
        }
    }

    /// Activate a tab, making it the focused tab in its window.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn activate_tab(&self, tab_id: TabId) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::ActivateTab { tab_id })
            .await
    }

    /// Navigate a tab to a new URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn navigate_tab(&self, tab_id: TabId, url: String) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::NavigateTab { tab_id, url })
            .await
    }

    /// Reload a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn reload_tab(&self, tab_id: TabId, bypass_cache: bool) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::ReloadTab {
            tab_id,
            bypass_cache,
        })
        .await
    }

    /// Close a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn close_tab(&self, tab_id: TabId) -> CmdResult<()> {
        self.execute_unit(CliCommand::CloseTab { tab_id }).await
    }

    /// Pin a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn pin_tab(&self, tab_id: TabId) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::PinTab { tab_id }).await
    }

    /// Unpin a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn unpin_tab(&self, tab_id: TabId) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::UnpinTab { tab_id })
            .await
    }

    /// Toggle Reader Mode for a tab (Firefox-only).
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn toggle_reader_mode(&self, tab_id: TabId) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::ToggleReaderMode { tab_id })
            .await
    }

    /// Discard a tab, unloading its content from memory without closing it.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn discard_tab(&self, tab_id: TabId) -> CmdResult<()> {
        self.execute_unit(CliCommand::DiscardTab { tab_id }).await
    }

    /// Warm up a discarded tab, loading its content without activating it.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn warmup_tab(&self, tab_id: TabId) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::WarmupTab { tab_id })
            .await
    }

    /// Mute a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn mute_tab(&self, tab_id: TabId) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::MuteTab { tab_id })
            .await
    }

    /// Unmute a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn unmute_tab(&self, tab_id: TabId) -> CmdResult<TabDetails> {
        self.execute_expect_tab(CliCommand::UnmuteTab { tab_id })
            .await
    }

    /// Move a tab to a new position within its window.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn move_tab(&self, tab_id: TabId, new_index: u32) -> CmdResult<TabDetails> {
        match self
            .execute(CliCommand::MoveTab { tab_id, new_index })
            .await?
        {
            CliResult::Tab(details) => Ok(details),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Tab",
                actual: Box::new(other),
            }),
        }
    }

    /// Navigate backward in a tab's session history.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn go_back(&self, tab_id: TabId, steps: u32) -> CmdResult<TabDetails> {
        match self.execute(CliCommand::GoBack { tab_id, steps }).await? {
            CliResult::Tab(details) => Ok(details),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Tab",
                actual: Box::new(other),
            }),
        }
    }

    /// Navigate forward in a tab's session history.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn go_forward(&self, tab_id: TabId, steps: u32) -> CmdResult<TabDetails> {
        match self
            .execute(CliCommand::GoForward { tab_id, steps })
            .await?
        {
            CliResult::Tab(details) => Ok(details),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Tab",
                actual: Box::new(other),
            }),
        }
    }

    /// Close a tab and reopen its URL in a different Firefox container.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn reopen_tab_in_container(
        &self,
        tab_id: TabId,
        cookie_store_id: CookieStoreId,
    ) -> CmdResult<TabDetails> {
        match self
            .execute(CliCommand::ReopenTabInContainer {
                tab_id,
                cookie_store_id,
            })
            .await?
        {
            CliResult::Tab(details) => Ok(details),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Tab",
                actual: Box::new(other),
            }),
        }
    }

    // ------------------------------------------------------------------
    // Containers
    // ------------------------------------------------------------------

    /// List all Firefox containers (contextual identities).
    ///
    /// Returns an empty list on browsers that don't support containers.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn list_containers(&self) -> CmdResult<Vec<ContainerInfo>> {
        match self.execute(CliCommand::ListContainers).await? {
            CliResult::Containers { containers } => Ok(containers),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Containers",
                actual: Box::new(other),
            }),
        }
    }

    // ------------------------------------------------------------------
    // Downloads
    // ------------------------------------------------------------------

    /// List downloads, optionally filtered by state.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn list_downloads(
        &self,
        state: Option<DownloadState>,
        limit: Option<u32>,
        query: Option<String>,
    ) -> CmdResult<Vec<DownloadItem>> {
        match self
            .execute(CliCommand::ListDownloads {
                state,
                limit,
                query,
            })
            .await?
        {
            CliResult::Downloads { downloads } => Ok(downloads),
            other => Err(CommandError::UnexpectedResponse {
                expected: "Downloads",
                actual: Box::new(other),
            }),
        }
    }

    /// Start a new download.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn start_download(
        &self,
        url: String,
        filename: Option<String>,
        save_as: bool,
        conflict_action: Option<FilenameConflictAction>,
    ) -> CmdResult<DownloadId> {
        match self
            .execute(CliCommand::StartDownload {
                url,
                filename,
                save_as,
                conflict_action,
            })
            .await?
        {
            CliResult::DownloadId { download_id } => Ok(download_id),
            other => Err(CommandError::UnexpectedResponse {
                expected: "DownloadId",
                actual: Box::new(other),
            }),
        }
    }

    /// Cancel an active download.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn cancel_download(&self, download_id: DownloadId) -> CmdResult<()> {
        self.execute_unit(CliCommand::CancelDownload { download_id })
            .await
    }

    /// Pause an active download.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn pause_download(&self, download_id: DownloadId) -> CmdResult<()> {
        self.execute_unit(CliCommand::PauseDownload { download_id })
            .await
    }

    /// Resume a paused download.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn resume_download(&self, download_id: DownloadId) -> CmdResult<()> {
        self.execute_unit(CliCommand::ResumeDownload { download_id })
            .await
    }

    /// Retry an interrupted download.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn retry_download(&self, download_id: DownloadId) -> CmdResult<()> {
        self.execute_unit(CliCommand::RetryDownload { download_id })
            .await
    }

    /// Remove a download from the browser's history (the file stays on disk).
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn erase_download(&self, download_id: DownloadId) -> CmdResult<()> {
        self.execute_unit(CliCommand::EraseDownload { download_id })
            .await
    }

    /// Clear all downloads from the browser's history, optionally filtered by state.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn erase_all_downloads(&self, state: Option<DownloadState>) -> CmdResult<()> {
        self.execute_unit(CliCommand::EraseAllDownloads { state })
            .await
    }

    // ------------------------------------------------------------------
    // Events
    // ------------------------------------------------------------------

    /// Subscribe to all browser events from the mediator.
    ///
    /// Returns an [`EventStream`] that yields events as they arrive.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or subscribe command fails.
    pub async fn subscribe_events(&self) -> Result<EventStream, EventStreamError> {
        EventStream::open(&self.socket_path, false, false).await
    }

    /// Subscribe to browser events with a category filter.
    ///
    /// When both `include_windows_tabs` and `include_downloads` are `false`,
    /// all event categories are delivered (same as [`subscribe_events`](Self::subscribe_events)).
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or subscribe command fails.
    pub async fn subscribe_events_filtered(
        &self,
        include_windows_tabs: bool,
        include_downloads: bool,
    ) -> Result<EventStream, EventStreamError> {
        EventStream::open(&self.socket_path, include_windows_tabs, include_downloads).await
    }

    // ------------------------------------------------------------------
    // Matcher-based resolution
    // ------------------------------------------------------------------

    /// Resolve a [`WindowMatcher`] to a list of matching window IDs.
    ///
    /// Sends `ListWindows` to the mediator, applies the matcher, and enforces
    /// [`MultipleMatchBehavior`].
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails, the regex is invalid, no window matches, or
    /// multiple windows match and the policy is abort.
    pub async fn resolve_windows(&self, matcher: &WindowMatcher) -> ResolveResult<Vec<WindowId>> {
        let windows = self.list_windows().await.map_err(|e| e.widen())?;
        let matched: Vec<WindowId> = windows
            .match_with(matcher)
            .map_err(CommandError::Other)?
            .iter()
            .map(|w| w.id)
            .collect();
        let criteria = matcher.to_string();
        match matched.len() {
            0 => Err(CommandError::Other(MatchError::NoMatchingWindow {
                criteria,
            })),
            1 => Ok(matched),
            count => match matcher.if_matches_multiple() {
                MultipleMatchBehavior::Abort => {
                    Err(CommandError::Other(MatchError::AmbiguousWindow {
                        count,
                        criteria,
                    }))
                }
                MultipleMatchBehavior::All => Ok(matched),
            },
        }
    }

    /// Resolve a [`WindowMatcher`] + [`TabMatcher`] to a list of matching tab IDs.
    ///
    /// The `window_matcher` determines which windows to search. An empty
    /// window matcher (the default) searches all windows. The `tab_matcher`
    /// filters tabs within those windows.
    ///
    /// # Errors
    ///
    /// Returns an error if any command fails, a regex is invalid, no tab matches, or
    /// multiple tabs match and the policy is abort.
    pub async fn resolve_tabs(
        &self,
        window_matcher: &WindowMatcher,
        tab_matcher: &TabMatcher,
    ) -> ResolveResult<Vec<TabId>> {
        let windows = self.list_windows().await.map_err(|e| e.widen())?;
        let matched_windows = windows
            .match_with(window_matcher)
            .map_err(CommandError::Other)?;

        let mut all_tabs: Vec<TabDetails> = Vec::new();
        for win in matched_windows {
            let tabs = self.list_tabs(win.id).await.map_err(|e| e.widen())?;
            all_tabs.extend(tabs);
        }

        let matched: Vec<TabId> = all_tabs
            .match_with(tab_matcher)
            .map_err(CommandError::Other)?
            .iter()
            .map(|t| t.id)
            .collect();
        let criteria = tab_matcher.to_string();
        match matched.len() {
            0 => Err(CommandError::Other(MatchError::NoMatchingTab { criteria })),
            1 => Ok(matched),
            count => match tab_matcher.if_matches_multiple() {
                MultipleMatchBehavior::Abort => {
                    Err(CommandError::Other(MatchError::AmbiguousTab {
                        count,
                        criteria,
                    }))
                }
                MultipleMatchBehavior::All => Ok(matched),
            },
        }
    }
}

/// Send a command to a mediator at the given socket path and return the result.
///
/// This is a standalone function used by both [`Client`] and discovery routines.
///
/// # Errors
///
/// Returns an error if the connection, serialization, or communication fails, or if the
/// command itself fails.
pub(crate) async fn send_command(
    socket_path: &Path,
    command: CliCommand,
) -> Result<CliResult, SendCommandError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let request = CliRequest::new(request_id.clone(), command);

    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(socket_path)
        .await
        .map_err(SendCommandError::Connect)?;
    #[cfg(windows)]
    let stream = {
        let pipe_name = crate::discovery::pipe_name_from_marker(socket_path)
            .map_err(SendCommandError::Connect)?;
        tokio::net::windows::named_pipe::ClientOptions::new()
            .open(&pipe_name)
            .map_err(SendCommandError::Connect)?
    };

    let (read_half, mut write_half) = tokio::io::split(stream);

    let mut json = serde_json::to_vec(&request).map_err(SendCommandError::Serialize)?;
    json.push(b'\n');
    write_half
        .write_all(&json)
        .await
        .map_err(SendCommandError::Send)?;

    let mut reader = tokio::io::BufReader::new(read_half);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(SendCommandError::Receive)?;

    let trimmed = line.trim_end().to_owned();
    let response: CliResponse =
        serde_json::from_str(&trimmed).map_err(|source| SendCommandError::Deserialize {
            source,
            raw: trimmed,
        })?;

    if response.request_id != request_id {
        tracing::warn!(
            expected = %request_id,
            received = %response.request_id,
            "Response request_id mismatch",
        );
    }

    match response.outcome {
        CliOutcome::Ok(result) => Ok(result),
        CliOutcome::Err(msg) => Err(SendCommandError::CommandRejected(msg)),
        _ => Err(SendCommandError::UnknownOutcome),
    }
}
