//! The main `Client` struct for communicating with a browser-controller mediator.

use std::path::{Path, PathBuf};
use std::time::Duration;

use browser_controller_types::{
    BrowserInfo, CliCommand, CliOutcome, CliRequest, CliResponse, CliResult, ContainerInfo,
    CookieStoreId, DownloadId, DownloadItem, DownloadState, FilenameConflictAction, TabDetails,
    TabId, WindowId, WindowSummary,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

use crate::Error;
use crate::event_stream::EventStream;
use crate::matchers::{
    MultipleMatchBehavior, TabMatcher, WindowMatcher, match_tabs, match_windows,
};

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
/// Use [`OpenTabParams::new`] with the target window ID, then chain builder
/// methods for optional fields before passing to [`Client::open_tab`].
#[derive(Debug, Clone)]
pub struct OpenTabParams {
    /// The window in which to open the tab.
    pub window_id: WindowId,
    /// Insert the new tab immediately before the tab with this ID.
    pub insert_before_tab_id: Option<TabId>,
    /// Insert the new tab immediately after the tab with this ID.
    pub insert_after_tab_id: Option<TabId>,
    /// URL to load in the new tab (defaults to the browser's new-tab page).
    pub url: Option<String>,
    /// Username for HTTP authentication.
    pub username: Option<String>,
    /// Password for HTTP authentication.
    pub password: Option<String>,
    /// If `true`, the tab opens in the background.
    pub background: bool,
    /// Firefox container (cookie store) ID.
    pub cookie_store_id: Option<CookieStoreId>,
}

impl OpenTabParams {
    /// Create parameters for opening a tab in the given window.
    #[must_use]
    pub const fn new(window_id: WindowId) -> Self {
        Self {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: None,
            username: None,
            password: None,
            background: false,
            cookie_store_id: None,
        }
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
    async fn execute(&self, command: CliCommand) -> Result<CliResult, Error> {
        tokio::time::timeout(self.timeout, send_command(&self.socket_path, command))
            .await
            .map_err(|_elapsed| Error::Timeout)?
    }

    /// Send a command that is expected to return [`CliResult::Unit`].
    async fn execute_unit(&self, command: CliCommand) -> Result<(), Error> {
        match self.execute(command).await? {
            CliResult::Unit => Ok(()),
            other => Err(Error::UnexpectedResponse {
                expected: "Unit",
                actual: Box::new(other),
            }),
        }
    }

    /// Send a command that is expected to return [`CliResult::Tab`].
    async fn execute_expect_tab(&self, command: CliCommand) -> Result<TabDetails, Error> {
        match self.execute(command).await? {
            CliResult::Tab(details) => Ok(details),
            other => Err(Error::UnexpectedResponse {
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
    pub async fn browser_info(&self) -> Result<BrowserInfo, Error> {
        match self.execute(CliCommand::GetBrowserInfo).await? {
            CliResult::BrowserInfo(info) => Ok(info),
            other => Err(Error::UnexpectedResponse {
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
    pub async fn list_windows(&self) -> Result<Vec<WindowSummary>, Error> {
        match self.execute(CliCommand::ListWindows).await? {
            CliResult::Windows { windows } => Ok(windows),
            other => Err(Error::UnexpectedResponse {
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
    ) -> Result<WindowId, Error> {
        match self
            .execute(CliCommand::OpenWindow {
                title_prefix,
                incognito,
            })
            .await?
        {
            CliResult::WindowId { window_id } => Ok(window_id),
            other => Err(Error::UnexpectedResponse {
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
    pub async fn close_window(&self, window_id: WindowId) -> Result<(), Error> {
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
    ) -> Result<(), Error> {
        self.execute_unit(CliCommand::SetWindowTitlePrefix { window_id, prefix })
            .await
    }

    /// Remove the title prefix from a window.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn remove_window_title_prefix(&self, window_id: WindowId) -> Result<(), Error> {
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
    pub async fn list_tabs(&self, window_id: WindowId) -> Result<Vec<TabDetails>, Error> {
        match self.execute(CliCommand::ListTabs { window_id }).await? {
            CliResult::Tabs { tabs } => Ok(tabs),
            other => Err(Error::UnexpectedResponse {
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
    pub async fn open_tab(&self, params: OpenTabParams) -> Result<TabDetails, Error> {
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
            other => Err(Error::UnexpectedResponse {
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
    pub async fn activate_tab(&self, tab_id: TabId) -> Result<TabDetails, Error> {
        self.execute_expect_tab(CliCommand::ActivateTab { tab_id })
            .await
    }

    /// Navigate a tab to a new URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn navigate_tab(&self, tab_id: TabId, url: String) -> Result<TabDetails, Error> {
        self.execute_expect_tab(CliCommand::NavigateTab { tab_id, url })
            .await
    }

    /// Reload a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn reload_tab(&self, tab_id: TabId, bypass_cache: bool) -> Result<TabDetails, Error> {
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
    pub async fn close_tab(&self, tab_id: TabId) -> Result<(), Error> {
        self.execute_unit(CliCommand::CloseTab { tab_id }).await
    }

    /// Pin a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn pin_tab(&self, tab_id: TabId) -> Result<TabDetails, Error> {
        self.execute_expect_tab(CliCommand::PinTab { tab_id }).await
    }

    /// Unpin a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn unpin_tab(&self, tab_id: TabId) -> Result<TabDetails, Error> {
        self.execute_expect_tab(CliCommand::UnpinTab { tab_id })
            .await
    }

    /// Toggle Reader Mode for a tab (Firefox-only).
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn toggle_reader_mode(&self, tab_id: TabId) -> Result<(), Error> {
        self.execute_unit(CliCommand::ToggleReaderMode { tab_id })
            .await
    }

    /// Discard a tab, unloading its content from memory without closing it.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn discard_tab(&self, tab_id: TabId) -> Result<(), Error> {
        self.execute_unit(CliCommand::DiscardTab { tab_id }).await
    }

    /// Warm up a discarded tab, loading its content without activating it.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn warmup_tab(&self, tab_id: TabId) -> Result<(), Error> {
        self.execute_unit(CliCommand::WarmupTab { tab_id }).await
    }

    /// Mute a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn mute_tab(&self, tab_id: TabId) -> Result<TabDetails, Error> {
        self.execute_expect_tab(CliCommand::MuteTab { tab_id })
            .await
    }

    /// Unmute a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn unmute_tab(&self, tab_id: TabId) -> Result<TabDetails, Error> {
        self.execute_expect_tab(CliCommand::UnmuteTab { tab_id })
            .await
    }

    /// Move a tab to a new position within its window.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn move_tab(&self, tab_id: TabId, new_index: u32) -> Result<TabDetails, Error> {
        match self
            .execute(CliCommand::MoveTab { tab_id, new_index })
            .await?
        {
            CliResult::Tab(details) => Ok(details),
            other => Err(Error::UnexpectedResponse {
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
    pub async fn go_back(&self, tab_id: TabId, steps: u32) -> Result<TabDetails, Error> {
        match self.execute(CliCommand::GoBack { tab_id, steps }).await? {
            CliResult::Tab(details) => Ok(details),
            other => Err(Error::UnexpectedResponse {
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
    pub async fn go_forward(&self, tab_id: TabId, steps: u32) -> Result<TabDetails, Error> {
        match self
            .execute(CliCommand::GoForward { tab_id, steps })
            .await?
        {
            CliResult::Tab(details) => Ok(details),
            other => Err(Error::UnexpectedResponse {
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
    ) -> Result<TabDetails, Error> {
        match self
            .execute(CliCommand::ReopenTabInContainer {
                tab_id,
                cookie_store_id,
            })
            .await?
        {
            CliResult::Tab(details) => Ok(details),
            other => Err(Error::UnexpectedResponse {
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
    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>, Error> {
        match self.execute(CliCommand::ListContainers).await? {
            CliResult::Containers { containers } => Ok(containers),
            other => Err(Error::UnexpectedResponse {
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
    ) -> Result<Vec<DownloadItem>, Error> {
        match self
            .execute(CliCommand::ListDownloads {
                state,
                limit,
                query,
            })
            .await?
        {
            CliResult::Downloads { downloads } => Ok(downloads),
            other => Err(Error::UnexpectedResponse {
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
    ) -> Result<DownloadId, Error> {
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
            other => Err(Error::UnexpectedResponse {
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
    pub async fn cancel_download(&self, download_id: DownloadId) -> Result<(), Error> {
        self.execute_unit(CliCommand::CancelDownload { download_id })
            .await
    }

    /// Pause an active download.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn pause_download(&self, download_id: DownloadId) -> Result<(), Error> {
        self.execute_unit(CliCommand::PauseDownload { download_id })
            .await
    }

    /// Resume a paused download.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn resume_download(&self, download_id: DownloadId) -> Result<(), Error> {
        self.execute_unit(CliCommand::ResumeDownload { download_id })
            .await
    }

    /// Retry an interrupted download.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn retry_download(&self, download_id: DownloadId) -> Result<(), Error> {
        self.execute_unit(CliCommand::RetryDownload { download_id })
            .await
    }

    /// Remove a download from the browser's history (the file stays on disk).
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn erase_download(&self, download_id: DownloadId) -> Result<(), Error> {
        self.execute_unit(CliCommand::EraseDownload { download_id })
            .await
    }

    /// Clear all downloads from the browser's history, optionally filtered by state.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails.
    pub async fn erase_all_downloads(&self, state: Option<DownloadState>) -> Result<(), Error> {
        self.execute_unit(CliCommand::EraseAllDownloads { state })
            .await
    }

    // ------------------------------------------------------------------
    // Events
    // ------------------------------------------------------------------

    /// Subscribe to browser events from the mediator.
    ///
    /// Returns an [`EventStream`] that yields events as they arrive.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or subscribe command fails.
    pub async fn subscribe_events(&self) -> Result<EventStream, Error> {
        EventStream::open(&self.socket_path).await
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
    pub async fn resolve_windows(&self, matcher: &WindowMatcher) -> Result<Vec<WindowId>, Error> {
        let windows = self.list_windows().await?;
        let matched = match_windows(&windows, matcher)?;
        let criteria = matcher.to_string();
        match matched.len() {
            0 => Err(Error::NoMatchingWindow { criteria }),
            1 => Ok(matched),
            count => match matcher.if_matches_multiple {
                MultipleMatchBehavior::Abort => Err(Error::AmbiguousWindow { count, criteria }),
                MultipleMatchBehavior::All => Ok(matched),
            },
        }
    }

    /// Resolve a [`TabMatcher`] to a list of matching tab IDs.
    ///
    /// If `tab_window_id` is set, only that window is searched; otherwise all windows
    /// are enumerated first. Enforces [`MultipleMatchBehavior`].
    ///
    /// # Errors
    ///
    /// Returns an error if any command fails, a regex is invalid, no tab matches, or
    /// multiple tabs match and the policy is abort.
    pub async fn resolve_tabs(&self, matcher: &TabMatcher) -> Result<Vec<TabId>, Error> {
        let window_ids_to_search: Vec<WindowId> = if let Some(win_id) = matcher.tab_window_id {
            vec![win_id]
        } else {
            let windows = self.list_windows().await?;
            windows.iter().map(|w| w.id).collect()
        };

        let mut all_tabs: Vec<TabDetails> = Vec::new();
        for win_id in window_ids_to_search {
            let tabs = self.list_tabs(win_id).await?;
            all_tabs.extend(tabs);
        }

        let matched = match_tabs(&all_tabs, matcher)?;
        let criteria = matcher.to_string();
        match matched.len() {
            0 => Err(Error::NoMatchingTab { criteria }),
            1 => Ok(matched),
            count => match matcher.if_matches_multiple {
                MultipleMatchBehavior::Abort => Err(Error::AmbiguousTab { count, criteria }),
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
) -> Result<CliResult, Error> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let request = CliRequest::new(request_id.clone(), command);

    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(socket_path).await?;
    #[cfg(windows)]
    let stream = {
        let pipe_name = crate::discovery::pipe_name_from_marker(socket_path)?;
        tokio::net::windows::named_pipe::ClientOptions::new().open(&pipe_name)?
    };

    let (read_half, mut write_half) = tokio::io::split(stream);

    let mut json = serde_json::to_vec(&request)?;
    json.push(b'\n');
    write_half.write_all(&json).await?;

    let mut reader = tokio::io::BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: CliResponse = serde_json::from_str(line.trim_end())?;

    if response.request_id != request_id {
        tracing::warn!(
            expected = %request_id,
            received = %response.request_id,
            "Response request_id mismatch",
        );
    }

    match response.outcome {
        CliOutcome::Ok(result) => Ok(result),
        CliOutcome::Err(msg) => Err(Error::CommandFailed(msg)),
        _ => Err(Error::CommandFailed("unknown outcome variant".to_owned())),
    }
}
