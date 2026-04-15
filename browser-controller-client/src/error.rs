//! Error types for the browser-controller client library.

use std::path::PathBuf;

/// Errors that can occur in the browser-controller client library.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred (covers both network and filesystem operations).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The browser command returned an error.
    #[error("command failed: {0}")]
    CommandFailed(String),

    /// The response `request_id` did not match the sent `request_id`.
    #[error("request_id mismatch: expected {expected}, got {received}")]
    RequestIdMismatch {
        /// The request ID that was sent.
        expected: String,
        /// The request ID that was received.
        received: String,
    },

    /// No window matched the given criteria.
    #[error("no window matched the criteria: {criteria}")]
    NoMatchingWindow {
        /// Description of the criteria that were used.
        criteria: String,
    },

    /// More than one window matched the criteria and the policy is abort.
    #[error(
        "{count} windows matched the criteria: {criteria}; use MultipleMatchBehavior::All to apply to all"
    )]
    AmbiguousWindow {
        /// Number of windows that matched.
        count: usize,
        /// Description of the criteria that were used.
        criteria: String,
    },

    /// No tab matched the given criteria.
    #[error("no tab matched the criteria: {criteria}")]
    NoMatchingTab {
        /// Description of the criteria that were used.
        criteria: String,
    },

    /// More than one tab matched the criteria and the policy is abort.
    #[error(
        "{count} tabs matched the criteria: {criteria}; use MultipleMatchBehavior::All to apply to all"
    )]
    AmbiguousTab {
        /// Number of tabs that matched.
        count: usize,
        /// Description of the criteria that were used.
        criteria: String,
    },

    /// A regular expression pattern could not be compiled.
    #[error("invalid regex: {0}")]
    InvalidRegex(#[from] regex::Error),

    /// A command timed out waiting for a response.
    #[error("command timed out")]
    Timeout,

    /// The runtime/temp directory cannot be determined.
    #[error("cannot determine runtime/temp directory for IPC sockets")]
    NoRuntimeDir,

    /// No running mediator instance was found.
    #[error("no browser-controller mediator is running (no sockets in {dir})")]
    NoInstances {
        /// The directory that was searched.
        dir: PathBuf,
    },

    /// Multiple instances are running and no selector was provided.
    #[error(
        "multiple browser instances are running; provide a selector (pid or browser-name) to choose one"
    )]
    MultipleInstances,

    /// The specified instance selector matched no running instance.
    #[error("no browser instance matches '{selector}'")]
    NoMatchingInstance {
        /// The selector that was provided.
        selector: String,
    },

    /// The specified browser name matched more than one running instance.
    #[error("multiple browser instances match '{selector}'; use a PID to disambiguate")]
    AmbiguousInstance {
        /// The selector that matched multiple instances.
        selector: String,
    },

    /// A background task panicked or was cancelled.
    #[error("background task error: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    /// The user's home directory could not be determined.
    #[error("could not determine home directory for manifest installation")]
    NoBrowserHome,

    /// No mediator binary path was given and none could be found automatically.
    #[error("mediator binary not found next to this executable; use a specific path")]
    MediatorNotFound,

    /// `extension_id` is required for Chromium-family browsers but was not supplied.
    #[error(
        "Chromium-family browsers require an extension ID; \
         find the ID on chrome://extensions after loading the unpacked extension \
         (a 32-character lowercase letter string)"
    )]
    ChromiumExtensionIdRequired,
}
