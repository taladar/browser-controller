//! Mediator instance discovery and selection.

use std::path::{Path, PathBuf};
use std::time::Duration;

use browser_controller_types::{BrowserInfo, CliCommand, CliResult};

use crate::client::{SendCommandError, send_command};

/// Timeout for connecting to and querying a single mediator instance during discovery.
const INSTANCE_QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// Errors that can occur during mediator instance discovery and selection.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The runtime/temp directory cannot be determined.
    #[error("cannot determine runtime/temp directory for IPC sockets")]
    NoRuntimeDir,
    /// Failed to read the socket directory.
    #[error("failed to read socket directory: {0}")]
    ListSockets(std::io::Error),
    /// Failed to query a specific mediator instance.
    #[error("failed to query mediator instance: {0}")]
    QueryInstance(#[from] SendCommandError),
    /// The instance query timed out.
    #[error("instance query timed out")]
    QueryTimeout,
    /// The mediator returned an unexpected response to GetBrowserInfo.
    #[error("unexpected response to GetBrowserInfo: {response:?}")]
    UnexpectedResponse {
        /// The actual response received.
        response: Box<CliResult>,
    },
    /// No running mediator instance was found.
    #[error("no browser-controller mediator is running (no sockets in {dir})")]
    NoInstances {
        /// The directory that was searched.
        dir: PathBuf,
    },
    /// A background task panicked or was cancelled.
    #[error("background task error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
    /// Invalid pipe marker file path (Windows).
    #[cfg(windows)]
    #[error("invalid pipe marker path: {0}")]
    InvalidPipeMarker(std::io::Error),
}

/// A discovered mediator instance.
#[derive(Debug)]
pub struct DiscoveredInstance {
    /// Path to the mediator's UDS socket.
    pub socket_path: PathBuf,
    /// Browser information returned by the mediator.
    pub info: BrowserInfo,
}

impl DiscoveredInstance {
    /// Create a [`Client`](crate::Client) connected to this instance.
    #[must_use]
    pub fn client(&self, timeout: Duration) -> crate::Client {
        crate::Client::new(self.socket_path.clone(), timeout)
    }
}

/// Return the directory where mediator IPC socket/marker files are stored.
///
/// - Linux: `$XDG_RUNTIME_DIR/browser-controller/`
/// - macOS: `$TMPDIR/browser-controller/` (user-private; falls back to `~/Library/Caches`)
/// - Windows: `%LOCALAPPDATA%\Temp\browser-controller\`
///
/// # Errors
///
/// Returns [`DiscoveryError::NoRuntimeDir`] when the platform base directory cannot be determined.
pub fn socket_dir() -> Result<PathBuf, DiscoveryError> {
    #[cfg(target_os = "linux")]
    {
        let runtime_dir =
            std::env::var("XDG_RUNTIME_DIR").map_err(|_not_set| DiscoveryError::NoRuntimeDir)?;
        Ok(Path::new(&runtime_dir).join("browser-controller"))
    }
    #[cfg(target_os = "macos")]
    {
        let dir = std::env::var("TMPDIR")
            .map(|t| Path::new(&t).join("browser-controller"))
            .or_else(|_| {
                directories::BaseDirs::new()
                    .map(|b| b.cache_dir().join("browser-controller"))
                    .ok_or(DiscoveryError::NoRuntimeDir)
            })?;
        Ok(dir)
    }
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var("LOCALAPPDATA").map_err(|_| DiscoveryError::NoRuntimeDir)?;
        Ok(Path::new(&local).join("Temp").join("browser-controller"))
    }
}

/// File extension used for mediator IPC discovery files.
///
/// On Unix: `.sock` (the actual socket file).
/// On Windows: `.pipe` (empty marker file; named pipe is discovered from the stem).
#[cfg(unix)]
const SOCKET_EXT: &str = "sock";
/// File extension used for mediator IPC discovery files.
#[cfg(windows)]
const SOCKET_EXT: &str = "pipe";

/// List all mediator IPC discovery files in `dir`.
///
/// # Errors
///
/// Returns an error if the directory cannot be read.
fn list_socket_files(dir: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
    tracing::debug!(dir = %dir.display(), "Scanning socket directory");
    let rd = match fs_err::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(dir = %dir.display(), "Socket directory does not exist");
            return Ok(Vec::new());
        }
        Err(e) => return Err(DiscoveryError::ListSockets(e)),
    };
    let mut paths = Vec::new();
    for entry in rd {
        let path = entry.map_err(DiscoveryError::ListSockets)?.path();
        if path.extension() == Some(std::ffi::OsStr::new(SOCKET_EXT)) {
            tracing::debug!(socket = %path.display(), "Found socket file");
            paths.push(path);
        } else {
            tracing::debug!(path = %path.display(), "Ignoring non-socket file");
        }
    }
    if paths.is_empty() {
        tracing::debug!(dir = %dir.display(), "No socket files found in directory");
    }
    Ok(paths)
}

/// Connect to a mediator socket, send `GetBrowserInfo`, and return the result.
///
/// Times out after [`INSTANCE_QUERY_TIMEOUT`].
///
/// # Errors
///
/// Returns an error if the connection or query fails or times out.
async fn query_instance(socket_path: &Path) -> Result<BrowserInfo, DiscoveryError> {
    let result = tokio::time::timeout(
        INSTANCE_QUERY_TIMEOUT,
        send_command(socket_path, CliCommand::GetBrowserInfo),
    )
    .await
    .map_err(|_elapsed| DiscoveryError::QueryTimeout)?;

    match result? {
        CliResult::BrowserInfo(info) => Ok(info),
        other => Err(DiscoveryError::UnexpectedResponse {
            response: Box::new(other),
        }),
    }
}

/// Discover all running mediator instances by scanning the socket directory.
///
/// Sockets that cannot be connected to (e.g. stale) are silently skipped.
///
/// # Errors
///
/// Returns an error if the runtime directory is not set or the directory cannot be read.
pub async fn discover_instances() -> Result<Vec<DiscoveredInstance>, DiscoveryError> {
    let dir = socket_dir()?;
    tracing::debug!(dir = %dir.display(), "Discovering mediator instances");
    let sock_paths = tokio::task::spawn_blocking({
        let dir = dir.clone();
        move || list_socket_files(&dir)
    })
    .await??;
    tracing::debug!(count = sock_paths.len(), "Socket files found");

    let mut instances = Vec::new();
    for socket_path in sock_paths {
        match query_instance(&socket_path).await {
            Ok(info) => {
                tracing::debug!(
                    socket = %socket_path.display(),
                    browser = %info.browser_name,
                    pid = info.pid,
                    "Discovered instance",
                );
                instances.push(DiscoveredInstance { socket_path, info });
            }
            Err(e) => {
                tracing::debug!(
                    socket = %socket_path.display(),
                    error = %e,
                    "Skipping unreachable socket",
                );
            }
        }
    }
    Ok(instances)
}

/// Derive the Windows named pipe name from a `.pipe` marker file path.
///
/// The stem of the file (the PID) is used to construct `\\.\pipe\browser-controller-<pid>`.
#[cfg(windows)]
pub(crate) fn pipe_name_from_marker(path: &Path) -> Result<String, std::io::Error> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| std::io::Error::other("invalid pipe marker path"))?;
    Ok(format!(r"\\.\pipe\browser-controller-{stem}"))
}
