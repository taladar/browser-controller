//! CLI command helpers for integration tests.
//!
//! Sends commands to the mediator over Unix Domain Socket using the same
//! newline-delimited JSON protocol that the CLI binary uses.

use std::path::Path;

use browser_controller_types::{CliCommand, CliOutcome, CliRequest, CliResponse, CliResult};
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
