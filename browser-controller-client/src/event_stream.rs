//! Event stream subscription for receiving browser events.

use std::path::Path;

use browser_controller_types::{BrowserEvent, CliCommand, CliRequest};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

use crate::Error;

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
    /// Sends `SubscribeEvents` and returns a handle for reading events.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket connection or command send fails.
    pub async fn open(socket_path: &Path) -> Result<Self, Error> {
        let request = CliRequest::new(
            uuid::Uuid::new_v4().to_string(),
            CliCommand::SubscribeEvents,
        );

        let stream = UnixStream::connect(socket_path).await?;
        let (read_half, mut write_half) = tokio::io::split(stream);

        let mut json = serde_json::to_vec(&request)?;
        json.push(b'\n');
        write_half.write_all(&json).await?;

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
    pub async fn next_event(&mut self) -> Result<Option<BrowserEvent>, Error> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        let event: BrowserEvent = serde_json::from_str(line.trim_end())?;
        Ok(Some(event))
    }
}
