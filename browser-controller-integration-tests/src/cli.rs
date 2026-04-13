//! CLI command helpers for integration tests.
//!
//! Sends commands to the mediator over Unix Domain Socket using the same
//! newline-delimited JSON protocol that the CLI binary uses.

use std::path::Path;

use browser_controller_types::{
    BrowserEvent, CliCommand, CliOutcome, CliRequest, CliResponse, CliResult,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

/// Error type for CLI command operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// IO error communicating with the mediator socket.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// The command returned an error from the extension.
    #[error("command failed: {0}")]
    CommandFailed(String),
    /// The response request_id did not match the sent request_id.
    #[error("request_id mismatch: expected {expected}, got {received}")]
    RequestIdMismatch {
        /// The request ID that was sent.
        expected: String,
        /// The request ID that was received.
        received: String,
    },
}

/// Send a command to the mediator and wait for the response.
///
/// This replicates the wire protocol from `browser-controller-cli/src/main.rs:1148-1190`:
/// newline-delimited JSON over a Unix Domain Socket.
///
/// # Errors
///
/// Returns an error on IO failure, JSON failure, request ID mismatch, or if the
/// command itself returns an error from the extension.
pub async fn send_command(socket_path: &Path, command: CliCommand) -> Result<CliResult, Error> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let request = CliRequest {
        request_id: request_id.clone(),
        command,
    };

    let stream = UnixStream::connect(socket_path).await?;
    let (read_half, mut write_half) = tokio::io::split(stream);

    let mut json = serde_json::to_vec(&request)?;
    json.push(b'\n');
    write_half.write_all(&json).await?;

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: CliResponse = serde_json::from_str(line.trim_end())?;

    if response.request_id != request_id {
        return Err(Error::RequestIdMismatch {
            expected: request_id,
            received: response.request_id,
        });
    }

    match response.outcome {
        CliOutcome::Ok(result) => Ok(result),
        CliOutcome::Err(msg) => Err(Error::CommandFailed(msg)),
    }
}

/// An active event subscription connection.
///
/// After sending `SubscribeEvents`, the mediator streams [`BrowserEvent`] objects
/// as newline-delimited JSON. Use [`EventSubscription::next_event`] to read them.
#[expect(
    missing_debug_implementations,
    reason = "tokio ReadHalf does not implement Debug in a useful way"
)]
pub struct EventSubscription {
    /// Buffered reader for the event stream.
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
}

impl EventSubscription {
    /// Open a new event subscription to the mediator.
    ///
    /// Sends `SubscribeEvents` and returns a handle for reading events.
    ///
    /// # Errors
    ///
    /// Returns an error if the socket connection or command send fails.
    pub async fn open(socket_path: &Path) -> Result<Self, Error> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let request = CliRequest {
            request_id,
            command: CliCommand::SubscribeEvents,
        };

        let stream = UnixStream::connect(socket_path).await?;
        let (read_half, mut write_half) = tokio::io::split(stream);

        let mut json = serde_json::to_vec(&request)?;
        json.push(b'\n');
        write_half.write_all(&json).await?;

        Ok(Self {
            reader: BufReader::new(read_half),
        })
    }

    /// Read the next event from the subscription.
    ///
    /// # Errors
    ///
    /// Returns an error if reading or parsing the event fails.
    pub async fn next_event(&mut self) -> Result<BrowserEvent, Error> {
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        let event: BrowserEvent = serde_json::from_str(line.trim_end())?;
        Ok(event)
    }
}
