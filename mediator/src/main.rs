//! Browser Controller Mediator — native messaging host that bridges the Firefox extension
//! to CLI clients via a Unix Domain Socket.
//!
//! Firefox launches this binary as a native messaging host when the extension calls
//! `browser.runtime.connectNative("browser_controller")`. The mediator then:
//!
//! 1. Reads the browser's identity from an initial `Hello` message.
//! 2. Creates a Unix Domain Socket in `$XDG_RUNTIME_DIR/browser-controller/<ppid>.sock`.
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

    /// `XDG_RUNTIME_DIR` is not set and no usable fallback is available.
    #[error("XDG_RUNTIME_DIR is not set; cannot determine socket directory")]
    NoRuntimeDir,

    /// The parent process ID could not be converted to `u32`.
    #[error("parent process ID {pid} is not a valid u32: {source}")]
    InvalidPpid {
        /// The raw (signed) PID value.
        pid: i32,
        /// The underlying conversion error.
        #[source]
        source: std::num::TryFromIntError,
    },

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

/// RAII guard that removes the UDS socket file on drop.
struct SocketGuard {
    /// Path to the socket file to clean up.
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
async fn read_cli_request(
    reader: &mut tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
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
async fn write_cli_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
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
async fn handle_cli_connection(
    conn: tokio::net::UnixStream,
    request_tx: mpsc::Sender<PendingRequest>,
    event_tx: broadcast::Sender<BrowserEvent>,
) -> Result<(), Error> {
    let (read_half, mut write_half) = conn.into_split();
    let mut reader = tokio::io::BufReader::new(read_half);
    loop {
        let Some(cli_request) = read_cli_request(&mut reader).await? else {
            break;
        };

        // SubscribeEvents is handled locally — stream events until the client disconnects.
        if matches!(
            &cli_request.command,
            browser_controller_types::CliCommand::SubscribeEvents
        ) {
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
        let response = CliResponse {
            request_id,
            outcome,
        };
        write_cli_response(&mut write_half, &response).await?;
    }
    Ok(())
}

/// Return the final path component of a UTF-8 path string as an owned `String`.
///
/// Returns `None` if the path has no file name component or it is not valid UTF-8.
#[cfg(target_os = "linux")]
#[must_use]
fn path_basename(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_owned())
}

/// Locate and read the Firefox `profiles.ini` file.
///
/// Tries, in order:
/// - `~/.mozilla/firefox/profiles.ini` (standard installation)
/// - `~/snap/firefox/common/.mozilla/firefox/profiles.ini` (Snap package)
/// - `~/.var/app/org.mozilla.firefox/.mozilla/firefox/profiles.ini` (Flatpak)
///
/// Falls back to the `$HOME` environment variable if the `directories` crate cannot
/// determine the home directory.  Returns `None` if no readable file is found.
#[cfg(target_os = "linux")]
#[must_use]
fn read_firefox_profiles_ini() -> Option<String> {
    let home = directories::BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .or_else(|| std::env::var("HOME").ok().map(std::path::PathBuf::from))?;

    let candidates = [
        home.join(".mozilla/firefox/profiles.ini"),
        home.join("snap/firefox/common/.mozilla/firefox/profiles.ini"),
        home.join(".var/app/org.mozilla.firefox/.mozilla/firefox/profiles.ini"),
    ];

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
#[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
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

/// Determine the Firefox profile ID for the process with the given PID.
///
/// Resolution order:
/// 1. `--profile <path>` in the process command line → basename of the path.
/// 2. `-P <name>` in the command line → look up the named profile in `profiles.ini`.
/// 3. No profile flag → read the default profile from `profiles.ini`
///    (`[Install*]` → `Default=`, or `[Profile*]` with `Default=1`).
///
/// Returns `None` if the command line cannot be read, no profile can be determined,
/// or `profiles.ini` is absent/unreadable.
#[cfg(target_os = "linux")]
fn read_parent_profile_id(ppid: u32) -> Option<String> {
    let cmdline = match fs_err::read(format!("/proc/{ppid}/cmdline")) {
        Ok(c) => c,
        Err(e) => {
            tracing::info!(ppid, error = %e, "Cannot read parent process cmdline; profile ID unavailable");
            return None;
        }
    };
    // NUL-separated, NUL-terminated; skip empty trailing entries.
    let args: Vec<&str> = cmdline
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|s| std::str::from_utf8(s).ok())
        .collect();

    tracing::debug!(args = ?args, "Parent process command line");

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

/// Returns `None` on non-Linux platforms where `/proc` is unavailable.
#[cfg(not(target_os = "linux"))]
fn read_parent_profile_id(_ppid: u32) -> Option<String> {
    None
}

/// Determine the UDS socket path from the parent PID and `XDG_RUNTIME_DIR`.
///
/// # Errors
///
/// Returns an error if `XDG_RUNTIME_DIR` is not set, the parent PID is invalid, or the
/// socket directory cannot be created.
fn socket_path(ppid_u32: u32) -> Result<std::path::PathBuf, Error> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").map_err(|_not_set| Error::NoRuntimeDir)?;
    let socket_dir = std::path::Path::new(&runtime_dir).join("browser-controller");
    fs_err::create_dir_all(&socket_dir)?;
    Ok(socket_dir.join(format!("{ppid_u32}.sock")))
}

/// Core mediator logic: run the main event loop.
///
/// # Errors
///
/// Returns an error if the event loop encounters an unrecoverable failure.
async fn run() -> Result<(), Error> {
    let ppid_raw = nix::unistd::getppid().as_raw();
    let ppid_u32 = u32::try_from(ppid_raw).map_err(|source| Error::InvalidPpid {
        pid: ppid_raw,
        source,
    })?;

    tracing::info!(ppid = ppid_u32, "Mediator started");

    let sock_path = socket_path(ppid_u32)?;

    // Remove a stale socket from a previous run if present.
    match fs_err::remove_file(&sock_path) {
        Ok(()) => tracing::debug!(path = %sock_path.display(), "Removed stale socket file"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Io(e)),
    }

    let listener = tokio::net::UnixListener::bind(&sock_path)?;
    let _socket_guard = SocketGuard {
        path: sock_path.clone(),
    };
    tracing::info!(path = %sock_path.display(), "Listening on UDS socket");

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
                let info = browser_controller_types::BrowserInfo {
                    browser_name: hello.browser_name.clone(),
                    browser_vendor: hello.browser_vendor.clone(),
                    browser_version: hello.browser_version.clone(),
                    pid: ppid_u32,
                    profile_id: profile_id.clone(),
                };
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
                    let ext_req = CliRequest {
                        request_id: request_id.clone(),
                        command: pending_req.command,
                    };
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
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((conn, _addr)) => {
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
    // Uses `.ok().map()` so both the Some and None arms have the same Option<Filtered<…>> type.
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
