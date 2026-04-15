//! Error types for the browser-controller client library.

use browser_controller_types::CliResult;

use crate::client::SendCommandError;
use crate::discovery::DiscoveryError;
use crate::event_stream::EventStreamError;
use crate::manifest::ManifestError;
use crate::matchers::MatchError;
use crate::rdp::RdpError;

/// Error from a [`Client`](crate::Client) method that sends a command.
///
/// The type parameter `E` captures method-specific errors beyond the common
/// send/timeout/unexpected-response failures. For most simple commands `E` is
/// [`std::convert::Infallible`]; for resolve operations it is [`MatchError`].
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum CommandError<E: std::error::Error + 'static> {
    /// The command could not be sent or the response could not be read.
    #[error(transparent)]
    Send(#[from] SendCommandError),
    /// The command timed out.
    #[error("command timed out")]
    Timeout,
    /// The mediator returned an unexpected response variant.
    #[error("unexpected response: expected {expected}, got {actual:?}")]
    UnexpectedResponse {
        /// The name of the expected `CliResult` variant.
        expected: &'static str,
        /// The actual `CliResult` that was received.
        actual: Box<CliResult>,
    },
    /// A method-specific error (e.g. [`MatchError`] for resolve operations).
    #[error(transparent)]
    Other(E),
}

/// Top-level error type for the browser-controller client library.
///
/// Each module defines its own focused error enum. This type aggregates them
/// for convenience when callers do not need fine-grained matching.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error occurred while sending a command to the mediator.
    #[error(transparent)]
    SendCommand(#[from] SendCommandError),
    /// An error occurred on the event stream.
    #[error(transparent)]
    EventStream(#[from] EventStreamError),
    /// An error occurred during Firefox RDP communication.
    #[error(transparent)]
    Rdp(#[from] RdpError),
    /// An error occurred during instance discovery or selection.
    #[error(transparent)]
    Discovery(#[from] DiscoveryError),
    /// An error occurred during manifest installation.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// An error occurred while matching windows, tabs, or instances.
    #[error(transparent)]
    Match(#[from] MatchError),
    /// The mediator returned a response variant that does not match the command.
    #[error("unexpected response: expected {expected}, got {actual:?}")]
    UnexpectedResponse {
        /// The name of the expected `CliResult` variant.
        expected: &'static str,
        /// The actual `CliResult` that was received.
        actual: Box<CliResult>,
    },
    /// A command timed out waiting for a response.
    #[error("command timed out")]
    Timeout,
    /// A background task panicked or was cancelled.
    #[error("background task error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

impl CommandError<std::convert::Infallible> {
    /// Widen this infallible command error to any error type parameter.
    ///
    /// Since `Infallible` can never be constructed, the `Other` variant is
    /// unreachable and this conversion is always safe.
    #[must_use]
    pub fn widen<E: std::error::Error + 'static>(self) -> CommandError<E> {
        match self {
            Self::Send(e) => CommandError::Send(e),
            Self::Timeout => CommandError::Timeout,
            Self::UnexpectedResponse { expected, actual } => {
                CommandError::UnexpectedResponse { expected, actual }
            }
            Self::Other(infallible) => match infallible {},
        }
    }
}

/// `Infallible` trivially converts to any error (it can never be constructed).
impl From<std::convert::Infallible> for Error {
    fn from(e: std::convert::Infallible) -> Self {
        match e {}
    }
}

/// Convert any `CommandError<E>` into the top-level `Error`.
impl<E: std::error::Error + 'static> From<CommandError<E>> for Error
where
    Self: From<E>,
{
    fn from(e: CommandError<E>) -> Self {
        match e {
            CommandError::Send(e) => Self::SendCommand(e),
            CommandError::Timeout => Self::Timeout,
            CommandError::UnexpectedResponse { expected, actual } => {
                Self::UnexpectedResponse { expected, actual }
            }
            CommandError::Other(e) => Self::from(e),
        }
    }
}
