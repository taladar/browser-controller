//! Browser Controller CLI — control Firefox windows and tabs from the command line.
//!
//! Connects to a running `browser-controller-mediator` instance via Unix Domain Socket
//! and issues commands to control the browser.

use std::time::Duration;

use browser_controller_types::{
    BrowserInfo, CliCommand, CliOutcome, CliRequest, CliResponse, CliResult, TabDetails, TabStatus,
    WindowState, WindowSummary,
};
use regex::Regex;
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

    /// An internal CLI command mapping error occurred (should never happen).
    #[error("invalid CLI command mapping: {0}")]
    InvalidCliCommandMapping(String),

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

    /// No window matched the given criteria.
    #[error("no window matched the criteria: {criteria}")]
    NoMatchingWindow {
        /// Description of the criteria that were used.
        criteria: String,
    },

    /// More than one window matched the criteria and `--if-matches-multiple abort` was set.
    #[error(
        "{count} windows matched the criteria: {criteria}; use --if-matches-multiple all to apply to all"
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

    /// More than one tab matched the criteria and `--if-matches-multiple abort` was set.
    #[error(
        "{count} tabs matched the criteria: {criteria}; use --if-matches-multiple all to apply to all"
    )]
    AmbiguousTab {
        /// Number of tabs that matched.
        count: usize,
        /// Description of the criteria that were used.
        criteria: String,
    },

    /// A regular expression pattern supplied by the user could not be compiled.
    #[error("invalid regex: {0}")]
    InvalidRegex(#[from] regex::Error),

    /// A command timed out waiting for a response.
    #[error(
        "command timed out after {0}s (the extension may have been reloaded or the page may not finish loading)"
    )]
    CommandTimeout(u64),
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
        #[cfg(target_os = "linux")]
        {
            let home = base.home_dir();
            match self {
                Self::Firefox => home.join(".mozilla/native-messaging-hosts"),
                Self::Librewolf => home.join(".librewolf/native-messaging-hosts"),
                Self::Waterfox => home.join(".waterfox/native-messaging-hosts"),
                Self::Chrome => home.join(".config/google-chrome/NativeMessagingHosts"),
                Self::Chromium => home.join(".config/chromium/NativeMessagingHosts"),
                Self::Brave => {
                    home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts")
                }
                Self::Edge => home.join(".config/microsoft-edge/NativeMessagingHosts"),
            }
        }

        #[cfg(target_os = "macos")]
        {
            let home = base.home_dir();
            match self {
                Self::Firefox => {
                    home.join("Library/Application Support/Mozilla/NativeMessagingHosts")
                }
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
            }
        }

        // Windows: JSON manifest file lives under APPDATA or LOCALAPPDATA.
        // A registry key also points to it (written in install_manifest).
        // `base` (home directory) is unused on Windows; bind it to suppress the warning.
        #[cfg(target_os = "windows")]
        {
            let _base = base;
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

/// Controls behavior when a matcher criterion matches more than one window or tab.
///
/// Used with `--if-matches-multiple` on window and tab commands.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MultipleMatchBehavior {
    /// Abort with an error if more than one match is found.
    ///
    /// Zero matches always produce an error regardless of this setting.
    #[default]
    Abort,
    /// Apply the command to every matched window or tab.
    ///
    /// Zero matches still produce an error.
    All,
}

/// CLI representation of a window's visual state, for use with `--window-state`.
///
/// Mirrors [`WindowState`] but derives [`clap::ValueEnum`] to allow direct CLI parsing.
///
/// [`WindowState`]: browser_controller_types::WindowState
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowStateArg {
    /// Window is in its normal state.
    Normal,
    /// Window is minimized.
    Minimized,
    /// Window is maximized.
    Maximized,
    /// Window is in full-screen mode.
    Fullscreen,
}

impl From<WindowStateArg> for WindowState {
    /// Convert a [`WindowStateArg`] CLI value into the protocol [`WindowState`] type.
    fn from(value: WindowStateArg) -> Self {
        match value {
            WindowStateArg::Normal => Self::Normal,
            WindowStateArg::Minimized => Self::Minimized,
            WindowStateArg::Maximized => Self::Maximized,
            WindowStateArg::Fullscreen => Self::Fullscreen,
        }
    }
}

/// CLI representation of a tab's loading status, for use with `--tab-status`.
///
/// Mirrors [`TabStatus`] but derives [`clap::ValueEnum`] to allow direct CLI parsing.
///
/// [`TabStatus`]: browser_controller_types::TabStatus
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStatusArg {
    /// The tab is currently loading.
    Loading,
    /// The tab has finished loading.
    Complete,
}

impl From<TabStatusArg> for TabStatus {
    /// Convert a [`TabStatusArg`] CLI value into the protocol [`TabStatus`] type.
    fn from(value: TabStatusArg) -> Self {
        match value {
            TabStatusArg::Loading => Self::Loading,
            TabStatusArg::Complete => Self::Complete,
        }
    }
}

