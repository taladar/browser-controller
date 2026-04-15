//! Mediator instance discovery and selection.

use std::path::{Path, PathBuf};
use std::time::Duration;

use browser_controller_types::{BrowserInfo, CliCommand, CliResult};

use crate::Error;
use crate::client::send_command;

/// Timeout for connecting to and querying a single mediator instance during discovery.
const INSTANCE_QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// A discovered mediator instance.
#[derive(Debug)]
pub struct DiscoveredInstance {
    /// Path to the mediator's UDS socket.
    pub socket_path: PathBuf,
    /// Browser information returned by the mediator.
    pub info: BrowserInfo,
}

/// Return the directory where mediator IPC socket/marker files are stored.
///
/// - Linux: `$XDG_RUNTIME_DIR/browser-controller/`
/// - macOS: `$TMPDIR/browser-controller/` (user-private; falls back to `~/Library/Caches`)
/// - Windows: `%LOCALAPPDATA%\Temp\browser-controller\`
///
/// # Errors
///
/// Returns [`Error::NoRuntimeDir`] when the platform base directory cannot be determined.
pub fn socket_dir() -> Result<PathBuf, Error> {
    #[cfg(target_os = "linux")]
    {
        let runtime_dir =
            std::env::var("XDG_RUNTIME_DIR").map_err(|_not_set| Error::NoRuntimeDir)?;
        Ok(Path::new(&runtime_dir).join("browser-controller"))
    }
    #[cfg(target_os = "macos")]
    {
        let dir = std::env::var("TMPDIR")
            .map(|t| Path::new(&t).join("browser-controller"))
            .or_else(|_| {
                directories::BaseDirs::new()
                    .map(|b| b.cache_dir().join("browser-controller"))
                    .ok_or(Error::NoRuntimeDir)
            })?;
        Ok(dir)
    }
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var("LOCALAPPDATA").map_err(|_| Error::NoRuntimeDir)?;
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
fn list_socket_files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    tracing::debug!(dir = %dir.display(), "Scanning socket directory");
    let rd = match fs_err::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!(dir = %dir.display(), "Socket directory does not exist");
            return Ok(Vec::new());
        }
        Err(e) => return Err(Error::Io(e)),
    };
    let mut paths = Vec::new();
    for entry in rd {
        let path = entry.map_err(Error::Io)?.path();
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
async fn query_instance(socket_path: &Path) -> Result<BrowserInfo, Error> {
    let result = tokio::time::timeout(
        INSTANCE_QUERY_TIMEOUT,
        send_command(socket_path, CliCommand::GetBrowserInfo),
    )
    .await
    .map_err(|_elapsed| {
        Error::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "instance query timed out",
        ))
    })?;

    match result? {
        CliResult::BrowserInfo(info) => Ok(info),
        other => Err(Error::CommandFailed(format!(
            "unexpected response to GetBrowserInfo: {other:?}"
        ))),
    }
}

/// Discover all running mediator instances by scanning the socket directory.
///
/// Sockets that cannot be connected to (e.g. stale) are silently skipped.
///
/// # Errors
///
/// Returns an error if the runtime directory is not set or the directory cannot be read.
pub async fn discover_instances() -> Result<Vec<DiscoveredInstance>, Error> {
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

/// Select an instance from the discovered list based on a selector.
///
/// The selector can be a numeric PID or a case-insensitive browser name substring.
///
/// # Errors
///
/// Returns an error if no instances are running, the selector is ambiguous, or no match is found.
pub fn select_instance<'a>(
    instances: &'a [DiscoveredInstance],
    selector: Option<&str>,
    socket_dir: &Path,
) -> Result<&'a DiscoveredInstance, Error> {
    if instances.is_empty() {
        return Err(Error::NoInstances {
            dir: socket_dir.to_owned(),
        });
    }

    match selector {
        None => match instances {
            [only] => Ok(only),
            _ => Err(Error::MultipleInstances),
        },
        Some(sel) => {
            // Try numeric PID match first.
            if let Ok(pid) = sel.parse::<u32>() {
                return instances.iter().find(|i| i.info.pid == pid).ok_or_else(|| {
                    Error::NoMatchingInstance {
                        selector: sel.to_owned(),
                    }
                });
            }
            // Browser name substring match (case-insensitive).
            let sel_lower = sel.to_ascii_lowercase();
            let matches: Vec<&DiscoveredInstance> = instances
                .iter()
                .filter(|i| {
                    i.info
                        .browser_name
                        .to_ascii_lowercase()
                        .contains(&sel_lower)
                })
                .collect();
            match matches.as_slice() {
                [] => Err(Error::NoMatchingInstance {
                    selector: sel.to_owned(),
                }),
                [only] => Ok(*only),
                _ => Err(Error::AmbiguousInstance {
                    selector: sel.to_owned(),
                }),
            }
        }
    }
}

/// Derive the Windows named pipe name from a `.pipe` marker file path.
///
/// The stem of the file (the PID) is used to construct `\\.\pipe\browser-controller-<pid>`.
#[cfg(windows)]
pub(crate) fn pipe_name_from_marker(path: &Path) -> Result<String, Error> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Io(std::io::Error::other("invalid pipe marker path")))?;
    Ok(format!(r"\\.\pipe\browser-controller-{stem}"))
}
