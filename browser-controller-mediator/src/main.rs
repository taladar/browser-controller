//! Browser Controller Mediator — native messaging host that bridges the browser extension
//! to CLI clients via a platform-specific IPC channel (Unix Domain Socket on Unix,
//! Named Pipe on Windows).
//!
//! The browser launches this binary as a native messaging host when the extension calls
//! `browser.runtime.connectNative("browser_controller")`. The mediator then:
//!
//! 1. Reads the browser's identity from an initial `Hello` message.
//! 2. Creates an IPC endpoint in the platform runtime/temp directory.
//! 3. Accepts CLI client connections and bridges their requests to the extension.

use std::collections::HashMap;

use browser_controller_types::{
    BrowserEvent, CliOutcome, CliRequest, CliResponse, ExtensionMessage,
};
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing_subscriber::{
    EnvFilter, Layer as _, Registry, filter::LevelFilter, layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

/// Maximum size of a native messaging message the extension may send (1 MiB).
const MAX_NATIVE_MESSAGE_SIZE: u32 = 0x0010_0000;

/// Errors that can occur in the mediator.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred (covers both network and filesystem operations).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The runtime/temp directory cannot be determined.
    #[error("cannot determine runtime/temp directory for IPC socket")]
    NoRuntimeDir,

    /// The parent process ID could not be determined.
    #[error("cannot determine parent process ID: {0}")]
    NoPpid(String),

    /// The native messaging connection was closed without a preceding `Hello` message.
    #[error("native messaging connection closed before receiving Hello from extension")]
    NativeMessagingClosedBeforeHello,

    /// An incoming native messaging message exceeded the maximum allowed size.
    #[error("native messaging message size {size} exceeds maximum {MAX_NATIVE_MESSAGE_SIZE}")]
    NativeMessageTooLarge {
        /// The declared size of the message.
        size: u32,
    },

    /// A log filter expression could not be parsed.
    #[error("failed to parse log filter: {0}")]
    LogFilter(#[from] tracing_subscriber::filter::ParseError),
}

/// A request received from a CLI client and waiting to be forwarded to the extension.
struct PendingRequest {
    /// Unique correlation ID for the request.
    request_id: String,
    /// The command to forward to the extension.
    command: browser_controller_types::CliCommand,
    /// Channel to deliver the extension's response back to the CLI task.
    response_tx: oneshot::Sender<CliOutcome>,
}

/// RAII guard that removes the IPC socket/marker file on drop.
struct SocketGuard {
    /// Path to the file to clean up.
    path: std::path::PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        match fs_err::remove_file(&self.path) {
            Ok(()) => {
                tracing::info!(path = %self.path.display(), "Removed socket file on shutdown");
            }
            Err(e) => {
                tracing::warn!(
                    path = %self.path.display(),
                    error = %e,
                    "Failed to remove socket file on shutdown",
                );
            }
        }
    }
}

/// A type-erased bidirectional async stream suitable for use in the accept loop.
trait BiDiStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static {}

#[cfg(unix)]
impl BiDiStream for tokio::net::UnixStream {}

#[cfg(windows)]
impl BiDiStream for tokio::net::windows::named_pipe::NamedPipeServer {}

impl BiDiStream for Box<dyn BiDiStream> {}

/// Read one length-prefixed JSON message from a native messaging stdin stream.
///
/// Returns `Ok(None)` on a clean EOF (extension disconnected).
///
/// # Errors
///
/// Returns an error if reading from `reader` fails, the message exceeds
/// [`MAX_NATIVE_MESSAGE_SIZE`], or the JSON cannot be deserialized.
async fn read_native_message(
    reader: &mut tokio::io::Stdin,
) -> Result<Option<ExtensionMessage>, Error> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(Error::Io(e)),
    }
    let len = u32::from_ne_bytes(len_buf);
    if len > MAX_NATIVE_MESSAGE_SIZE {
        return Err(Error::NativeMessageTooLarge { size: len });
    }
    let len_usize = usize::try_from(len).unwrap_or(usize::MAX);
    let mut json_buf = vec![0u8; len_usize];
    reader.read_exact(&mut json_buf).await?;
    let json_str = std::str::from_utf8(&json_buf).unwrap_or("<invalid utf-8>");
    tracing::debug!(json = %json_str, "Received native message from extension");
    let msg = serde_json::from_slice(&json_buf).map_err(|e| {
        tracing::error!(
            error = %e,
            json = %json_str,
            "Failed to deserialize native message from extension",
        );
        Error::Json(e)
    })?;
    Ok(Some(msg))
}

