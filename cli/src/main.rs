//! Browser Controller CLI — control Firefox windows and tabs from the command line.
//!
//! Connects to a running `browser-controller-mediator` instance via Unix Domain Socket
//! and issues commands to control the browser.

use std::time::Duration;

use browser_controller_types::{
    BrowserInfo, CliCommand, CliOutcome, CliRequest, CliResponse, CliResult,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _};
use tracing_subscriber::{
    EnvFilter, Layer as _, Registry, filter::LevelFilter, layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

/// Timeout for connecting to and querying a single mediator instance during discovery.
const INSTANCE_QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// Errors that can occur in the CLI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An I/O error occurred (covers both network and filesystem operations).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// A background task panicked or was cancelled.
    #[error("background task error: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    /// The runtime/temp directory cannot be determined.
    #[error("cannot determine runtime/temp directory for IPC sockets")]
    NoRuntimeDir,

    /// Writing a Windows registry key for native messaging host registration failed.
    #[cfg(target_os = "windows")]
    #[error("failed to write Windows registry key: {0}")]
    RegistryWriteFailed(#[source] std::io::Error),

    /// No running mediator instance was found.
    #[error("no browser-controller mediator is running (no sockets in {dir})")]
    NoInstances {
        /// The directory that was searched.
        dir: std::path::PathBuf,
    },

    /// Multiple instances are running and no selector was provided.
    #[error(
        "multiple browser instances are running; use --instance <pid|browser-name> to select one"
    )]
    MultipleInstances,

    /// The specified instance selector matched no running instance.
    #[error("no browser instance matches '{selector}'")]
    NoMatchingInstance {
        /// The selector that was provided.
        selector: String,
    },

    /// The specified browser name matched more than one running instance.
    #[error("multiple browser instances match '{selector}'; use --instance <pid> to disambiguate")]
    AmbiguousInstance {
        /// The selector that matched multiple instances.
        selector: String,
    },

    /// The browser command returned an error.
    #[error("command failed: {0}")]
    CommandFailed(String),

    /// A log filter expression could not be parsed.
    #[error("failed to parse log filter: {0}")]
    LogFilter(#[from] tracing_subscriber::filter::ParseError),

    /// Man page generation failed.
    #[error("failed to generate man page: {0}")]
    GenerateManpage(#[source] std::io::Error),

    /// Shell completion generation failed.
    #[error("failed to generate shell completion: {0}")]
    GenerateShellCompletion(#[source] std::io::Error),

    /// The user's home directory could not be determined.
    #[error("could not determine home directory for manifest installation")]
    NoBrowserHome,

    /// No mediator binary path was given and none could be found automatically.
    #[error(
        "mediator binary not found next to this executable; use --mediator-path to specify its location"
    )]
    MediatorNotFound,

    /// `--extension-id` is required for Chromium-family browsers but was not supplied.
    #[error(
        "Chromium-family browsers require --extension-id; \
         find the ID on chrome://extensions after loading the unpacked extension \
         (a 32-character lowercase letter string)"
    )]
    ChromiumExtensionIdRequired,
}

/// The native messaging protocol family, which determines the JSON manifest format.
///
/// Each family uses a different field to restrict which browser extension may connect:
/// Gecko uses `allowed_extensions` (extension IDs), Chromium uses `allowed_origins`
/// (extension origin URLs). New browser families can be added here without changing
/// [`BrowserTarget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserFamily {
    /// Gecko-based browsers (Firefox and its forks); manifest uses `allowed_extensions`.
    Gecko,
    /// Chromium-based browsers (Chrome, Chromium, Brave, Edge, …); manifest uses `allowed_origins`.
    Chromium,
}

/// Browser to install the native messaging host manifest for.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserTarget {
    /// Mozilla Firefox.
    Firefox,
    /// LibreWolf (privacy-focused Firefox fork).
    Librewolf,
    /// Waterfox (performance-focused Firefox fork).
    Waterfox,
    /// Google Chrome.
    Chrome,
    /// Chromium (open-source Chrome base).
    Chromium,
    /// Brave Browser (privacy-focused Chromium fork).
    Brave,
    /// Microsoft Edge (Chromium-based).
    Edge,
}

impl BrowserTarget {
    /// Return the native messaging protocol family used by this browser.
    ///
    /// The family determines which JSON manifest format is written.
    #[must_use]
    const fn family(self) -> BrowserFamily {
        match self {
            Self::Firefox | Self::Librewolf | Self::Waterfox => BrowserFamily::Gecko,
            Self::Chrome | Self::Chromium | Self::Brave | Self::Edge => BrowserFamily::Chromium,
        }
    }

