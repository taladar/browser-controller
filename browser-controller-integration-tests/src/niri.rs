//! Niri compositor IPC helpers for verifying window state.
//!
//! Uses the `niri-ipc` crate to query the niri Wayland compositor for window
//! information. All verification functions gracefully skip when niri is not
//! available (i.e. `$NIRI_SOCKET` is not set).

use niri_ipc::socket::Socket;
use niri_ipc::{Request, Response, Window};

/// Whether niri IPC is available in the current environment.
#[must_use]
pub fn is_available() -> bool {
    std::env::var("NIRI_SOCKET").is_ok()
}

/// Error type for niri IPC operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to connect to the niri socket.
    #[error("failed to connect to niri: {0}")]
    Connect(String),
    /// The niri request failed.
    #[error("niri request failed: {0}")]
    Request(String),
    /// Unexpected response type.
    #[error("unexpected niri response")]
    UnexpectedResponse,
}

/// Query all windows from the niri compositor.
///
/// # Errors
///
/// Returns a [`Error`] if the connection or request fails.
pub fn get_all_windows() -> Result<Vec<Window>, Error> {
    let mut socket = Socket::connect().map_err(|e| Error::Connect(format!("{e}")))?;

    let reply = socket
        .send(Request::Windows)
        .map_err(|e| Error::Request(format!("{e}")))?;

    match reply {
        Ok(Response::Windows(windows)) => Ok(windows),
        Ok(_) => Err(Error::UnexpectedResponse),
        Err(msg) => Err(Error::Request(msg)),
    }
}

/// Get all windows belonging to a specific process ID.
///
/// Filters the niri window list to only include windows whose `pid` field
/// matches the given PID. This is used to distinguish the test browser's
/// windows from any production browser windows.
///
/// # Errors
///
/// Returns a [`Error`] if the underlying niri query fails.
pub fn get_windows_for_pid(pid: u32) -> Result<Vec<Window>, Error> {
    let all = get_all_windows()?;
    let pid_i32 = i32::try_from(pid).unwrap_or(0);
    Ok(all.into_iter().filter(|w| w.pid == Some(pid_i32)).collect())
}

/// Check whether any window belonging to the given PID has a title starting
/// with the given prefix.
///
/// # Errors
///
/// Returns a [`Error`] if the underlying niri query fails.
pub fn has_window_with_title_prefix(pid: u32, prefix: &str) -> Result<bool, Error> {
    let windows = get_windows_for_pid(pid)?;
    Ok(windows
        .iter()
        .any(|w| w.title.as_deref().is_some_and(|t| t.starts_with(prefix))))
}

/// Count how many windows belong to the given PID.
///
/// # Errors
///
/// Returns a [`Error`] if the underlying niri query fails.
pub fn count_windows_for_pid(pid: u32) -> Result<usize, Error> {
    get_windows_for_pid(pid).map(|w| w.len())
}