/// Write one length-prefixed JSON message to a native messaging stdout stream.
///
/// # Errors
///
/// Returns an error if JSON serialization or writing to `writer` fails.
async fn write_native_message(
    writer: &mut tokio::io::BufWriter<tokio::io::Stdout>,
    msg: &CliRequest,
) -> Result<(), Error> {
    let json = serde_json::to_vec(msg)?;
    let len = u32::try_from(json.len())
        .map_err(|_overflow| Error::NativeMessageTooLarge { size: u32::MAX })?;
    writer.write_all(&len.to_ne_bytes()).await?;
    writer.write_all(&json).await?;
    writer.flush().await?;
    Ok(())
}

/// Read one newline-delimited JSON request from a CLI client connection.
///
/// Returns `Ok(None)` when the client closes the connection.
///
/// # Errors
///
/// Returns an error if reading fails or the JSON cannot be deserialized.
async fn read_cli_request<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<CliRequest>, Error> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(None);
    }
    let request = serde_json::from_str(line.trim_end())?;
    Ok(Some(request))
}

/// Write one newline-delimited JSON response to a CLI client connection.
///
/// # Errors
///
/// Returns an error if JSON serialization or writing fails.
async fn write_cli_response<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    response: &CliResponse,
) -> Result<(), Error> {
    let mut json = serde_json::to_vec(response)?;
    json.push(b'\n');
    writer.write_all(&json).await?;
    Ok(())
}

/// Handle a single CLI client connection.
///
/// Reads requests from the client, forwards them to the main loop via `request_tx`, waits for
/// responses, and writes them back to the client.
///
/// # Errors
///
/// Returns an error if communication with the client fails.
async fn handle_cli_connection<S>(
    conn: S,
    request_tx: mpsc::Sender<PendingRequest>,
    event_tx: broadcast::Sender<BrowserEvent>,
) -> Result<(), Error>
where
    S: BiDiStream,
{
    let (read_half, mut write_half) = tokio::io::split(conn);
    let mut reader = tokio::io::BufReader::new(read_half);
    loop {
        let Some(cli_request) = read_cli_request(&mut reader).await? else {
            break;
        };

        // SubscribeEvents is handled locally — stream events until the client disconnects.
        if let browser_controller_types::CliCommand::SubscribeEvents {
            include_windows_tabs,
            include_downloads,
        } = &cli_request.command
        {
            let include_windows_tabs = *include_windows_tabs;
            let include_downloads = *include_downloads;
            let mut event_rx = event_tx.subscribe();
            // Spawn a task that signals when the client closes the connection.
            let (disconnect_tx, disconnect_rx) = oneshot::channel::<()>();
            tokio::spawn(async move {
                let mut buf = [0u8; 1];
                loop {
                    match reader.read(&mut buf).await {
                        Ok(0) | Err(_) => {
                            if disconnect_tx.send(()).is_err() {
                                tracing::debug!("Disconnect receiver already dropped");
                            }
                            break;
                        }
                        Ok(_) => {}
                    }
                }
            });
            let mut disconnect_rx = disconnect_rx;
            let mut client_disconnected = false;
            loop {
                tokio::select! {
                    _ = &mut disconnect_rx, if !client_disconnected => {
                        client_disconnected = true;
                    }
                    event = event_rx.recv() => {
                        match event {
                            Ok(ev) => {
                                if !ev.matches_filter(include_windows_tabs, include_downloads) {
                                    continue;
                                }
                                let mut json = serde_json::to_vec(&ev)?;
                                json.push(b'\n');
                                write_half.write_all(&json).await?;
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(
                                    count = n,
                                    "Event subscriber lagged; {n} events dropped",
                                );
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                break;
                            }
                        }
                    }
                }
                if client_disconnected {
                    break;
                }
            }
            return Ok(());
        }

        let request_id = cli_request.request_id.clone();
        let (response_tx, response_rx) = oneshot::channel::<CliOutcome>();
        let pending = PendingRequest {
            request_id: request_id.clone(),
            command: cli_request.command,
            response_tx,
        };
        if request_tx.send(pending).await.is_err() {
            tracing::warn!(
                request_id = %request_id,
                "Main loop shut down before CLI request could be sent",
            );
            break;
        }
        let outcome = match response_rx.await {
            Ok(o) => o,
            Err(_) => {
                tracing::warn!(
                    request_id = %request_id,
                    "Response channel closed before reply arrived",
                );
                CliOutcome::Err("mediator shut down before response arrived".to_owned())
            }
        };
        let response = CliResponse::new(request_id, outcome);
        write_cli_response(&mut write_half, &response).await?;
    }
    Ok(())
}