    /// Return the directory where this browser looks for native messaging host manifests.
    #[must_use]
    fn manifest_dir(self, base: &directories::BaseDirs) -> std::path::PathBuf {
        let home = base.home_dir();

        #[cfg(target_os = "linux")]
        return match self {
            Self::Firefox => home.join(".mozilla/native-messaging-hosts"),
            Self::Librewolf => home.join(".librewolf/native-messaging-hosts"),
            Self::Waterfox => home.join(".waterfox/native-messaging-hosts"),
            Self::Chrome => home.join(".config/google-chrome/NativeMessagingHosts"),
            Self::Chromium => home.join(".config/chromium/NativeMessagingHosts"),
            Self::Brave => home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            Self::Edge => home.join(".config/microsoft-edge/NativeMessagingHosts"),
        };

        #[cfg(target_os = "macos")]
        return match self {
            Self::Firefox => home.join("Library/Application Support/Mozilla/NativeMessagingHosts"),
            Self::Librewolf => {
                home.join("Library/Application Support/librewolf/NativeMessagingHosts")
            }
            Self::Waterfox => {
                home.join("Library/Application Support/Waterfox/NativeMessagingHosts")
            }
            Self::Chrome => {
                home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
            }
            Self::Chromium => {
                home.join("Library/Application Support/Chromium/NativeMessagingHosts")
            }
            Self::Brave => home.join(
                "Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts",
            ),
            Self::Edge => {
                home.join("Library/Application Support/Microsoft Edge/NativeMessagingHosts")
            }
        };

        // Windows: JSON manifest file lives under APPDATA or LOCALAPPDATA.
        // A registry key also points to it (written in install_manifest).
        #[cfg(target_os = "windows")]
        {
            let appdata = std::env::var("APPDATA").unwrap_or_default();
            let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
            match self {
                Self::Firefox | Self::Librewolf | Self::Waterfox => {
                    std::path::Path::new(&appdata).join("Mozilla/NativeMessagingHosts")
                }
                Self::Chrome => {
                    std::path::Path::new(&localappdata).join("Google/Chrome/NativeMessagingHosts")
                }
                Self::Chromium => {
                    std::path::Path::new(&localappdata).join("Chromium/NativeMessagingHosts")
                }
                Self::Brave => std::path::Path::new(&localappdata)
                    .join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
                Self::Edge => {
                    std::path::Path::new(&localappdata).join("Microsoft/Edge/NativeMessagingHosts")
                }
            }
        }
    }

    /// Return the Windows registry subkey path for this browser's native messaging host.
    ///
    /// The key is written under `HKEY_CURRENT_USER` during `install-manifest`.
    #[cfg(target_os = "windows")]
    #[must_use]
    const fn windows_registry_key(self) -> &'static str {
        match self {
            Self::Firefox | Self::Librewolf | Self::Waterfox => {
                r"Software\Mozilla\NativeMessagingHosts\browser_controller"
            }
            Self::Chrome => r"Software\Google\Chrome\NativeMessagingHosts\browser_controller",
            Self::Chromium => r"Software\Chromium\NativeMessagingHosts\browser_controller",
            Self::Brave => {
                r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\browser_controller"
            }
            Self::Edge => r"Software\Microsoft\Edge\NativeMessagingHosts\browser_controller",
        }
    }
}

/// Output format for command results.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable formatted output.
    Human,
    /// Machine-readable JSON output.
    Json,
}

/// Top-level CLI options.
#[derive(clap::Parser, Debug)]
#[clap(
    name = "browser-controller",
    about = clap::crate_description!(),
    author = clap::crate_authors!(),
    version = clap::crate_version!()
)]
struct Cli {
    /// Which subcommand to run.
    #[clap(subcommand)]
    command: Command,

    /// Output format.
    #[clap(long, short = 'o', default_value = "human", global = true)]
    output: OutputFormat,

    /// Select a browser instance by PID (numeric) or browser name (case-insensitive substring).
    ///
    /// If omitted and exactly one instance is running, it is selected automatically.
    #[clap(long, short = 'i', global = true)]
    instance: Option<String>,
}

/// Available commands.
#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// List all running browser instances.
    Instances,
    /// Stream browser events as newline-delimited JSON.
    ///
    /// Prints one JSON object per line for each browser event (window/tab open, close,
    /// navigation, title change, active tab change) until interrupted.
    ///
    /// Multiple `event-stream` processes for the same browser instance are supported.
    EventStream,
    /// Manage browser windows.
    Windows(WindowsArgs),
    /// Manage tabs within a browser window.
    Tabs(TabsArgs),
    /// Generate a man page for this tool.
    GenerateManpage {
        /// Directory in which to write the generated man page.
        #[clap(long)]
        output_dir: std::path::PathBuf,
    },
    /// Generate shell completion scripts.
    GenerateShellCompletion {
        /// File to write the completion script to.
        #[clap(long)]
        output_file: std::path::PathBuf,
        /// Shell to generate completions for.
        #[clap(long)]
        shell: clap_complete::aot::Shell,
    },
    /// Install the native messaging host manifest for a browser.
    ///
    /// The manifest tells the browser where to find the `browser-controller-mediator`
    /// binary when the extension requests a native messaging connection.
    InstallManifest {
        /// Browser to install the manifest for.
        #[clap(long)]
        browser: BrowserTarget,
        /// Path to the `browser-controller-mediator` binary.
        ///
        /// If omitted, the CLI looks for `browser-controller-mediator` next to its
        /// own executable.
        #[clap(long)]
        mediator_path: Option<std::path::PathBuf>,
        /// Chromium extension ID (required for Chrome, Chromium, Brave, Edge).
        ///
        /// Find this on chrome://extensions after loading the unpacked extension.
        /// It is a 32-character lowercase letter string, e.g.
        /// "abcdefghijklmnopabcdefghijklmnop". Not used for Gecko-family browsers.
        #[clap(long)]
        extension_id: Option<String>,
    },
}