/// Criteria for selecting one or more browser windows.
///
/// All provided criteria are combined with AND logic. If no criteria are specified,
/// every window will match, which will produce an error unless
/// `--if-matches-multiple all` is also passed.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each bool is an independent opt-in filter flag; there is no simpler representation"
)]
#[derive(clap::Args, Debug, Default)]
pub struct WindowMatcher {
    /// Match a window by its exact browser-assigned numeric ID.
    #[clap(long)]
    pub window_id: Option<u32>,
    /// Match windows whose full title equals this string exactly.
    #[clap(long)]
    pub window_title: Option<String>,
    /// Match windows whose title prefix (Firefox `titlePreface`) equals this string exactly.
    #[clap(long)]
    pub window_title_prefix: Option<String>,
    /// Match windows whose full title matches this regular expression.
    #[clap(long)]
    pub window_title_regex: Option<String>,
    /// Match only windows that currently have input focus.
    #[clap(long)]
    pub window_focused: bool,
    /// Match only windows that do not currently have input focus.
    #[clap(long, conflicts_with = "window_focused")]
    pub window_not_focused: bool,
    /// Match only the most recently focused window.
    #[clap(long)]
    pub window_last_focused: bool,
    /// Match only windows that are not the most recently focused.
    #[clap(long, conflicts_with = "window_last_focused")]
    pub window_not_last_focused: bool,
    /// Match only windows in this visual state.
    #[clap(long)]
    pub window_state: Option<WindowStateArg>,
    /// How to handle a criterion that matches multiple windows.
    ///
    /// `abort` (the default) treats more than one match as an error.
    /// `all` applies the command to every matched window.
    #[clap(long, default_value = "abort")]
    pub if_matches_multiple: MultipleMatchBehavior,
}

impl std::fmt::Display for WindowMatcher {
    /// Format the active window criteria as a human-readable string for error messages.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(id) = self.window_id {
            parts.push(format!("window-id={id}"));
        }
        if let Some(ref title) = self.window_title {
            parts.push(format!("window-title={title:?}"));
        }
        if let Some(ref prefix) = self.window_title_prefix {
            parts.push(format!("window-title-prefix={prefix:?}"));
        }
        if let Some(ref regex) = self.window_title_regex {
            parts.push(format!("window-title-regex={regex:?}"));
        }
        if self.window_focused {
            parts.push("window-focused".to_owned());
        }
        if self.window_not_focused {
            parts.push("window-not-focused".to_owned());
        }
        if self.window_last_focused {
            parts.push("window-last-focused".to_owned());
        }
        if self.window_not_last_focused {
            parts.push("window-not-last-focused".to_owned());
        }
        if let Some(state) = self.window_state {
            parts.push(format!("window-state={state:?}"));
        }
        if parts.is_empty() {
            write!(f, "(all windows)")
        } else {
            write!(f, "{}", parts.join(", "))
        }
    }
}

/// Criteria for selecting one or more browser tabs.
///
/// All provided criteria are combined with AND logic. If no criteria are specified,
/// every tab in every searched window will match, which will produce an error unless
/// `--if-matches-multiple all` is also passed.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each bool is an independent opt-in filter flag mirroring the boolean fields of TabDetails; there is no simpler representation"
)]
#[derive(clap::Args, Debug, Default)]
pub struct TabMatcher {
    /// Match a tab by its exact browser-assigned numeric ID.
    #[clap(long)]
    pub tab_id: Option<u32>,
    /// Match tabs whose title equals this string exactly.
    #[clap(long)]
    pub tab_title: Option<String>,
    /// Match tabs whose title matches this regular expression.
    #[clap(long)]
    pub tab_title_regex: Option<String>,
    /// Match tabs whose URL equals this string exactly.
    #[clap(long)]
    pub tab_url: Option<String>,
    /// Match tabs whose URL's registered domain equals this string (e.g. `example.com`).
    #[clap(long)]
    pub tab_url_domain: Option<String>,
    /// Match tabs whose URL matches this regular expression.
    #[clap(long)]
    pub tab_url_regex: Option<String>,
    /// Restrict the search to tabs belonging to the window with this ID.
    #[clap(long)]
    pub tab_window_id: Option<u32>,
    /// Match only the currently active tab in each window.
    #[clap(long)]
    pub tab_active: bool,
    /// Match only tabs that are not the active tab in their window.
    #[clap(long, conflicts_with = "tab_active")]
    pub tab_not_active: bool,
    /// Match only pinned tabs.
    #[clap(long)]
    pub tab_pinned: bool,
    /// Match only unpinned tabs.
    #[clap(long, conflicts_with = "tab_pinned")]
    pub tab_not_pinned: bool,
    /// Match only discarded (unloaded from memory) tabs.
    #[clap(long)]
    pub tab_discarded: bool,
    /// Match only non-discarded tabs.
    #[clap(long, conflicts_with = "tab_discarded")]
    pub tab_not_discarded: bool,
    /// Match only tabs that are currently producing audio.
    #[clap(long)]
    pub tab_audible: bool,
    /// Match only tabs that are not currently producing audio.
    #[clap(long, conflicts_with = "tab_audible")]
    pub tab_not_audible: bool,
    /// Match only tabs whose audio is muted.
    #[clap(long)]
    pub tab_muted: bool,
    /// Match only tabs whose audio is not muted.
    #[clap(long, conflicts_with = "tab_muted")]
    pub tab_not_muted: bool,
    /// Match only tabs open in a private/incognito window.
    #[clap(long)]
    pub tab_incognito: bool,
    /// Match only tabs not open in a private/incognito window.
    #[clap(long, conflicts_with = "tab_incognito")]
    pub tab_not_incognito: bool,
    /// Match only tabs that are currently awaiting HTTP basic authentication.
    #[clap(long)]
    pub tab_awaiting_auth: bool,
    /// Match only tabs that are not currently awaiting HTTP basic authentication.
    #[clap(long, conflicts_with = "tab_awaiting_auth")]
    pub tab_not_awaiting_auth: bool,
    /// Match only tabs currently displayed in Reader Mode.
    #[clap(long)]
    pub tab_in_reader_mode: bool,
    /// Match only tabs not currently displayed in Reader Mode.
    #[clap(long, conflicts_with = "tab_in_reader_mode")]
    pub tab_not_in_reader_mode: bool,
    /// Match only tabs with this loading status.
    #[clap(long)]
    pub tab_status: Option<TabStatusArg>,
    /// How to handle a criterion that matches multiple tabs.
    ///
    /// `abort` (the default) treats more than one match as an error.
    /// `all` applies the command to every matched tab.
    #[clap(long, default_value = "abort")]
    pub if_matches_multiple: MultipleMatchBehavior,
}