/// Return the final path component of a UTF-8 path string as an owned `String`.
///
/// Returns `None` if the path has no file name component or it is not valid UTF-8.
#[must_use]
fn path_basename(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_owned())
}

/// Locate and read the Firefox `profiles.ini` file.
///
/// Tries platform-specific candidate paths in order, returning the content of the
/// first readable file. Returns `None` if no readable file is found.
#[must_use]
fn read_firefox_profiles_ini() -> Option<String> {
    #[cfg(target_os = "linux")]
    let candidates: Vec<std::path::PathBuf> = {
        let home = directories::BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .or_else(|| std::env::var("HOME").ok().map(std::path::PathBuf::from))?;
        vec![
            home.join(".mozilla/firefox/profiles.ini"),
            home.join("snap/firefox/common/.mozilla/firefox/profiles.ini"),
            home.join(".var/app/org.mozilla.firefox/.mozilla/firefox/profiles.ini"),
        ]
    };

    #[cfg(target_os = "macos")]
    let candidates: Vec<std::path::PathBuf> = {
        let home = directories::BaseDirs::new()
            .map(|b| b.home_dir().to_path_buf())
            .or_else(|| std::env::var("HOME").ok().map(std::path::PathBuf::from))?;
        vec![home.join("Library/Application Support/Firefox/profiles.ini")]
    };

    #[cfg(target_os = "windows")]
    let candidates: Vec<std::path::PathBuf> = {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        vec![std::path::Path::new(&appdata).join("Mozilla/Firefox/profiles.ini")]
    };

    for candidate in &candidates {
        match fs_err::read_to_string(candidate) {
            Ok(content) => {
                tracing::info!(path = %candidate.display(), "Found Firefox profiles.ini");
                return Some(content);
            }
            Err(e) => {
                tracing::debug!(
                    path = %candidate.display(),
                    error = %e,
                    "profiles.ini not readable at path"
                );
            }
        }
    }

    tracing::info!("Firefox profiles.ini not found at any standard location");
    None
}

/// Look up the directory basename for a named Firefox profile in `profiles.ini` content.
///
/// Scans `[Profile*]` sections for one with `Name=<name>` and returns the basename of
/// its `Path` value.  Returns `None` if the named profile is not found.
#[must_use]
fn firefox_profile_id_from_name(ini_content: &str, name: &str) -> Option<String> {
    let mut in_profile_section = false;
    let mut current_name: Option<&str> = None;
    let mut current_path: Option<&str> = None;

    for line in ini_content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if in_profile_section && current_name == Some(name) {
                return current_path.and_then(path_basename);
            }
            in_profile_section = line.starts_with("[Profile");
            current_name = None;
            current_path = None;
        } else if in_profile_section {
            if let Some(v) = line.strip_prefix("Name=") {
                current_name = Some(v);
            } else if let Some(v) = line.strip_prefix("Path=") {
                current_path = Some(v);
            }
        }
    }
    // Check the last section.
    if in_profile_section && current_name == Some(name) {
        current_path.and_then(path_basename)
    } else {
        None
    }
}