/// Arguments for the `windows` subcommand group.
#[derive(clap::Args, Debug)]
pub struct WindowsArgs {
    /// Window operation to perform.
    #[clap(subcommand)]
    command: WindowsCommand,
}

/// Operations on browser windows.
#[derive(clap::Subcommand, Debug)]
pub enum WindowsCommand {
    /// List all open windows with their tabs.
    List,
    /// Open a new browser window.
    Open,
    /// Close a browser window.
    Close {
        /// ID of the window to close.
        #[clap(long)]
        window_id: u32,
    },
    /// Set the title prefix (Firefox `titlePreface`) for a window.
    SetTitlePrefix {
        /// ID of the window to modify.
        #[clap(long)]
        window_id: u32,
        /// Prefix to prepend to the window title.
        prefix: String,
    },
    /// Remove the title prefix from a window.
    RemoveTitlePrefix {
        /// ID of the window to modify.
        #[clap(long)]
        window_id: u32,
    },
}

/// Arguments for the `tabs` subcommand group.
#[derive(clap::Args, Debug)]
pub struct TabsArgs {
    /// Tab operation to perform.
    #[clap(subcommand)]
    command: TabsCommand,
}

/// Operations on browser tabs.
#[derive(clap::Subcommand, Debug)]
pub enum TabsCommand {
    /// List all tabs in a window with full details.
    List {
        /// ID of the window whose tabs to list.
        #[clap(long)]
        window_id: u32,
    },
    /// Open a new tab in a window.
    Open {
        /// ID of the window in which to open the tab.
        #[clap(long)]
        window_id: u32,
        /// Insert the new tab immediately before the tab with this ID.
        #[clap(long, conflicts_with = "after")]
        before: Option<u32>,
        /// Insert the new tab immediately after the tab with this ID.
        #[clap(long, conflicts_with = "before")]
        after: Option<u32>,
        /// URL to load in the new tab (defaults to the browser's new-tab page).
        #[clap(long)]
        url: Option<String>,
        /// After the tab finishes loading, strip any embedded `user:password@` credentials
        /// from the URL and navigate to the clean URL.
        ///
        /// Firefox caches the credentials from the initial load and uses them to satisfy
        /// future auth challenges automatically, while the tab ends up displaying the URL
        /// without visible credentials. Requires `--url`.
        #[clap(long, requires = "url")]
        strip_credentials: bool,
    },
    /// Activate a tab, making it the focused tab in its window.
    Activate {
        /// Internal browser tab ID to activate (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
    },
    /// Navigate an existing tab to a new URL.
    Navigate {
        /// Internal browser tab ID to navigate (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
        /// URL to load in the tab.
        #[clap(long)]
        url: String,
    },
    /// Close a tab.
    Close {
        /// Internal browser tab ID to close (shown in `tabs list`).
        tab_id: u32,
    },
    /// Pin a tab.
    Pin {
        /// Internal browser tab ID to pin (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
    },
    /// Unpin a tab.
    Unpin {
        /// Internal browser tab ID to unpin (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
    },
    /// Warm up a discarded tab, loading its content into memory without activating it.
    Warmup {
        /// Internal browser tab ID to warm up (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
    },
    /// Mute a tab, suppressing any audio it produces.
    Mute {
        /// Internal browser tab ID to mute (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
    },
    /// Unmute a tab, allowing it to produce audio again.
    Unmute {
        /// Internal browser tab ID to unmute (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
    },
    /// Move a tab to a new position within its window.
    Move {
        /// Internal browser tab ID to move (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
        /// New zero-based index for the tab within its window.
        #[clap(long)]
        new_index: u32,
    },
    /// Navigate backward in a tab's session history.
    Back {
        /// Internal browser tab ID (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
        /// Number of steps to go back.
        ///
        /// Values greater than 1 skip intermediate pages atomically, which is useful
        /// when those pages redirect immediately forward again.
        #[clap(long, default_value_t = 1u32)]
        steps: u32,
    },
    /// Navigate forward in a tab's session history.
    Forward {
        /// Internal browser tab ID (shown in `tabs list`).
        #[clap(long)]
        tab_id: u32,
        /// Number of steps to go forward.
        ///
        /// Values greater than 1 skip intermediate pages atomically, which is useful
        /// when those pages redirect immediately backward again.
        #[clap(long, default_value_t = 1u32)]
        steps: u32,
    },
}

/// A discovered mediator instance.
struct DiscoveredInstance {
    /// Path to the mediator's UDS socket.
    socket_path: std::path::PathBuf,
    /// Browser information returned by the mediator.
    info: BrowserInfo,
}

