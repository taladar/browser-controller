//! Mediator socket discovery and readiness polling.
//!
//! The mediator creates its socket at `$XDG_RUNTIME_DIR/browser-controller/<ppid>.sock`
//! where `<ppid>` is the browser's PID. Since the browser launches the mediator via
//! native messaging, we discover the socket by watching for new `.sock` files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::time::sleep;

/// Error type for mediator discovery operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The socket directory could not be determined.
    #[error("$XDG_RUNTIME_DIR is not set")]
    NoRuntimeDir,
    /// No new socket appeared within the timeout.
    #[error("no new mediator socket appeared within {0:?}")]
    Timeout(Duration),
    /// IO error while scanning the socket directory.
    #[error("IO error scanning socket directory: {0}")]
    Io(#[from] std::io::Error),
}

/// Return the directory where mediator sockets are created.
///
/// On Linux this is `$XDG_RUNTIME_DIR/browser-controller/`.
///
/// # Errors
///
/// Returns [`Error::NoRuntimeDir`] if `$XDG_RUNTIME_DIR` is not set.
pub fn socket_dir() -> Result<PathBuf, Error> {
    let runtime = std::env::var("XDG_RUNTIME_DIR").map_err(|_not_set| Error::NoRuntimeDir)?;
    Ok(Path::new(&runtime).join("browser-controller"))
}

/// List all `.sock` files currently in the socket directory.
///
/// Returns an empty set if the directory does not exist.
#[must_use]
pub fn list_sockets(dir: &Path) -> HashSet<PathBuf> {
    let mut sockets = HashSet::new();
    if let Ok(entries) = fs_err::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "sock") {
                sockets.insert(path);
            }
        }
    }
    sockets
}

/// Wait for a new `.sock` file to appear in the socket directory that was not
/// in `existing_sockets`.
///
/// # Errors
///
/// Returns [`Error::Timeout`] if no new socket appears within the given timeout.
pub async fn wait_for_new_socket(
    dir: &Path,
    existing_sockets: &HashSet<PathBuf>,
    timeout: Duration,
) -> Result<PathBuf, Error> {
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(tokio::time::Instant::now);
    let poll_interval = Duration::from_millis(250);

    loop {
        let current = list_sockets(dir);
        let new_sockets: Vec<_> = current.difference(existing_sockets).collect();

        if let Some(socket) = new_sockets.first() {
            return Ok((*socket).clone());
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Timeout(timeout));
        }
        sleep(poll_interval).await;
    }
}