/// Determine the default Firefox profile ID from `profiles.ini` content.
///
/// Checks `[Install*]` sections for a `Default=<path>` key first (present in Firefox 67+,
/// reflects the most-recently-used default).  Falls back to a `[Profile*]` section marked
/// with `Default=1`.  Returns the directory basename of the located profile path.
#[must_use]
fn firefox_default_profile_id(ini_content: &str) -> Option<String> {
    // [Install<hash>] → Default=Profiles/abc123.default-release (most authoritative).
    let mut in_install = false;
    let mut install_section: Option<&str> = None;
    for line in ini_content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_install = line.starts_with("[Install");
            if in_install {
                install_section = Some(line);
            }
        } else if in_install {
            let Some(path) = line.strip_prefix("Default=") else {
                continue;
            };
            tracing::info!(
                section = ?install_section,
                path,
                "Found default profile in [Install*] section"
            );
            return path_basename(path);
        }
    }
    tracing::info!("No [Install*] section with Default= found; trying [Profile*] with Default=1");

    // Fallback: [Profile*] with Default=1.
    let mut in_profile = false;
    let mut current_path: Option<&str> = None;
    let mut is_default = false;
    for line in ini_content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if in_profile && is_default {
                return current_path.and_then(path_basename);
            }
            in_profile = line.starts_with("[Profile");
            current_path = None;
            is_default = false;
        } else if in_profile {
            if line == "Default=1" {
                is_default = true;
            } else if let Some(v) = line.strip_prefix("Path=") {
                current_path = Some(v);
            }
        }
    }
    if in_profile && is_default {
        current_path.and_then(path_basename)
    } else {
        None
    }
}

/// Which browser family the parent process belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserKind {
    /// Firefox or a Firefox-compatible fork (LibreWolf, Waterfox, …).
    Firefox,
    /// Chrome, Chromium, or a Chromium-based browser (Brave, Edge, …).
    Chrome,
    /// Browser family could not be determined from the command line.
    Unknown,
}

/// Detect the browser family from the parent process command-line arguments.
///
/// Inspects `argv[0]` (the executable name / path) for well-known substrings that
/// identify Firefox-family or Chrome-family browsers.
fn detect_browser_from_cmdline(args: &[&str]) -> BrowserKind {
    let exe = args.first().copied().unwrap_or("").to_ascii_lowercase();
    let exe_name = std::path::Path::new(&exe)
        .file_name()
        .and_then(|n| n.to_str())
        .map_or_else(|| exe.clone(), str::to_ascii_lowercase);
    if exe_name.contains("firefox")
        || exe_name.contains("librewolf")
        || exe_name.contains("waterfox")
    {
        return BrowserKind::Firefox;
    }
    if exe_name.contains("chrome")
        || exe_name.contains("chromium")
        || exe_name.contains("brave")
        || exe_name.contains("msedge")
        || exe_name.contains("opera")
        || exe_name.contains("vivaldi")
    {
        return BrowserKind::Chrome;
    }
    BrowserKind::Unknown
}