/// Serializable view of a discovered instance for JSON output.
#[derive(Debug, serde::Serialize)]
struct DiscoveredInstanceJson<'a> {
    /// Path to the mediator's UDS socket.
    socket_path: &'a std::path::Path,
    /// Human-readable browser name.
    browser_name: &'a str,
    /// Browser vendor (e.g. "Mozilla"), if reported.
    browser_vendor: Option<&'a str>,
    /// Browser version string.
    browser_version: &'a str,
    /// Browser main process PID.
    pid: u32,
    /// Firefox profile ID (directory basename), if available.
    profile_id: Option<&'a str>,
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
fn socket_dir() -> Result<std::path::PathBuf, Error> {
    #[cfg(target_os = "linux")]
    {
        let runtime_dir =
            std::env::var("XDG_RUNTIME_DIR").map_err(|_not_set| Error::NoRuntimeDir)?;
        Ok(std::path::Path::new(&runtime_dir).join("browser-controller"))
    }
    #[cfg(target_os = "macos")]
    {
        let dir = std::env::var("TMPDIR")
            .map(|t| std::path::Path::new(&t).join("browser-controller"))
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
        Ok(std::path::Path::new(&local)
            .join("Temp")
            .join("browser-controller"))
    }
}

/// File extension used for mediator IPC discovery files.
///
/// On Unix: `.sock` (the actual socket file).
/// On Windows: `.pipe` (empty marker file; named pipe is discovered from the stem).
#[cfg(unix)]
const SOCKET_EXT: &str = "sock";
#[cfg(windows)]
const SOCKET_EXT: &str = "pipe";

/// List all mediator IPC discovery files in `dir`.
///
/// # Errors
///
/// Returns an error if the directory cannot be read.
fn list_socket_files(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, Error> {
    let rd = match fs_err::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Error::Io(e)),
    };
    let mut paths = Vec::new();
    for entry in rd {
        let path = entry.map_err(Error::Io)?.path();
        if path.extension() == Some(std::ffi::OsStr::new(SOCKET_EXT)) {
            paths.push(path);
        }
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
async fn query_instance(socket_path: &std::path::Path) -> Result<BrowserInfo, Error> {
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
/// Returns an error if `XDG_RUNTIME_DIR` is not set or the directory cannot be read.
async fn discover_instances() -> Result<Vec<DiscoveredInstance>, Error> {
    let dir = socket_dir()?;
    let sock_paths = tokio::task::spawn_blocking({
        let dir = dir.clone();
        move || list_socket_files(&dir)
    })
    .await??;

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

/// Select an instance from the discovered list based on the `--instance` flag value.
///
/// # Errors
///
/// Returns an error if no instances are running, the selector is ambiguous, or no match is found.
fn select_instance<'a>(
    instances: &'a [DiscoveredInstance],
    selector: Option<&str>,
    socket_dir: &std::path::Path,
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
fn pipe_name_from_marker(path: &std::path::Path) -> Result<String, Error> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::Io(std::io::Error::other("invalid pipe marker path")))?;
    Ok(format!(r"\\.\pipe\browser-controller-{stem}"))
}

/// Send a command to a mediator and return the result.
///
/// # Errors
///
/// Returns an error if the connection, serialization, or communication fails, or if the
/// command itself fails.
async fn send_command(
    socket_path: &std::path::Path,
    command: CliCommand,
) -> Result<CliResult, Error> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let request = CliRequest {
        request_id: request_id.clone(),
        command,
    };

    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(socket_path).await?;
    #[cfg(windows)]
    let stream = {
        let pipe_name = pipe_name_from_marker(socket_path)?;
        tokio::net::windows::named_pipe::ClientOptions::new().open(&pipe_name)?
    };

    let (read_half, mut write_half) = tokio::io::split(stream);

    let mut json = serde_json::to_vec(&request)?;
    json.push(b'\n');
    write_half.write_all(&json).await?;

    let mut reader = tokio::io::BufReader::new(read_half);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response: CliResponse = serde_json::from_str(line.trim_end())?;

    if response.request_id != request_id {
        tracing::warn!(
            expected = %request_id,
            received = %response.request_id,
            "Response request_id mismatch",
        );
    }

    match response.outcome {
        CliOutcome::Ok(result) => Ok(result),
        CliOutcome::Err(msg) => Err(Error::CommandFailed(msg)),
    }
}

/// Print the `instances` list in human-readable format.
///
/// # Errors
///
/// Returns an error if writing to stdout fails.
#[expect(
    clippy::print_stdout,
    reason = "instances output goes to stdout by design"
)]
fn print_instances_human(instances: &[DiscoveredInstance]) -> Result<(), Error> {
    println!(
        "{:<8} {:<14} {:<10} {:<10} {:<30} SOCKET",
        "PID", "BROWSER", "VENDOR", "VERSION", "PROFILE"
    );
    for inst in instances {
        println!(
            "{:<8} {:<14} {:<10} {:<10} {:<30} {}",
            inst.info.pid,
            inst.info.browser_name,
            inst.info.browser_vendor.as_deref().unwrap_or("-"),
            inst.info.browser_version,
            inst.info.profile_id.as_deref().unwrap_or("-"),
            inst.socket_path.display(),
        );
    }
    Ok(())
}

