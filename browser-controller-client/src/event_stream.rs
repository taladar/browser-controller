//! Event stream subscription for receiving browser events.

use std::path::Path;

use browser_controller_types::{BrowserEvent, CliCommand, CliRequest};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

/// Errors that can occur when working with the event stream.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum EventStreamError {
    /// Failed to connect to the mediator socket.
    #[error("failed to connect to mediator socket: {0}")]
    Connect(std::io::Error),
    /// Failed to serialize the subscribe command.
    #[error("failed to serialize subscribe command: {0}")]
    Serialize(serde_json::Error),
    /// Failed to send the subscribe command.
    #[error("failed to send subscribe command: {0}")]
    Send(std::io::Error),
    /// Failed to read an event from the stream.
    #[error("failed to read event from stream: {0}")]
    Read(std::io::Error),
    /// Failed to parse an event JSON message.
    #[error("failed to parse event JSON {raw:?}: {source}")]
    ParseEvent {
        /// The JSON parse error.
        source: serde_json::Error,
        /// The raw line that failed to parse.
        raw: String,
    },
}

/// An active event subscription connection.
///
/// After sending `SubscribeEvents`, the mediator streams [`BrowserEvent`] objects
/// as newline-delimited JSON. Use [`EventStream::next_event`] to read them.
#[expect(
    missing_debug_implementations,
    reason = "tokio ReadHalf does not implement Debug in a useful way"
)]
pub struct EventStream {
    /// Buffered reader for the event stream.
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
    /// Keep write half alive so the mediator doesn't observe EOF.
    _write_half: tokio::io::WriteHalf<UnixStream>,
}

impl EventStream {
    /// Open a new event subscription to the mediator.
    ///
    /// When both `include_windows_tabs` and `include_downloads` are `false`,
    /// all event categories are delivered (backward compatible).
    ///
    /// # Errors
    ///
    /// Returns an error if the socket connection or command send fails.
    pub async fn open(
        socket_path: &Path,
        include_windows_tabs: bool,
        include_downloads: bool,
    ) -> Result<Self, EventStreamError> {
        let request = CliRequest::new(
            uuid::Uuid::new_v4().to_string(),
            CliCommand::SubscribeEvents {
                include_windows_tabs,
                include_downloads,
            },
        );

        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(EventStreamError::Connect)?;
        let (read_half, mut write_half) = tokio::io::split(stream);

        let mut json = serde_json::to_vec(&request).map_err(EventStreamError::Serialize)?;
        json.push(b'\n');
        write_half
            .write_all(&json)
            .await
            .map_err(EventStreamError::Send)?;

        Ok(Self {
            reader: BufReader::new(read_half),
            _write_half: write_half,
        })
    }

    /// Read the next event from the subscription.
    ///
    /// Returns `None` when the mediator closes the connection.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or parsing the event fails.
    pub async fn next_event(&mut self) -> Result<Option<BrowserEvent>, EventStreamError> {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(EventStreamError::Read)?;
        if n == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end().to_owned();
        let event: BrowserEvent =
            serde_json::from_str(&trimmed).map_err(|source| EventStreamError::ParseEvent {
                source,
                raw: trimmed,
            })?;
        Ok(Some(event))
    }
}
