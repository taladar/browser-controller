//! The main `Client` struct for communicating with a browser-controller mediator.

use std::path::{Path, PathBuf};
use std::time::Duration;

use browser_controller_types::{
    BrowserInfo, CliCommand, CliOutcome, CliRequest, CliResponse, CliResult, TabDetails,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};

use crate::Error;
use crate::event_stream::EventStream;
use crate::matchers::{
    MultipleMatchBehavior, TabMatcher, WindowMatcher, match_tabs, match_windows,
};

/// An async client for communicating with a browser-controller mediator.
///
/// Create a `Client` with the path to a mediator's Unix Domain Socket,
/// then use its methods to send commands and subscribe to events.
#[derive(Debug, Clone)]
pub struct Client {
    /// Path to the mediator's UDS socket (Unix) or pipe marker file (Windows).
    socket_path: PathBuf,
}

impl Client {
    /// Create a new client connected to the mediator at the given socket path.
    #[must_use]
    pub const fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Return the socket path this client is configured to use.
    #[must_use]
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Send a command to the mediator and wait for the response.
    ///
    /// # Errors
    ///
    /// Returns an error on connection failure, serialization failure,
    /// request ID mismatch, or if the command itself fails.
    pub async fn send_command(&self, command: CliCommand) -> Result<CliResult, Error> {
        send_command(&self.socket_path, command).await
    }

    /// Send a command with a timeout.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Timeout`] if the command does not complete within the given duration.
    pub async fn send_command_timeout(
        &self,
        command: CliCommand,
        timeout: Duration,
    ) -> Result<CliResult, Error> {
        tokio::time::timeout(timeout, self.send_command(command))
            .await
            .map_err(|_elapsed| Error::Timeout)?
    }

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

    /// Retrieve information about the connected browser instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails or returns an unexpected response.
    pub async fn browser_info(&self) -> Result<BrowserInfo, Error> {
        match self.send_command(CliCommand::GetBrowserInfo).await? {
            CliResult::BrowserInfo(info) => Ok(info),
            other => Err(Error::CommandFailed(format!(
                "unexpected response to GetBrowserInfo: {other:?}"
            ))),
        }
    }

    /// Resolve a [`WindowMatcher`] to a list of matching window IDs.
    ///
    /// Sends `ListWindows` to the mediator, applies the matcher, and enforces
    /// [`MultipleMatchBehavior`].
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails, the regex is invalid, no window matches, or
    /// multiple windows match and the policy is abort.
    pub async fn resolve_windows(&self, matcher: &WindowMatcher) -> Result<Vec<u32>, Error> {
        let result = send_command(self.socket_path(), CliCommand::ListWindows).await?;
        let CliResult::Windows { windows } = result else {
            return Err(Error::CommandFailed(format!(
                "unexpected response to ListWindows: {result:?}"
            )));
        };
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
    pub async fn resolve_tabs(&self, matcher: &TabMatcher) -> Result<Vec<u32>, Error> {
        let window_ids_to_search: Vec<u32> = if let Some(win_id) = matcher.tab_window_id {
            vec![win_id]
        } else {
            let list_result = send_command(self.socket_path(), CliCommand::ListWindows).await?;
            let CliResult::Windows { windows } = list_result else {
                return Err(Error::CommandFailed(format!(
                    "unexpected response to ListWindows: {list_result:?}"
                )));
            };
            windows.iter().map(|w| w.id).collect()
        };

        let mut all_tabs: Vec<TabDetails> = Vec::new();
        for win_id in window_ids_to_search {
            let tabs_result = send_command(
                self.socket_path(),
                CliCommand::ListTabs { window_id: win_id },
            )
            .await?;
            let CliResult::Tabs { tabs } = tabs_result else {
                return Err(Error::CommandFailed(format!(
                    "unexpected response to ListTabs: {tabs_result:?}"
                )));
            };
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