/// Print the `instances` list as JSON.
///
/// # Errors
///
/// Returns an error if JSON serialization or writing to stdout fails.
#[expect(
    clippy::print_stdout,
    reason = "instances output goes to stdout by design"
)]
fn print_instances_json(instances: &[DiscoveredInstance]) -> Result<(), Error> {
    let data: Vec<DiscoveredInstanceJson<'_>> = instances
        .iter()
        .map(|i| DiscoveredInstanceJson {
            socket_path: &i.socket_path,
            browser_name: &i.info.browser_name,
            browser_vendor: i.info.browser_vendor.as_deref(),
            browser_version: &i.info.browser_version,
            pid: i.info.pid,
            profile_id: i.info.profile_id.as_deref(),
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

/// Print a [`CliResult`] in human-readable format.
///
/// # Errors
///
/// Returns an error if writing to stdout fails.
#[expect(
    clippy::print_stdout,
    reason = "command output goes to stdout by design"
)]
fn print_result_human(result: &CliResult) -> Result<(), Error> {
    match result {
        CliResult::BrowserInfo(info) => {
            println!("Browser: {} {}", info.browser_name, info.browser_version);
            if let Some(vendor) = &info.browser_vendor {
                println!("Vendor:  {vendor}");
            }
            println!("PID:     {}", info.pid);
            if let Some(profile) = &info.profile_id {
                println!("Profile: {profile}");
            }
        }
        CliResult::Windows { windows } => {
            for win in windows {
                let prefix_display = win
                    .title_prefix
                    .as_deref()
                    .map(|p| format!(" (prefix: {p:?})"))
                    .unwrap_or_default();
                let focused = if win.is_focused { ", focused" } else { "" };
                println!(
                    "Window {} — {:?}{}{} [{}]",
                    win.id, win.title, prefix_display, focused, win.state,
                );
                for tab in &win.tabs {
                    let active = if tab.is_active { "*" } else { " " };
                    println!("  {active}{:<4} {} — {}", tab.index, tab.title, tab.url);
                }
            }
        }
        CliResult::WindowId { window_id } => {
            println!("Opened window {window_id}");
        }
        CliResult::Tabs { tabs } => {
            for tab in tabs {
                let active = if tab.is_active { "[active]" } else { "       " };
                println!("Tab {:<4} {} {}", tab.index, active, tab.status,);
                println!("  ID:     {}", tab.id);
                println!("  Title:  {}", tab.title);
                println!("  URL:    {}", tab.url);
                let history = format_history(
                    tab.history_length,
                    tab.history_steps_back,
                    tab.history_steps_forward,
                    tab.history_hidden_count,
                );
                println!(
                    "  Pinned: {}  Discarded: {}  Audible: {}  Muted: {}  History: {}",
                    yn(tab.is_pinned),
                    yn(tab.is_discarded),
                    yn(tab.is_audible),
                    yn(tab.is_muted),
                    history,
                );
                println!(
                    "  Attention: {}  Awaiting auth: {}  Reader mode: {}  Incognito: {}",
                    yn(tab.has_attention),
                    yn(tab.is_awaiting_auth),
                    yn(tab.is_in_reader_mode),
                    yn(tab.incognito),
                );
            }
        }
        CliResult::Tab(tab) => {
            let active = if tab.is_active { "[active]" } else { "       " };
            println!("Tab {:<4} {} {}", tab.index, active, tab.status);
            println!("  ID:     {}", tab.id);
            println!("  Title:  {}", tab.title);
            println!("  URL:    {}", tab.url);
            let history = format_history(
                tab.history_length,
                tab.history_steps_back,
                tab.history_steps_forward,
                tab.history_hidden_count,
            );
            println!(
                "  Pinned: {}  Discarded: {}  Audible: {}  Muted: {}  History: {}",
                yn(tab.is_pinned),
                yn(tab.is_discarded),
                yn(tab.is_audible),
                yn(tab.is_muted),
                history,
            );
            println!(
                "  Attention: {}  Awaiting auth: {}  Reader mode: {}  Incognito: {}",
                yn(tab.has_attention),
                yn(tab.is_awaiting_auth),
                yn(tab.is_in_reader_mode),
                yn(tab.incognito),
            );
        }
        CliResult::Unit => {}
    }
    Ok(())
}

/// Return `"yes"` or `"no"` for human-readable boolean display.
#[must_use]
const fn yn(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

/// Format the session history depth for human-readable display.
///
/// When back/forward step counts are available (Firefox 125+) the output is
/// `"<back>←  <accessible>  →<forward>"`, with `(+<N> hidden)` appended when
/// cross-origin inaccessible entries are known to exist.  Falls back to just
/// the total length when the Navigation API is unavailable.
#[must_use]
fn format_history(
    length: u32,
    steps_back: Option<u32>,
    steps_forward: Option<u32>,
    hidden_count: Option<u32>,
) -> String {
    match (steps_back, steps_forward) {
        (Some(back), Some(forward)) => {
            let hidden = match hidden_count {
                Some(n) if n > 0 => format!(" (+{n} hidden)"),
                _ => String::new(),
            };
            format!("{back}\u{2190}  {length}  \u{2192}{forward}{hidden}")
        }
        _ => format!("{length}"),
    }
}

/// Print a [`CliResult`] as pretty-printed JSON.
///
/// # Errors
///
/// Returns an error if JSON serialization fails.
#[expect(
    clippy::print_stdout,
    reason = "command output goes to stdout by design"
)]
fn print_result_json(result: &CliResult) -> Result<(), Error> {
    println!("{}", serde_json::to_string_pretty(result)?);
    Ok(())
}