impl std::fmt::Display for TabMatcher {
    /// Format the active tab criteria as a human-readable string for error messages.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(id) = self.tab_id {
            parts.push(format!("tab-id={id}"));
        }
        if let Some(ref title) = self.tab_title {
            parts.push(format!("tab-title={title:?}"));
        }
        if let Some(ref regex) = self.tab_title_regex {
            parts.push(format!("tab-title-regex={regex:?}"));
        }
        if let Some(ref url) = self.tab_url {
            parts.push(format!("tab-url={url:?}"));
        }
        if let Some(ref domain) = self.tab_url_domain {
            parts.push(format!("tab-url-domain={domain:?}"));
        }
        if let Some(ref regex) = self.tab_url_regex {
            parts.push(format!("tab-url-regex={regex:?}"));
        }
        if let Some(win_id) = self.tab_window_id {
            parts.push(format!("tab-window-id={win_id}"));
        }
        if self.tab_active {
            parts.push("tab-active".to_owned());
        }
        if self.tab_not_active {
            parts.push("tab-not-active".to_owned());
        }
        if self.tab_pinned {
            parts.push("tab-pinned".to_owned());
        }
        if self.tab_not_pinned {
            parts.push("tab-not-pinned".to_owned());
        }
        if self.tab_discarded {
            parts.push("tab-discarded".to_owned());
        }
        if self.tab_not_discarded {
            parts.push("tab-not-discarded".to_owned());
        }
        if self.tab_audible {
            parts.push("tab-audible".to_owned());
        }
        if self.tab_not_audible {
            parts.push("tab-not-audible".to_owned());
        }
        if self.tab_muted {
            parts.push("tab-muted".to_owned());
        }
        if self.tab_not_muted {
            parts.push("tab-not-muted".to_owned());
        }
        if self.tab_incognito {
            parts.push("tab-incognito".to_owned());
        }
        if self.tab_not_incognito {
            parts.push("tab-not-incognito".to_owned());
        }
        if self.tab_awaiting_auth {
            parts.push("tab-awaiting-auth".to_owned());
        }
        if self.tab_not_awaiting_auth {
            parts.push("tab-not-awaiting-auth".to_owned());
        }
        if self.tab_in_reader_mode {
            parts.push("tab-in-reader-mode".to_owned());
        }
        if self.tab_not_in_reader_mode {
            parts.push("tab-not-in-reader-mode".to_owned());
        }
        if let Some(status) = self.tab_status {
            parts.push(format!("tab-status={status:?}"));
        }
        if parts.is_empty() {
            write!(f, "(all tabs)")
        } else {
            write!(f, "{}", parts.join(", "))
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

    /// Timeout in seconds for a command to complete.
    ///
    /// If the mediator or extension does not respond within this time (e.g. due
    /// to an extension reload, crash, or a page that never finishes loading),
    /// the command fails with an error instead of hanging indefinitely.
    /// Set to 0 to disable the timeout.
    #[clap(long, short = 't', default_value = "30", global = true)]
    timeout: u64,
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
    Open {
        /// Title prefix (Firefox `titlePreface`) to set on the new window immediately after opening.
        #[clap(long)]
        title_prefix: Option<String>,
        /// Only open the window if no existing window already has the title prefix given by `--title-prefix`.
        ///
        /// If a window with that title prefix already exists the command succeeds silently
        /// without opening a new window. Requires `--title-prefix`.
        #[clap(long, requires = "title_prefix")]
        if_title_prefix_does_not_exist: bool,
    },
    /// Close one or more browser windows.
    Close {
        /// Criteria selecting the window(s) to close.
        #[clap(flatten)]
        window: WindowMatcher,
    },
    /// Set the title prefix (Firefox `titlePreface`) for one or more windows.
    SetTitlePrefix {
        /// Criteria selecting the window(s) to modify.
        #[clap(flatten)]
        window: WindowMatcher,
        /// Prefix to prepend to the window title.
        prefix: String,
    },
    /// Remove the title prefix from one or more windows.
    RemoveTitlePrefix {
        /// Criteria selecting the window(s) to modify.
        #[clap(flatten)]
        window: WindowMatcher,
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
    /// List all tabs in one or more windows with full details.
    List {
        /// Criteria selecting the window(s) whose tabs to list.
        #[clap(flatten)]
        window: WindowMatcher,
    },
    /// Open a new tab in a window.
    Open {
        /// Criteria selecting the window in which to open the tab.
        #[clap(flatten)]
        window: WindowMatcher,
        /// Insert the new tab immediately before the tab with this ID.
        #[clap(long, conflicts_with = "after")]
        before: Option<u32>,
        /// Insert the new tab immediately after the tab with this ID.
        #[clap(long, conflicts_with = "before")]
        after: Option<u32>,
        /// URL to load in the new tab (defaults to the browser's new-tab page).
        #[clap(long)]
        url: Option<String>,
        /// Username for HTTP authentication.
        ///
        /// When provided together with `--password-env`, the extension strips any embedded
        /// credentials from the URL and provides the given username/password to the server's
        /// 401 challenge via the browser's `onAuthRequired` API. The browser caches the
        /// credentials for the realm, so subsequent requests work automatically.
        /// Requires `--url`.
        #[clap(long, requires = "url")]
        username: Option<String>,
        /// Name of an environment variable containing the password for HTTP authentication.
        ///
        /// The password is read from this environment variable instead of being passed
        /// directly on the command line, to avoid exposing it in process listings.
        /// Requires `--url` and `--username`.
        #[clap(long, requires_all = ["url", "username"])]
        password_env: Option<String>,
        /// Open the new tab in the background, keeping the currently active tab focused.
        #[clap(long)]
        background: bool,
        /// Only open the tab if no existing tab in any window has a URL matching `--url`.
        ///
        /// The comparison strips `user:password@` credentials from both sides before comparing,
        /// so a tab previously opened with `--username` is still considered a match.
        /// If a matching tab already exists the command succeeds silently without opening a new tab.
        /// Requires `--url`.
        #[clap(long, requires = "url")]
        if_url_does_not_exist: bool,
    },
    /// Activate a tab, making it the focused tab in its window.
    Activate {
        /// Criteria selecting the tab(s) to activate.
        #[clap(flatten)]
        tab: TabMatcher,
    },
    /// Navigate an existing tab to a new URL.
    Navigate {
        /// Criteria selecting the tab(s) to navigate.
        #[clap(flatten)]
        tab: TabMatcher,
        /// URL to load in the tab.
        #[clap(long)]
        url: String,
    },
    /// Close one or more tabs.
    Close {
        /// Criteria selecting the tab(s) to close.
        #[clap(flatten)]
        tab: TabMatcher,
    },
    /// Pin one or more tabs.
    Pin {
        /// Criteria selecting the tab(s) to pin.
        #[clap(flatten)]
        tab: TabMatcher,
    },
    /// Unpin one or more tabs.
    Unpin {
        /// Criteria selecting the tab(s) to unpin.
        #[clap(flatten)]
        tab: TabMatcher,
    },
    /// Warm up one or more discarded tabs, loading their content into memory without activating.
    Warmup {
        /// Criteria selecting the tab(s) to warm up.
        #[clap(flatten)]
        tab: TabMatcher,
    },
    /// Mute one or more tabs, suppressing any audio they produce.
    Mute {
        /// Criteria selecting the tab(s) to mute.
        #[clap(flatten)]
        tab: TabMatcher,
    },
    /// Unmute one or more tabs, allowing them to produce audio again.
    Unmute {
        /// Criteria selecting the tab(s) to unmute.
        #[clap(flatten)]
        tab: TabMatcher,
    },
    /// Move a tab to a new position within its window.
    Move {
        /// Criteria selecting the tab(s) to move.
        #[clap(flatten)]
        tab: TabMatcher,
        /// New zero-based index for the tab within its window.
        #[clap(long)]
        new_index: u32,
    },
    /// Navigate backward in a tab's session history.
    Back {
        /// Criteria selecting the tab(s) to navigate backward.
        #[clap(flatten)]
        tab: TabMatcher,
        /// Number of steps to go back.
        ///
        /// Values greater than 1 skip intermediate pages atomically, which is useful
        /// when those pages redirect immediately forward again.
        #[clap(long, default_value_t = 1u32)]
        steps: u32,
    },
    /// Navigate forward in a tab's session history.
    Forward {
        /// Criteria selecting the tab(s) to navigate forward.
        #[clap(flatten)]
        tab: TabMatcher,
        /// Number of steps to go forward.
        ///
        /// Values greater than 1 skip intermediate pages atomically, which is useful
        /// when those pages redirect immediately backward again.
        #[clap(long, default_value_t = 1u32)]
        steps: u32,
    },
    /// Sort tabs in one or more windows according to specified domain order.
    Sort {
        /// Criteria selecting the window(s) whose tabs to sort.
        #[clap(flatten)]
        window: WindowMatcher,
        /// List of domains in the desired sort order. Tabs with domains not in this list
        /// will be placed after all listed domains, maintaining their original relative order.
        /// Tabs with the same domain will also maintain their original relative order (stable sort).
        #[clap(
            long,
            value_delimiter = ',',
            help = "Comma-separated list of domains to sort by"
        )]
        domains: Vec<String>,
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
                let focused = if win.is_focused {
                    ", focused"
                } else if win.is_last_focused {
                    ", last-focused"
                } else {
                    ""
                };
                println!(
                    "Window {} — {:?}{}{} [{}]",
                    win.id, win.title, prefix_display, focused, win.state,
                );
                for tab in &win.tabs {
                    let active = if tab.is_active { "*" } else { " " };
                    println!(
                        "  {active}{:<4} [{}] {} — {}",
                        tab.index, tab.id, tab.title, tab.url
                    );
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

/// Apply [`WindowMatcher`] criteria to a list of windows and return the matching IDs.
///
/// All criteria are combined with AND logic. An empty matcher matches every window.
///
/// # Errors
///
/// Returns [`Error::InvalidRegex`] if `--window-title-regex` cannot be compiled.
fn match_windows(windows: &[WindowSummary], m: &WindowMatcher) -> Result<Vec<u32>, Error> {
    let title_regex = m
        .window_title_regex
        .as_deref()
        .map(Regex::new)
        .transpose()?;

    let matched = windows
        .iter()
        .filter(|win| {
            if let Some(id) = m.window_id
                && win.id != id
            {
                return false;
            }
            if let Some(ref title) = m.window_title
                && win.title != *title
            {
                return false;
            }
            if let Some(ref prefix) = m.window_title_prefix
                && win.title_prefix.as_deref() != Some(prefix.as_str())
            {
                return false;
            }
            if let Some(ref re) = title_regex
                && !re.is_match(&win.title)
            {
                return false;
            }
            if m.window_focused && !win.is_focused {
                return false;
            }
            if m.window_not_focused && win.is_focused {
                return false;
            }
            if m.window_last_focused && !win.is_last_focused {
                return false;
            }
            if m.window_not_last_focused && win.is_last_focused {
                return false;
            }
            if let Some(state) = m.window_state
                && win.state != state.into()
            {
                return false;
            }
            true
        })
        .map(|win| win.id)
        .collect();
    Ok(matched)
}

/// Apply [`TabMatcher`] criteria to a list of tabs and return the matching IDs.
///
/// All criteria are combined with AND logic. An empty matcher matches every tab.
///
/// # Errors
///
/// Returns [`Error::InvalidRegex`] if a regex pattern cannot be compiled.
fn match_tabs(tabs: &[TabDetails], m: &TabMatcher) -> Result<Vec<u32>, Error> {
    let title_regex = m.tab_title_regex.as_deref().map(Regex::new).transpose()?;
    let url_regex = m.tab_url_regex.as_deref().map(Regex::new).transpose()?;

    let matched = tabs
        .iter()
        .filter(|tab| {
            if let Some(id) = m.tab_id
                && tab.id != id
            {
                return false;
            }
            if let Some(ref title) = m.tab_title
                && tab.title != *title
            {
                return false;
            }
            if let Some(ref re) = title_regex
                && !re.is_match(&tab.title)
            {
                return false;
            }
            if let Some(ref url) = m.tab_url
                && tab.url != *url
            {
                return false;
            }
            if let Some(ref domain) = m.tab_url_domain {
                let tab_domain = url::Url::parse(&tab.url)
                    .ok()
                    .and_then(|u| u.domain().map(|s| s.to_owned()))
                    .unwrap_or_default();
                if tab_domain != *domain {
                    return false;
                }
            }
            if let Some(ref re) = url_regex
                && !re.is_match(&tab.url)
            {
                return false;
            }
            if let Some(win_id) = m.tab_window_id
                && tab.window_id != win_id
            {
                return false;
            }
            if m.tab_active && !tab.is_active {
                return false;
            }
            if m.tab_not_active && tab.is_active {
                return false;
            }
            if m.tab_pinned && !tab.is_pinned {
                return false;
            }
            if m.tab_not_pinned && tab.is_pinned {
                return false;
            }
            if m.tab_discarded && !tab.is_discarded {
                return false;
            }
            if m.tab_not_discarded && tab.is_discarded {
                return false;
            }
            if m.tab_audible && !tab.is_audible {
                return false;
            }
            if m.tab_not_audible && tab.is_audible {
                return false;
            }
            if m.tab_muted && !tab.is_muted {
                return false;
            }
            if m.tab_not_muted && tab.is_muted {
                return false;
            }
            if m.tab_incognito && !tab.incognito {
                return false;
            }
            if m.tab_not_incognito && tab.incognito {
                return false;
            }
            if m.tab_awaiting_auth && !tab.is_awaiting_auth {
                return false;
            }
            if m.tab_not_awaiting_auth && tab.is_awaiting_auth {
                return false;
            }
            if m.tab_in_reader_mode && !tab.is_in_reader_mode {
                return false;
            }
            if m.tab_not_in_reader_mode && tab.is_in_reader_mode {
                return false;
            }
            if let Some(status) = m.tab_status
                && tab.status != status.into()
            {
                return false;
            }
            true
        })
        .map(|tab| tab.id)
        .collect();
    Ok(matched)
}

/// Resolve a [`WindowMatcher`] to a list of matching window IDs.
///
/// Sends `ListWindows` to the mediator, applies the matcher, and enforces
/// [`MultipleMatchBehavior`].
///
/// # Errors
///
/// Returns an error if the command fails, the regex is invalid, no window matches, or
/// multiple windows match and `--if-matches-multiple abort` is set.
async fn resolve_windows(
    socket_path: &std::path::Path,
    matcher: &WindowMatcher,
) -> Result<Vec<u32>, Error> {
    let result = send_command(socket_path, CliCommand::ListWindows).await?;
    let CliResult::Windows { windows } = result else {
        return Err(Error::CommandFailed(format!(
            "unexpected response to ListWindows: {result:?}"
        )));
    };
    let matched = match_windows(&windows, matcher)?;
    let criteria = matcher.to_string();
    match matched.len() {
        0 => Err(Error::NoMatchingWindow { criteria }),
        1 => Ok(matched),
        count => match matcher.if_matches_multiple {
            MultipleMatchBehavior::Abort => Err(Error::AmbiguousWindow { count, criteria }),
            MultipleMatchBehavior::All => Ok(matched),
        },
    }
}

/// Resolve a [`TabMatcher`] to a list of matching tab IDs.
///
/// If `--tab-window-id` is set, only that window is searched; otherwise `ListWindows`
/// is called first to enumerate all windows, then `ListTabs` is called for each.
/// Enforces [`MultipleMatchBehavior`].
///
/// # Errors
///
/// Returns an error if any command fails, a regex is invalid, no tab matches, or
/// multiple tabs match and `--if-matches-multiple abort` is set.
async fn resolve_tabs(
    socket_path: &std::path::Path,
    matcher: &TabMatcher,
) -> Result<Vec<u32>, Error> {
    let window_ids_to_search: Vec<u32> = if let Some(win_id) = matcher.tab_window_id {
        vec![win_id]
    } else {
        let list_result = send_command(socket_path, CliCommand::ListWindows).await?;
        let CliResult::Windows { windows } = list_result else {
            return Err(Error::CommandFailed(format!(
                "unexpected response to ListWindows: {list_result:?}"
            )));
        };
        windows.iter().map(|w| w.id).collect()
    };

    let mut all_tabs: Vec<TabDetails> = Vec::new();
    for win_id in window_ids_to_search {
        let tabs_result =
            send_command(socket_path, CliCommand::ListTabs { window_id: win_id }).await?;
        let CliResult::Tabs { tabs } = tabs_result else {
            return Err(Error::CommandFailed(format!(
                "unexpected response to ListTabs: {tabs_result:?}"
            )));
        };
        all_tabs.extend(tabs);
    }

    let matched = match_tabs(&all_tabs, matcher)?;
    let criteria = matcher.to_string();
    match matched.len() {
        0 => Err(Error::NoMatchingTab { criteria }),
        1 => Ok(matched),
        count => match matcher.if_matches_multiple {
            MultipleMatchBehavior::Abort => Err(Error::AmbiguousTab { count, criteria }),
            MultipleMatchBehavior::All => Ok(matched),
        },
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

/// Return `url_str` with any embedded `user:password@` credentials removed.
///
/// Used to normalize URLs before comparing them, so that a tab opened with
/// `--strip-credentials` still matches a check against the credential-free URL.
/// Falls back to the original string unchanged if parsing fails (e.g. non-HTTP URLs).
#[must_use]
fn strip_url_credentials(url_str: &str) -> String {
    if let Ok(mut u) = url::Url::parse(url_str) {
        // set_username/set_password only fail on cannot-be-a-base URLs (e.g. data:),
        // which cannot carry credentials; the Err(()) is safe to ignore.
        match u.set_username("") {
            Ok(()) | Err(()) => {}
        }
        match u.set_password(None) {
            Ok(()) | Err(()) => {}
        }
        u.to_string()
    } else {
        url_str.to_owned()
    }
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

    let timeout_secs = cli.timeout;
    let command_future = execute_command(cli, instance);
    if timeout_secs == 0 {
        command_future.await
    } else {
        tokio::time::timeout(Duration::from_secs(timeout_secs), command_future)
            .await
            .map_err(|_elapsed| Error::CommandTimeout(timeout_secs))?
    }
}

/// Execute the selected command against the given browser instance.
///
/// # Errors
///
/// Returns an error if the command fails.
async fn execute_command(cli: Cli, instance: &DiscoveredInstance) -> Result<(), Error> {
    match cli.command {
        Command::Windows(w) => match w.command {
            WindowsCommand::List => {
                let result = send_command(&instance.socket_path, CliCommand::ListWindows).await?;
                print_result(&result, cli.output)?;
            }
            WindowsCommand::Open {
                title_prefix,
                if_title_prefix_does_not_exist,
            } => {
                // Guard: skip opening if a window with the required prefix already exists.
                if if_title_prefix_does_not_exist && let Some(ref required_prefix) = title_prefix {
                    let list_result =
                        send_command(&instance.socket_path, CliCommand::ListWindows).await?;
                    let CliResult::Windows { windows } = list_result else {
                        return Err(Error::CommandFailed(format!(
                            "unexpected response to ListWindows: {list_result:?}"
                        )));
                    };
                    if windows
                        .iter()
                        .any(|w| w.title_prefix.as_deref() == Some(required_prefix.as_str()))
                    {
                        return Ok(());
                    }
                }
                let result = send_command(
                    &instance.socket_path,
                    CliCommand::OpenWindow { title_prefix },
                )
                .await?;
                print_result(&result, cli.output)?;
            }
            WindowsCommand::Close { window } => {
                let window_ids = resolve_windows(&instance.socket_path, &window).await?;
                for window_id in window_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::CloseWindow { window_id })
                            .await?;
                    print_result(&result, cli.output)?;
                }
            }
            WindowsCommand::SetTitlePrefix { window, prefix } => {
                let window_ids = resolve_windows(&instance.socket_path, &window).await?;
                for window_id in window_ids {
                    let result = send_command(
                        &instance.socket_path,
                        CliCommand::SetWindowTitlePrefix {
                            window_id,
                            prefix: prefix.clone(),
                        },
                    )
                    .await?;
                    print_result(&result, cli.output)?;
                }
            }
            WindowsCommand::RemoveTitlePrefix { window } => {
                let window_ids = resolve_windows(&instance.socket_path, &window).await?;
                for window_id in window_ids {
                    let result = send_command(
                        &instance.socket_path,
                        CliCommand::RemoveWindowTitlePrefix { window_id },
                    )
                    .await?;
                    print_result(&result, cli.output)?;
                }
            }
        },
        Command::Tabs(t) => match t.command {
            TabsCommand::List { window } => {
                let window_ids = resolve_windows(&instance.socket_path, &window).await?;
                for window_id in window_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::ListTabs { window_id })
                            .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Open {
                window,
                before,
                after,
                url,
                username,
                password_env,
                background,
                if_url_does_not_exist,
            } => {
                // Guard: skip opening if a tab with the given URL already exists anywhere.
                if if_url_does_not_exist && let Some(ref check_url) = url {
                    let list_result =
                        send_command(&instance.socket_path, CliCommand::ListWindows).await?;
                    let CliResult::Windows { windows } = list_result else {
                        return Err(Error::CommandFailed(format!(
                            "unexpected response to ListWindows: {list_result:?}"
                        )));
                    };
                    let normalized = strip_url_credentials(check_url);
                    let already_exists = windows
                        .iter()
                        .flat_map(|w| &w.tabs)
                        .any(|t| strip_url_credentials(&t.url) == normalized);
                    if already_exists {
                        return Ok(());
                    }
                }
                // Resolve the password from the environment variable if specified.
                let password = password_env
                    .map(|env_name| {
                        std::env::var(&env_name).map_err(|_not_set| {
                            Error::CommandFailed(format!(
                                "environment variable {env_name} is not set"
                            ))
                        })
                    })
                    .transpose()?;

                let window_ids = resolve_windows(&instance.socket_path, &window).await?;
                for window_id in window_ids {
                    let result = send_command(
                        &instance.socket_path,
                        CliCommand::OpenTab {
                            window_id,
                            insert_before_tab_id: before,
                            insert_after_tab_id: after,
                            url: url.clone(),
                            username: username.clone(),
                            password: password.clone(),
                            background,
                        },
                    )
                    .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Activate { tab } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::ActivateTab { tab_id })
                            .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Navigate { tab, url } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result = send_command(
                        &instance.socket_path,
                        CliCommand::NavigateTab {
                            tab_id,
                            url: url.clone(),
                        },
                    )
                    .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Close { tab } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::CloseTab { tab_id })
                            .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Pin { tab } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::PinTab { tab_id }).await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Unpin { tab } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::UnpinTab { tab_id })
                            .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Warmup { tab } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::WarmupTab { tab_id })
                            .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Mute { tab } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::MuteTab { tab_id }).await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Unmute { tab } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::UnmuteTab { tab_id })
                            .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Move { tab, new_index } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result = send_command(
                        &instance.socket_path,
                        CliCommand::MoveTab { tab_id, new_index },
                    )
                    .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Back { tab, steps } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result =
                        send_command(&instance.socket_path, CliCommand::GoBack { tab_id, steps })
                            .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Forward { tab, steps } => {
                let tab_ids = resolve_tabs(&instance.socket_path, &tab).await?;
                for tab_id in tab_ids {
                    let result = send_command(
                        &instance.socket_path,
                        CliCommand::GoForward { tab_id, steps },
                    )
                    .await?;
                    print_result(&result, cli.output)?;
                }
            }
            TabsCommand::Sort { window, domains } => {
                let window_ids = resolve_windows(&instance.socket_path, &window).await?;
                for window_id in window_ids {
                    let list_tabs_result =
                        send_command(&instance.socket_path, CliCommand::ListTabs { window_id })
                            .await?;

                    let CliResult::Tabs { mut tabs } = list_tabs_result else {
                        return Err(Error::CommandFailed(format!(
                            "unexpected response to ListTabs: {list_tabs_result:?}"
                        )));
                    };

                    // Store original indices to maintain stable sort for unlisted domains
                    // and same domains.
                    let original_tab_order: Vec<_> = tabs.iter().map(|t| t.id).collect();

                    // Create a domain priority map.
                    let domain_priority: std::collections::HashMap<String, usize> = domains
                        .iter()
                        .enumerate()
                        .map(|(i, d)| (d.clone(), i))
                        .collect();

                    // Sort tabs.
                    tabs.sort_by(|a, b| {
                        let domain_a = url::Url::parse(&a.url)
                            .ok()
                            .and_then(|u| u.domain().map(|s| s.to_owned()))
                            .unwrap_or_default();
                        let domain_b = url::Url::parse(&b.url)
                            .ok()
                            .and_then(|u| u.domain().map(|s| s.to_owned()))
                            .unwrap_or_default();

                        let priority_a = domain_priority.get(&domain_a).copied();
                        let priority_b = domain_priority.get(&domain_b).copied();

                        match (priority_a, priority_b) {
                            (Some(pa), Some(pb)) => pa.cmp(&pb),
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => {
                                let original_index_a = original_tab_order
                                    .iter()
                                    .position(|&id| id == a.id)
                                    .unwrap_or_default();
                                let original_index_b = original_tab_order
                                    .iter()
                                    .position(|&id| id == b.id)
                                    .unwrap_or_default();
                                original_index_a.cmp(&original_index_b)
                            }
                        }
                    });

                    // Send move commands for tabs that are out of place.
                    for (new_index, tab) in tabs.into_iter().enumerate() {
                        #[expect(
                            clippy::as_conversions,
                            reason = "Tab index values are small enough that overflows are never an issue"
                        )]
                        if (tab.index as usize) != new_index {
                            #[expect(
                                clippy::cast_possible_truncation,
                                reason = "Tab index values (and for that matter values coming out of enumerate) are small enough that overflows are never an issue"
                            )]
                            send_command(
                                &instance.socket_path,
                                CliCommand::MoveTab {
                                    tab_id: tab.id,
                                    new_index: new_index as u32,
                                },
                            )
                            .await?;
                        }
                    }
                }
            }
        },
        // Already handled above.
        Command::Instances
        | Command::EventStream
        | Command::GenerateManpage { .. }
        | Command::GenerateShellCompletion { .. }
        | Command::InstallManifest { .. } => {}
    }
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
    use browser_controller_types::{TabDetails, TabStatus, WindowState, WindowSummary};

    use super::{TabMatcher, WindowMatcher, match_tabs, match_windows};

    /// Build a minimal [`WindowSummary`] for use in tests.
    fn make_window(id: u32, title: &str) -> WindowSummary {
        WindowSummary {
            id,
            title: title.to_owned(),
            title_prefix: None,
            is_focused: false,
            is_last_focused: false,
            state: WindowState::Normal,
            tabs: vec![],
        }
    }

    /// Build a minimal [`TabDetails`] for use in tests.
    fn make_tab(id: u32, window_id: u32, title: &str, url: &str) -> TabDetails {
        TabDetails {
            id,
            index: 0,
            window_id,
            title: title.to_owned(),
            url: url.to_owned(),
            is_active: false,
            is_pinned: false,
            is_discarded: false,
            is_audible: false,
            is_muted: false,
            status: TabStatus::Complete,
            has_attention: false,
            is_awaiting_auth: false,
            is_in_reader_mode: false,
            incognito: false,
            history_length: 0,
            history_steps_back: None,
            history_steps_forward: None,
            history_hidden_count: None,
        }
    }

    /// Verify that `--window-id` selects exactly the window with that ID.
    #[test]
    fn match_windows_by_id() -> Result<(), crate::Error> {
        let windows = vec![make_window(1, "Window One"), make_window(2, "Window Two")];
        let m = WindowMatcher {
            window_id: Some(1),
            ..Default::default()
        };
        let ids = match_windows(&windows, &m)?;
        pretty_assertions::assert_eq!(ids, vec![1u32]);
        Ok(())
    }

    /// Verify that `--window-title` selects exactly the window with that exact title.
    #[test]
    fn match_windows_by_title() -> Result<(), crate::Error> {
        let windows = vec![make_window(1, "Work"), make_window(2, "Personal")];
        let m = WindowMatcher {
            window_title: Some("Work".to_owned()),
            ..Default::default()
        };
        let ids = match_windows(&windows, &m)?;
        pretty_assertions::assert_eq!(ids, vec![1u32]);
        Ok(())
    }

    /// Verify that `--tab-id` selects exactly the tab with that ID.
    #[test]
    fn match_tabs_by_id() -> Result<(), crate::Error> {
        let tabs = vec![
            make_tab(10, 1, "Tab A", "https://example.com"),
            make_tab(11, 1, "Tab B", "https://other.com"),
        ];
        let m = TabMatcher {
            tab_id: Some(10),
            ..Default::default()
        };
        let ids = match_tabs(&tabs, &m)?;
        pretty_assertions::assert_eq!(ids, vec![10u32]);
        Ok(())
    }

    /// Verify that `--tab-title` selects exactly the tab with that exact title.
    #[test]
    fn match_tabs_by_title() -> Result<(), crate::Error> {
        let tabs = vec![
            make_tab(10, 1, "Dashboard", "https://example.com"),
            make_tab(11, 1, "Settings", "https://example.com/settings"),
        ];
        let m = TabMatcher {
            tab_title: Some("Dashboard".to_owned()),
            ..Default::default()
        };
        let ids = match_tabs(&tabs, &m)?;
        pretty_assertions::assert_eq!(ids, vec![10u32]);
        Ok(())
    }
}
