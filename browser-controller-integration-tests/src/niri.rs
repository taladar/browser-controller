//! Niri compositor IPC helpers for verifying window state.
//!
//! Uses the `niri-ipc` crate to query the niri Wayland compositor for window
//! information. All verification functions gracefully skip when niri is not
//! available (i.e. `$NIRI_SOCKET` is not set, or the platform is not Linux).

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
    /// Niri is not supported on this platform.
    #[error("niri is not supported on this platform")]
    Unsupported,
}

/// Whether niri IPC is available in the current environment.
#[cfg(target_os = "linux")]
#[must_use]
pub fn is_available() -> bool {
    std::env::var("NIRI_SOCKET").is_ok()
}

/// Whether niri IPC is available in the current environment.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub const fn is_available() -> bool {
    false
}

/// Linux-only implementation backed by `niri-ipc`.
#[cfg(target_os = "linux")]
mod imp {
    use super::Error;
    use niri_ipc::socket::Socket;
    use niri_ipc::{Request, Response, Window};

    /// Query all windows from the niri compositor.
    pub(super) fn get_all_windows() -> Result<Vec<Window>, Error> {
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

    /// Filter all windows down to those owned by the given PID.
    pub(super) fn get_windows_for_pid(pid: u32) -> Result<Vec<Window>, Error> {
        let all = get_all_windows()?;
        let pid_i32 = i32::try_from(pid).unwrap_or(0);
        Ok(all.into_iter().filter(|w| w.pid == Some(pid_i32)).collect())
    }
}

/// Check whether any window belonging to the given PID has a title starting
/// with the given prefix.
///
/// # Errors
///
/// Returns a [`Error`] if the underlying niri query fails.
#[cfg(target_os = "linux")]
pub fn has_window_with_title_prefix(pid: u32, prefix: &str) -> Result<bool, Error> {
    let windows = imp::get_windows_for_pid(pid)?;
    Ok(windows
        .iter()
        .any(|w| w.title.as_deref().is_some_and(|t| t.starts_with(prefix))))
}

/// Stub: always returns [`Error::Unsupported`] on non-Linux platforms.
///
/// # Errors
///
/// Always returns [`Error::Unsupported`].
#[cfg(not(target_os = "linux"))]
pub const fn has_window_with_title_prefix(_pid: u32, _prefix: &str) -> Result<bool, Error> {
    Err(Error::Unsupported)
}

/// Count how many windows belong to the given PID.
///
/// # Errors
///
/// Returns a [`Error`] if the underlying niri query fails.
#[cfg(target_os = "linux")]
pub fn count_windows_for_pid(pid: u32) -> Result<usize, Error> {
    imp::get_windows_for_pid(pid).map(|w| w.len())
}

/// Stub: always returns [`Error::Unsupported`] on non-Linux platforms.
///
/// # Errors
///
/// Always returns [`Error::Unsupported`].
#[cfg(not(target_os = "linux"))]
pub const fn count_windows_for_pid(_pid: u32) -> Result<usize, Error> {
    Err(Error::Unsupported)
}