/// JSON structure of a Gecko-family native messaging host manifest.
///
/// Written to the browser's native-messaging-hosts directory as `browser_controller.json`.
#[derive(Debug, serde::Serialize)]
struct GeckoManifest<'a> {
    /// The registered name of the native messaging host.
    name: &'a str,
    /// Human-readable description of the host.
    description: &'a str,
    /// Absolute path to the native messaging host binary.
    path: &'a std::path::Path,
    /// Transport type; always `"stdio"` for native messaging hosts.
    #[serde(rename = "type")]
    kind: &'a str,
    /// Extension IDs allowed to connect to this host.
    allowed_extensions: &'a [&'a str],
}

/// JSON structure of a Chromium-family native messaging host manifest.
///
/// Written to the browser's NativeMessagingHosts directory as `browser_controller.json`.
/// Unlike the Gecko format, Chromium identifies allowed extensions by origin URL rather
/// than extension ID string.
#[derive(Debug, serde::Serialize)]
struct ChromiumManifest<'a> {
    /// The registered name of the native messaging host.
    name: &'a str,
    /// Human-readable description of the host.
    description: &'a str,
    /// Absolute path to the native messaging host binary.
    path: &'a std::path::Path,
    /// Transport type; always `"stdio"` for native messaging hosts.
    #[serde(rename = "type")]
    kind: &'a str,
    /// Extension origin URLs allowed to connect to this host.
    ///
    /// Each entry has the form `"chrome-extension://<extension-id>/"`.
    allowed_origins: &'a [String],
}

/// Result of a successful manifest installation, used for JSON output.
#[derive(Debug, serde::Serialize)]
struct InstallManifestResult<'a> {
    /// Absolute path where the manifest was written.
    manifest_path: &'a std::path::Path,
    /// Absolute path to the mediator binary recorded in the manifest.
    mediator_path: &'a std::path::Path,
}