/// Determine the Chrome/Chromium profile ID from the parent process command-line arguments.
///
/// Resolution order:
/// 1. `--profile-directory=<name>` present → return that value.
/// 2. `--user-data-dir=<path>` present → return the basename of the path.
/// 3. Neither flag present → return `Some("Default".to_owned())`.
fn chrome_profile_id_from_args(args: &[&str]) -> Option<String> {
    let mut profile_dir: Option<&str> = None;
    let mut user_data_dir: Option<&str> = None;

    for arg in args {
        if let Some(val) = arg.strip_prefix("--profile-directory=") {
            profile_dir = Some(val);
        } else if let Some(val) = arg.strip_prefix("--user-data-dir=") {
            user_data_dir = Some(val);
        }
    }

    if let Some(dir) = profile_dir {
        tracing::info!(
            profile_dir = dir,
            "Chrome profile from --profile-directory flag"
        );
        return Some(dir.to_owned());
    }
    if let Some(dir) = user_data_dir {
        tracing::info!(
            user_data_dir = dir,
            "Chrome profile from --user-data-dir flag basename"
        );
        return path_basename(dir);
    }
    tracing::info!("No Chrome profile flags found; using Default");
    Some("Default".to_owned())
}

/// Return the parent process ID of this process.
///
/// # Errors
///
/// Returns an error if the parent PID cannot be determined.
fn parent_pid() -> Result<u32, Error> {
    let sys = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::nothing()),
    );
    let our_pid = sysinfo::get_current_pid()
        .map_err(|e| Error::NoPpid(format!("cannot determine own PID: {e}")))?;
    sys.process(our_pid)
        .and_then(sysinfo::Process::parent)
        .map(|p| p.as_u32())
        .ok_or_else(|| Error::NoPpid("parent process not found in process list".into()))
}

/// Determine the browser profile ID for the process with the given PID.
///
/// Uses sysinfo for cross-platform process command-line access.
///
/// Returns `None` if the command line cannot be read or no profile can be determined.
fn read_parent_profile_id(ppid: u32) -> Option<String> {
    let sys = sysinfo::System::new_with_specifics(sysinfo::RefreshKind::nothing().with_processes(
        sysinfo::ProcessRefreshKind::nothing().with_cmd(sysinfo::UpdateKind::Always),
    ));
    let process = sys.process(sysinfo::Pid::from_u32(ppid))?;
    let args: Vec<&str> = process.cmd().iter().filter_map(|s| s.to_str()).collect();

    tracing::debug!(args = ?args, "Parent process command line");

    let browser_kind = detect_browser_from_cmdline(&args);
    tracing::debug!(browser_kind = ?browser_kind, "Detected browser kind from cmdline");

    match browser_kind {
        BrowserKind::Chrome => chrome_profile_id_from_args(&args),
        BrowserKind::Unknown => {
            tracing::info!("Unknown browser kind; profile ID unavailable");
            None
        }
        BrowserKind::Firefox => {
            for window in args.windows(2) {
                if let [flag, value] = window {
                    if *flag == "--profile" || *flag == "-profile" {
                        tracing::info!(path = *value, "Profile determined from cmdline flag");
                        return path_basename(value);
                    }
                    if *flag == "-P" {
                        tracing::info!(
                            name = *value,
                            "Named profile from -P flag; looking up in profiles.ini"
                        );
                        let ini = read_firefox_profiles_ini()?;
                        return firefox_profile_id_from_name(&ini, value);
                    }
                }
            }

            // No explicit profile flag: fall back to the default profile.
            tracing::info!("No profile flag in cmdline; reading default profile from profiles.ini");
            let ini = read_firefox_profiles_ini()?;
            let result = firefox_default_profile_id(&ini);
            tracing::info!(profile_id = ?result, "Profile ID from profiles.ini");
            result
        }
    }
}

/// Return the directory in which IPC socket/marker files are stored.
///
/// Delegates to [`browser_controller_client::socket_dir`].
///
/// # Errors
///
/// Returns [`Error::NoRuntimeDir`] when the platform base directory cannot be determined.
fn socket_dir() -> Result<std::path::PathBuf, Error> {
    browser_controller_client::socket_dir().map_err(|_e| Error::NoRuntimeDir)
}