/// Install the native messaging host manifest for the given browser.
///
/// # Errors
///
/// Returns an error if the home directory cannot be determined, the mediator binary cannot
/// be found automatically, the manifest directory cannot be created, the manifest file
/// cannot be written, or a Chromium-family browser is selected without `--extension-id`.
#[expect(
    clippy::print_stdout,
    reason = "manifest installation result goes to stdout by design"
)]
fn install_manifest(
    browser: BrowserTarget,
    mediator_path: Option<std::path::PathBuf>,
    extension_id: Option<String>,
    format: OutputFormat,
) -> Result<(), Error> {
    let base = directories::BaseDirs::new().ok_or(Error::NoBrowserHome)?;

    let mediator_path = match mediator_path {
        Some(p) => p,
        None => {
            let exe = std::env::current_exe()?;
            let candidate = exe
                .parent()
                .map(|dir| dir.join("browser-controller-mediator"));
            match candidate {
                Some(p) if p.exists() => p,
                _ => return Err(Error::MediatorNotFound),
            }
        }
    };

    let manifest_dir = browser.manifest_dir(&base);
    fs_err::create_dir_all(&manifest_dir).map_err(Error::Io)?;
    let manifest_path = manifest_dir.join("browser_controller.json");

    let json = match browser.family() {
        BrowserFamily::Gecko => {
            let manifest = GeckoManifest {
                name: "browser_controller",
                description: "Browser Controller Mediator",
                path: &mediator_path,
                kind: "stdio",
                allowed_extensions: &["browser-controller@taladar.net"],
            };
            serde_json::to_string_pretty(&manifest)?
        }
        BrowserFamily::Chromium => {
            let id = extension_id.ok_or(Error::ChromiumExtensionIdRequired)?;
            let origin = format!("chrome-extension://{id}/");
            let manifest = ChromiumManifest {
                name: "browser_controller",
                description: "Browser Controller Mediator",
                path: &mediator_path,
                kind: "stdio",
                allowed_origins: &[origin],
            };
            serde_json::to_string_pretty(&manifest)?
        }
    };

    fs_err::write(&manifest_path, json.as_bytes()).map_err(Error::Io)?;

    // On Windows, browsers find native messaging hosts exclusively via the registry.
    // Write the registry key pointing to the manifest file.
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(browser.windows_registry_key())
            .map_err(Error::RegistryWriteFailed)?;
        key.set_value("", &manifest_path.to_string_lossy().as_ref())
            .map_err(Error::RegistryWriteFailed)?;
    }

    match format {
        OutputFormat::Human => {
            println!("Installed manifest to {}", manifest_path.display());
            #[cfg(target_os = "windows")]
            println!("Registered in HKCU\\{}", browser.windows_registry_key());
        }
        OutputFormat::Json => {
            let result = InstallManifestResult {
                manifest_path: &manifest_path,
                mediator_path: &mediator_path,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

/// Print a [`CliResult`] using the requested output format.
///
/// # Errors
///
/// Returns an error if formatting or writing to stdout fails.
fn print_result(result: &CliResult, format: OutputFormat) -> Result<(), Error> {
    match format {
        OutputFormat::Human => print_result_human(result),
        OutputFormat::Json => print_result_json(result),
    }
}

/// Convert a [`TabsCommand`] into the corresponding [`CliCommand`].
fn tabs_command_to_cli(cmd: TabsCommand) -> CliCommand {
    match cmd {
        TabsCommand::List { window_id } => CliCommand::ListTabs { window_id },
        TabsCommand::Open {
            window_id,
            before,
            after,
            url,
            strip_credentials,
        } => CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: before,
            insert_after_tab_id: after,
            url,
            strip_credentials,
        },
        TabsCommand::Activate { tab_id } => CliCommand::ActivateTab { tab_id },
        TabsCommand::Navigate { tab_id, url } => CliCommand::NavigateTab { tab_id, url },
        TabsCommand::Close { tab_id } => CliCommand::CloseTab { tab_id },
        TabsCommand::Pin { tab_id } => CliCommand::PinTab { tab_id },
        TabsCommand::Unpin { tab_id } => CliCommand::UnpinTab { tab_id },
        TabsCommand::Warmup { tab_id } => CliCommand::WarmupTab { tab_id },
        TabsCommand::Mute { tab_id } => CliCommand::MuteTab { tab_id },
        TabsCommand::Unmute { tab_id } => CliCommand::UnmuteTab { tab_id },
        TabsCommand::Move { tab_id, new_index } => CliCommand::MoveTab { tab_id, new_index },
        TabsCommand::Back { tab_id, steps } => CliCommand::GoBack { tab_id, steps },
        TabsCommand::Forward { tab_id, steps } => CliCommand::GoForward { tab_id, steps },
    }
}

/// Connect to a mediator and stream browser events as newline-delimited JSON to stdout.
///
/// Sends `SubscribeEvents` and then reads `BrowserEvent` JSON lines from the socket,
/// printing each to stdout. Runs until the connection closes or an error occurs.
///
/// # Errors
///
/// Returns an error if the connection or I/O fails.
#[expect(
    clippy::print_stdout,
    reason = "event stream output goes to stdout by design"
)]
async fn stream_events(socket_path: &std::path::Path) -> Result<(), Error> {
    let request = CliRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        command: CliCommand::SubscribeEvents,
    };

    #[cfg(unix)]
    let stream = tokio::net::UnixStream::connect(socket_path).await?;
    #[cfg(windows)]
    let stream = {
        let pipe_name = pipe_name_from_marker(socket_path)?;
        tokio::net::windows::named_pipe::ClientOptions::new().open(&pipe_name)?
    };

    let (read_half, mut write_half) = tokio::io::split(stream);

    let mut json = serde_json::to_vec(&request)?;
    json.push(b'\n');
    write_half.write_all(&json).await?;
    // Keep write_half alive until the function returns so the mediator does not
    // observe EOF on its read half and terminate the stream prematurely.
    let _write_half = write_half;

    let mut reader = tokio::io::BufReader::new(read_half);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break; // Mediator closed the connection.
        }
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            println!("{trimmed}");
        }
    }
    Ok(())
}