/// Determine the IPC path for the given parent PID.
///
/// On Unix this is an actual socket file (`<ppid>.sock`).
/// On Windows this is an empty marker file (`<ppid>.pipe`) used for discovery;
/// the named pipe itself is `\\.\pipe\browser-controller-<ppid>`.
///
/// # Errors
///
/// Returns an error if the socket directory cannot be determined or created.
fn ipc_path(ppid_u32: u32) -> Result<std::path::PathBuf, Error> {
    let dir = socket_dir()?;
    fs_err::create_dir_all(&dir)?;
    #[cfg(unix)]
    return Ok(dir.join(format!("{ppid_u32}.sock")));
    #[cfg(windows)]
    return Ok(dir.join(format!("{ppid_u32}.pipe")));
}

/// Core mediator logic: run the main event loop.
///
/// # Errors
///
/// Returns an error if the event loop encounters an unrecoverable failure.
async fn run() -> Result<(), Error> {
    let ppid_u32 = parent_pid()?;

    tracing::info!(ppid = ppid_u32, "Mediator started");

    let sock_path = ipc_path(ppid_u32)?;

    // Channel carrying accepted connections (type-erased) to the main select! loop.
    let (conn_tx, mut conn_rx) = mpsc::channel::<Result<Box<dyn BiDiStream>, std::io::Error>>(32);

    // Remove a stale socket/marker from a previous run if present.
    match fs_err::remove_file(&sock_path) {
        Ok(()) => tracing::debug!(path = %sock_path.display(), "Removed stale socket file"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Io(e)),
    }

    // Platform-specific listener setup.
    #[cfg(unix)]
    {
        let listener = tokio::net::UnixListener::bind(&sock_path)?;
        tracing::info!(path = %sock_path.display(), "Listening on UDS socket");
        tokio::spawn(async move {
            loop {
                let result = listener
                    .accept()
                    .await
                    .map(|(conn, _addr)| -> Box<dyn BiDiStream> { Box::new(conn) });
                if conn_tx.send(result).await.is_err() {
                    break;
                }
            }
        });
    }

    #[cfg(windows)]
    {
        let pipe_name = format!(r"\\.\pipe\browser-controller-{ppid_u32}");
        // Write an empty marker file for CLI discovery.
        fs_err::write(&sock_path, &[])?;
        tracing::info!(
            path = %sock_path.display(),
            pipe = %pipe_name,
            "Listening on named pipe",
        );
        let pipe_name_owned = pipe_name.clone();
        tokio::spawn(async move {
            use tokio::net::windows::named_pipe::ServerOptions;
            loop {
                let server = match ServerOptions::new()
                    .first_pipe_instance(false)
                    .create(&pipe_name_owned)
                {
                    Ok(s) => s,
                    Err(e) => {
                        if conn_tx.send(Err(e)).await.is_err() {
                            break;
                        }
                        continue;
                    }
                };
                match server.connect().await {
                    Ok(()) => {
                        let boxed: Box<dyn BiDiStream> = Box::new(server);
                        if conn_tx.send(Ok(boxed)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        if conn_tx.send(Err(e)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }

    // Keep the socket/marker file alive for the full duration of run().
    let _socket_guard = SocketGuard {
        path: sock_path.clone(),
    };

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::BufWriter::new(tokio::io::stdout());

    // Channel from the stdin reader task to the main loop.
    let (ext_msg_tx, mut ext_msg_rx) = mpsc::channel::<Result<Option<ExtensionMessage>, Error>>(32);

    // Channel from CLI connection tasks to the main loop.
    let (request_tx, mut request_rx) = mpsc::channel::<PendingRequest>(32);

    // Broadcast channel for fanning out browser events to all event-stream subscribers.
    let (event_tx, _initial_rx) = broadcast::channel::<BrowserEvent>(256);

    // Spawn the stdin reader task so that non-cancellation-safe reads do not block
    // the main select! loop.
    tokio::spawn(async move {
        loop {
            let result = read_native_message(&mut stdin).await;
            let is_done = matches!(result, Ok(None) | Err(_));
            if ext_msg_tx.send(result).await.is_err() {
                break;
            }
            if is_done {
                break;
            }
        }
    });

    // Wait for the initial Hello from the extension.
    let browser_info = loop {
        match ext_msg_rx.recv().await {
            Some(Ok(Some(ExtensionMessage::Hello(hello)))) => {
                let profile_id = read_parent_profile_id(ppid_u32);
                let info = browser_controller_types::BrowserInfo::new(
                    hello.browser_name.clone(),
                    hello.browser_vendor.clone(),
                    hello.browser_version.clone(),
                    ppid_u32,
                    profile_id.clone(),
                );
                let vendor_str = hello
                    .browser_vendor
                    .as_deref()
                    .map(|v| format!(" ({v})"))
                    .unwrap_or_default();
                let profile_str = profile_id
                    .as_deref()
                    .map(|p| format!(", profile {p}"))
                    .unwrap_or_default();
                tracing::info!(
                    mediator_pid = std::process::id(),
                    browser_name = %hello.browser_name,
                    browser_vendor = ?hello.browser_vendor,
                    browser_version = %hello.browser_version,
                    browser_pid = ppid_u32,
                    profile_id = ?profile_id,
                    "Connected to browser instance: {}{} {}, pid {}{}", hello.browser_name, vendor_str, hello.browser_version, ppid_u32, profile_str,
                );
                break info;
            }
            Some(Ok(Some(ExtensionMessage::Response(r)))) => {
                tracing::warn!(
                    request_id = %r.request_id,
                    "Received Response before Hello; ignoring",
                );
            }
            Some(Ok(Some(ExtensionMessage::Event { .. }))) => {
                tracing::debug!("Received browser event before Hello; ignoring");
            }
            Some(Ok(Some(_))) => {
                tracing::debug!("Received unknown extension message before Hello; ignoring");
            }
            Some(Ok(None)) | None => return Err(Error::NativeMessagingClosedBeforeHello),
            Some(Err(e)) => return Err(e),
        }
    };

    // Map from request_id to the oneshot sender waiting for its response.
    let mut pending: HashMap<String, oneshot::Sender<CliOutcome>> = HashMap::new();

    loop {
        tokio::select! {
            // Message from extension via stdin reader task.
            msg = ext_msg_rx.recv() => {
                match msg {
                    Some(Ok(Some(ExtensionMessage::Response(resp)))) => {
                        tracing::debug!(request_id = %resp.request_id, "Received response from extension");
                        if let Some(tx) = pending.remove(&resp.request_id) {
                            drop(tx.send(resp.outcome));
                        } else {
                            tracing::warn!(
                                request_id = %resp.request_id,
                                "Received response for unknown request ID",
                            );
                        }
                    }
                    Some(Ok(Some(ExtensionMessage::Hello(_)))) => {
                        tracing::warn!("Received unexpected Hello after initial handshake");
                    }
                    Some(Ok(Some(ExtensionMessage::Event { event }))) => {
                        tracing::debug!("Broadcasting browser event");
                        // .send() returns Err only if there are no receivers; that's fine.
                        drop(event_tx.send(event));
                    }
                    Some(Ok(Some(_))) => {
                        tracing::debug!("Received unknown extension message; ignoring");
                    }
                    Some(Ok(None)) | None => {
                        tracing::info!("Extension disconnected; shutting down");
                        break;
                    }
                    Some(Err(e)) => {
                        tracing::error!(error = %e, "Error reading from extension; shutting down");
                        break;
                    }
                }
            }

            // Request from a CLI connection task.
            Some(pending_req) = request_rx.recv() => {
                let request_id = pending_req.request_id.clone();
                tracing::debug!(request_id = %request_id, "Forwarding request to extension");

                // Handle GetBrowserInfo locally without a round-trip to the extension.
                if matches!(&pending_req.command, browser_controller_types::CliCommand::GetBrowserInfo) {
                    let outcome = CliOutcome::Ok(
                        browser_controller_types::CliResult::BrowserInfo(browser_info.clone()),
                    );
                    drop(pending_req.response_tx.send(outcome));
                } else {
                    let ext_req = CliRequest::new(request_id.clone(), pending_req.command);
                    match write_native_message(&mut stdout, &ext_req).await {
                        Ok(()) => {
                            let _prev = pending.insert(request_id, pending_req.response_tx);
                        }
                        Err(e) => {
                            tracing::error!(
                                request_id = %request_id,
                                error = %e,
                                "Failed to write request to extension",
                            );
                            let outcome = CliOutcome::Err(format!("mediator write error: {e}"));
                            drop(pending_req.response_tx.send(outcome));
                        }
                    }
                }
            }

            // New CLI client connection.
            Some(accept_result) = conn_rx.recv() => {
                match accept_result {
                    Ok(conn) => {
                        tracing::debug!("Accepted new CLI client connection");
                        let request_tx_clone = request_tx.clone();
                        let event_tx_clone = event_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_cli_connection(conn, request_tx_clone, event_tx_clone).await {
                                tracing::warn!(error = %e, "CLI connection error");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to accept CLI connection");
                    }
                }
            }
        }
    }

    Ok(())
}

/// Entry point.
///
/// Sets up tracing to journald (Linux), a log file, and stderr, then delegates to [`run`].
#[expect(
    clippy::print_stderr,
    reason = "stderr is used for critical diagnostic messages before and after the logging system is initialized"
)]
#[tokio::main]
async fn main() {
    let terminal_env_filter = match EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .parse(std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_owned()))
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to parse RUST_LOG: {e}");
            std::process::exit(1);
        }
    };

    let registry = Registry::default();
    let registry = registry.with(
        tracing_subscriber::fmt::Layer::default()
            .with_writer(std::io::stderr)
            .with_filter(terminal_env_filter),
    );

    let file_layer = match EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse(
            std::env::var("BROWSER_CONTROLLER_MEDIATOR_LOG").unwrap_or_else(|_| "trace".to_owned()),
        ) {
        Err(e) => {
            eprintln!("Failed to parse BROWSER_CONTROLLER_MEDIATOR_LOG: {e}");
            std::process::exit(1);
        }
        Ok(filter) => std::env::var("BROWSER_CONTROLLER_MEDIATOR_LOG_DIR")
            .ok()
            .map(|log_dir| {
                let log_file = std::env::var("BROWSER_CONTROLLER_MEDIATOR_LOG_FILE")
                    .unwrap_or_else(|_| "browser_controller_mediator.log".to_owned());
                let appender = tracing_appender::rolling::never(log_dir, log_file);
                tracing_subscriber::fmt::Layer::default()
                    .with_writer(appender)
                    .with_filter(filter)
            }),
    };
    let registry = registry.with(file_layer);

    // Set up journald logging on Linux.
    #[cfg(target_os = "linux")]
    let registry = {
        let journald_filter = match EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .parse(
                std::env::var("BROWSER_CONTROLLER_MEDIATOR_JOURNALD_LOG")
                    .unwrap_or_else(|_| "info".to_owned()),
            ) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to parse BROWSER_CONTROLLER_MEDIATOR_JOURNALD_LOG: {e}");
                std::process::exit(1);
            }
        };
        let journald_layer = tracing_journald::layer()
            .ok()
            .map(|l| l.with_filter(journald_filter));
        registry.with(journald_layer)
    };

    registry.init();
    log_panics::init();

    if let Err(e) = run().await {
        tracing::error!(error = %e, "Mediator failed");
        eprintln!("Mediator error: {e}");
        std::process::exit(1);
    }

    tracing::debug!("Mediator exiting normally");
}

#[cfg(test)]
mod test {
    //use super::*;
    //use pretty_assertions::{assert_eq, assert_ne};
}