/// Main application logic.
///
/// # Errors
///
/// Returns an error if any step of the application fails.
async fn do_stuff() -> Result<(), Error> {
    let cli = <Cli as clap::Parser>::parse();
    tracing::debug!("{:#?}", cli);

    // Commands that do not require a browser connection.
    match &cli.command {
        Command::GenerateManpage { output_dir } => {
            clap_mangen::generate_to(<Cli as clap::CommandFactory>::command(), output_dir)
                .map_err(Error::GenerateManpage)?;
            return Ok(());
        }
        Command::GenerateShellCompletion { output_file, shell } => {
            let mut f =
                std::fs::File::create(output_file).map_err(Error::GenerateShellCompletion)?;
            let mut c = <Cli as clap::CommandFactory>::command();
            clap_complete::generate(*shell, &mut c, "browser-controller", &mut f);
            return Ok(());
        }
        Command::Instances => {
            let instances = discover_instances().await?;
            match cli.output {
                OutputFormat::Human => print_instances_human(&instances)?,
                OutputFormat::Json => print_instances_json(&instances)?,
            }
            return Ok(());
        }
        Command::InstallManifest {
            browser,
            mediator_path,
            extension_id,
        } => {
            install_manifest(
                *browser,
                mediator_path.clone(),
                extension_id.clone(),
                cli.output,
            )?;
            return Ok(());
        }
        Command::Windows(_) | Command::Tabs(_) | Command::EventStream => {}
    }

    // Commands that require a browser connection.
    let instances = discover_instances().await?;
    let dir = socket_dir()?;
    let instance = select_instance(&instances, cli.instance.as_deref(), &dir)?;
    tracing::debug!(
        browser = %instance.info.browser_name,
        pid = instance.info.pid,
        "Selected browser instance",
    );

    if matches!(cli.command, Command::EventStream) {
        stream_events(&instance.socket_path).await?;
        return Ok(());
    }

    let cli_command = match cli.command {
        Command::Windows(w) => match w.command {
            WindowsCommand::List => CliCommand::ListWindows,
            WindowsCommand::Open => CliCommand::OpenWindow,
            WindowsCommand::Close { window_id } => CliCommand::CloseWindow { window_id },
            WindowsCommand::SetTitlePrefix { window_id, prefix } => {
                CliCommand::SetWindowTitlePrefix { window_id, prefix }
            }
            WindowsCommand::RemoveTitlePrefix { window_id } => {
                CliCommand::RemoveWindowTitlePrefix { window_id }
            }
        },
        Command::Tabs(t) => tabs_command_to_cli(t.command),
        Command::Instances
        | Command::EventStream
        | Command::GenerateManpage { .. }
        | Command::GenerateShellCompletion { .. }
        | Command::InstallManifest { .. } => {
            // Already handled above.
            return Ok(());
        }
    };

    let result = send_command(&instance.socket_path, cli_command).await?;
    print_result(&result, cli.output)?;
    Ok(())
}

/// Entry point.
///
/// Sets up tracing, then delegates to [`do_stuff`].
#[expect(
    clippy::print_stderr,
    reason = "stderr is used for diagnostic messages before and after the logging system is initialized, and for fatal errors"
)]
#[tokio::main]
async fn main() {
    let terminal_filter = match EnvFilter::builder()
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
            .with_filter(terminal_filter),
    );

    let file_layer = match EnvFilter::builder()
        .with_default_directive(LevelFilter::TRACE.into())
        .parse(std::env::var("BROWSER_CONTROLLER_LOG").unwrap_or_else(|_| "trace".to_owned()))
    {
        Err(e) => {
            eprintln!("Failed to parse BROWSER_CONTROLLER_LOG: {e}");
            std::process::exit(1);
        }
        Ok(filter) => std::env::var("BROWSER_CONTROLLER_LOG_DIR")
            .ok()
            .map(|log_dir| {
                let log_file = std::env::var("BROWSER_CONTROLLER_LOG_FILE")
                    .unwrap_or_else(|_| "browser_controller.log".to_owned());
                let appender = tracing_appender::rolling::never(log_dir, log_file);
                tracing_subscriber::fmt::Layer::default()
                    .with_writer(appender)
                    .with_filter(filter)
            }),
    };
    let registry = registry.with(file_layer);

    #[cfg(target_os = "linux")]
    let registry = {
        let journald_filter = match EnvFilter::builder()
            .with_default_directive(LevelFilter::INFO.into())
            .parse(
                std::env::var("BROWSER_CONTROLLER_JOURNALD_LOG")
                    .unwrap_or_else(|_| "info".to_owned()),
            ) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to parse BROWSER_CONTROLLER_JOURNALD_LOG: {e}");
                std::process::exit(1);
            }
        };
        let journald_layer = match tracing_journald::layer() {
            Ok(l) => Some(l.with_filter(journald_filter)),
            Err(e) => {
                eprintln!("Warning: failed to connect to journald: {e}");
                None
            }
        };
        registry.with(journald_layer)
    };

    registry.init();
    log_panics::init();

    match do_stuff().await {
        Ok(()) => {}
        Err(e) => {
            tracing::error!(error = %e, "Command failed");
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
    tracing::debug!("Exiting");
}

#[cfg(test)]
mod test {
    use super::tabs_command_to_cli;
    use browser_controller_types::CliCommand;

    /// Verify that `--before <tab-id>` maps to `insert_before_tab_id = Some(tab_id)`.
    #[test]
    fn tabs_open_before() {
        let cmd = super::TabsCommand::Open {
            window_id: 1,
            before: Some(3),
            after: None,
            url: None,
            strip_credentials: false,
        };
        pretty_assertions::assert_eq!(
            tabs_command_to_cli(cmd),
            CliCommand::OpenTab {
                window_id: 1,
                insert_before_tab_id: Some(3),
                insert_after_tab_id: None,
                url: None,
                strip_credentials: false,
            }
        );
    }

    /// Verify that `--after <tab-id>` maps to `insert_after_tab_id = Some(tab_id)`.
    #[test]
    fn tabs_open_after() {
        let cmd = super::TabsCommand::Open {
            window_id: 1,
            before: None,
            after: Some(2),
            url: None,
            strip_credentials: false,
        };
        pretty_assertions::assert_eq!(
            tabs_command_to_cli(cmd),
            CliCommand::OpenTab {
                window_id: 1,
                insert_before_tab_id: None,
                insert_after_tab_id: Some(2),
                url: None,
                strip_credentials: false,
            }
        );
    }
}
