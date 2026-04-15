//! Browser Controller CLI — control Firefox windows and tabs from the command line.
//!
//! Connects to a running `browser-controller-mediator` instance via Unix Domain Socket
//! and issues commands to control the browser.

use std::time::Duration;

use browser_controller_client::{
    BooleanCondition, CliResult, Client, CookieStoreId, DiscoveredInstance, DownloadId,
    OpenTabParamsBuilder, TabId, TabStatus, WindowId, WindowState,
};
use tracing_subscriber::{
    EnvFilter, Layer as _, Registry, filter::LevelFilter, layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

/// Errors that can occur in the CLI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An error from the client library.
    #[error("client error: {0}")]
    Client(#[from] browser_controller_client::Error),

    /// An I/O error occurred (covers both network and filesystem operations).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Writing a Windows registry key for native messaging host registration failed.
    #[cfg(target_os = "windows")]
    #[error("failed to write Windows registry key: {0}")]
    RegistryWriteFailed(#[source] std::io::Error),

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

    /// A window matcher could not be built.
    #[error("invalid window matcher: {0}")]
    WindowMatcherBuild(#[from] browser_controller_client::WindowMatcherBuilderError),

    /// A tab matcher could not be built.
    #[error("invalid tab matcher: {0}")]
    TabMatcherBuild(#[from] browser_controller_client::TabMatcherBuilderError),

    /// An instance matcher could not be built.
    #[error("invalid instance matcher: {0}")]
    InstanceMatcherBuild(#[from] browser_controller_client::InstanceMatcherBuilderError),

    /// Open-tab parameters could not be built.
    #[error("invalid open-tab params: {0}")]
    OpenTabParamsBuild(#[from] browser_controller_client::OpenTabParamsBuilderError),
}

/// Browser to install the native messaging host manifest for (CLI argument type).
///
/// Mirrors [`browser_controller_client::BrowserKind`] but derives [`clap::ValueEnum`]
/// to allow direct CLI parsing.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliBrowserTarget {
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

impl From<CliBrowserTarget> for browser_controller_client::BrowserKind {
    /// Convert a [`CliBrowserTarget`] CLI value into the client library's
    /// [`BrowserKind`](browser_controller_client::BrowserKind) type.
    fn from(value: CliBrowserTarget) -> Self {
        match value {
            CliBrowserTarget::Firefox => Self::Firefox,
            CliBrowserTarget::Librewolf => Self::Librewolf,
            CliBrowserTarget::Waterfox => Self::Waterfox,
            CliBrowserTarget::Chrome => Self::Chrome,
            CliBrowserTarget::Chromium => Self::Chromium,
            CliBrowserTarget::Brave => Self::Brave,
            CliBrowserTarget::Edge => Self::Edge,
        }
    }
}

/// Controls behavior when a matcher criterion matches more than one window or tab (CLI argument type).
///
/// Mirrors [`browser_controller_client::MultipleMatchBehavior`] but derives [`clap::ValueEnum`]
/// to allow direct CLI parsing.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CliMultipleMatchBehavior {
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

impl From<CliMultipleMatchBehavior> for browser_controller_client::MultipleMatchBehavior {
    /// Convert a [`CliMultipleMatchBehavior`] CLI value into the client library's
    /// [`MultipleMatchBehavior`](browser_controller_client::MultipleMatchBehavior) type.
    fn from(value: CliMultipleMatchBehavior) -> Self {
        match value {
            CliMultipleMatchBehavior::Abort => Self::Abort,
            CliMultipleMatchBehavior::All => Self::All,
        }
    }
}

/// Convert a pair of positive/negative boolean CLI flags into an optional
/// [`BooleanCondition`].
///
/// When `positive` is `true`, returns `Some(BooleanCondition::Is)`. When `negative`
/// is `true`, returns `Some(BooleanCondition::IsNot)`. When both are `false` (the
/// default), returns `None` (no filtering). Clap `conflicts_with` prevents both
/// being `true` simultaneously.
#[must_use]
const fn bool_pair_to_condition(positive: bool, negative: bool) -> Option<BooleanCondition> {
    match (positive, negative) {
        (true, false) => Some(BooleanCondition::Is),
        (false, true) => Some(BooleanCondition::IsNot),
        _ => None,
    }
}

/// CLI representation of a window's visual state, for use with `--window-state`.
///
/// Mirrors [`WindowState`] but derives [`clap::ValueEnum`] to allow direct CLI parsing.
///
/// [`WindowState`]: browser_controller_client::WindowState
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
/// [`TabStatus`]: browser_controller_client::TabStatus
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

/// Criteria for selecting one or more browser windows (CLI argument type).
///
/// All provided criteria are combined with AND logic. If no criteria are specified,
/// every window will match, which will produce an error unless
/// `--if-matches-multiple all` is also passed.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each bool is an independent opt-in filter flag; there is no simpler representation"
)]
#[derive(clap::Args, Debug, Default)]
pub struct CliWindowMatcher {
    /// Match a window by its exact browser-assigned numeric ID.
    #[clap(long)]
    pub window_id: Option<WindowId>,
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
    #[clap(long = "if-window-matches-multiple", default_value = "abort")]
    pub if_matches_multiple: CliMultipleMatchBehavior,
}

impl TryFrom<CliWindowMatcher> for browser_controller_client::WindowMatcher {
    type Error = Error;
    fn try_from(m: CliWindowMatcher) -> Result<Self, Self::Error> {
        let mut b = Self::builder();
        if let Some(v) = m.window_id {
            b.window_id(v);
        }
        if let Some(v) = m.window_title {
            b.window_title(v);
        }
        if let Some(v) = m.window_title_prefix {
            b.window_title_prefix(v);
        }
        if let Some(v) = m.window_title_regex {
            b.window_title_regex(v);
        }
        if let Some(v) = bool_pair_to_condition(m.window_focused, m.window_not_focused) {
            b.window_focused(v);
        }
        if let Some(v) = bool_pair_to_condition(m.window_last_focused, m.window_not_last_focused) {
            b.window_last_focused(v);
        }
        if let Some(v) = m.window_state {
            b.window_state(WindowState::from(v));
        }
        b.if_matches_multiple(m.if_matches_multiple.into());
        Ok(b.build()?)
    }
}

impl TryFrom<&CliWindowMatcher> for browser_controller_client::WindowMatcher {
    type Error = Error;
    fn try_from(m: &CliWindowMatcher) -> Result<Self, Self::Error> {
        let mut b = Self::builder();
        if let Some(v) = m.window_id {
            b.window_id(v);
        }
        if let Some(ref v) = m.window_title {
            b.window_title(v.clone());
        }
        if let Some(ref v) = m.window_title_prefix {
            b.window_title_prefix(v.clone());
        }
        if let Some(ref v) = m.window_title_regex {
            b.window_title_regex(v.clone());
        }
        if let Some(v) = bool_pair_to_condition(m.window_focused, m.window_not_focused) {
            b.window_focused(v);
        }
        if let Some(v) = bool_pair_to_condition(m.window_last_focused, m.window_not_last_focused) {
            b.window_last_focused(v);
        }
        if let Some(v) = m.window_state {
            b.window_state(WindowState::from(v));
        }
        b.if_matches_multiple(m.if_matches_multiple.into());
        Ok(b.build()?)
    }
}

/// Criteria for selecting one or more browser tabs (CLI argument type).
///
/// All provided criteria are combined with AND logic. If no criteria are specified,
/// every tab in every searched window will match, which will produce an error unless
/// `--if-matches-multiple all` is also passed.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each bool is an independent opt-in filter flag mirroring the boolean fields of TabDetails; there is no simpler representation"
)]
#[derive(clap::Args, Debug, Default)]
pub struct CliTabMatcher {
    /// Match a tab by its exact browser-assigned numeric ID.
    #[clap(long)]
    pub tab_id: Option<TabId>,
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
    /// Match only tabs that have the attention flag set (e.g. unread notification).
    #[clap(long)]
    pub tab_has_attention: bool,
    /// Match only tabs that do not have the attention flag set.
    #[clap(long, conflicts_with = "tab_has_attention")]
    pub tab_not_has_attention: bool,
    /// Match only tabs with this loading status.
    #[clap(long)]
    pub tab_status: Option<TabStatusArg>,
    /// Match only tabs in a specific Firefox container (by cookie store ID).
    #[clap(long)]
    pub tab_cookie_store_id: Option<CookieStoreId>,
    /// Match only tabs in a specific Firefox container (by container name).
    #[clap(long)]
    pub tab_container_name: Option<String>,
    /// How to handle a criterion that matches multiple tabs.
    ///
    /// `abort` (the default) treats more than one match as an error.
    /// `all` applies the command to every matched tab.
    #[clap(long = "if-tab-matches-multiple", default_value = "abort")]
    pub if_matches_multiple: CliMultipleMatchBehavior,
}

/// Apply an `Option<BooleanCondition>` to a builder setter if present.
macro_rules! set_bool_condition {
    ($builder:expr, $setter:ident, $pos:expr, $neg:expr) => {
        if let Some(v) = bool_pair_to_condition($pos, $neg) {
            $builder.$setter(v);
        }
    };
}

impl TryFrom<CliTabMatcher> for browser_controller_client::TabMatcher {
    type Error = Error;
    fn try_from(m: CliTabMatcher) -> Result<Self, Self::Error> {
        let mut b = Self::builder();
        if let Some(v) = m.tab_id {
            b.tab_id(v);
        }
        if let Some(v) = m.tab_title {
            b.tab_title(v);
        }
        if let Some(v) = m.tab_title_regex {
            b.tab_title_regex(v);
        }
        if let Some(v) = m.tab_url {
            b.tab_url(v);
        }
        if let Some(v) = m.tab_url_domain {
            b.tab_url_domain(v);
        }
        if let Some(v) = m.tab_url_regex {
            b.tab_url_regex(v);
        }
        set_bool_condition!(b, tab_active, m.tab_active, m.tab_not_active);
        set_bool_condition!(b, tab_pinned, m.tab_pinned, m.tab_not_pinned);
        set_bool_condition!(b, tab_discarded, m.tab_discarded, m.tab_not_discarded);
        set_bool_condition!(b, tab_audible, m.tab_audible, m.tab_not_audible);
        set_bool_condition!(b, tab_muted, m.tab_muted, m.tab_not_muted);
        set_bool_condition!(b, tab_incognito, m.tab_incognito, m.tab_not_incognito);
        set_bool_condition!(
            b,
            tab_awaiting_auth,
            m.tab_awaiting_auth,
            m.tab_not_awaiting_auth
        );
        set_bool_condition!(
            b,
            tab_in_reader_mode,
            m.tab_in_reader_mode,
            m.tab_not_in_reader_mode
        );
        set_bool_condition!(
            b,
            tab_has_attention,
            m.tab_has_attention,
            m.tab_not_has_attention
        );
        if let Some(v) = m.tab_status {
            b.tab_status(TabStatus::from(v));
        }
        if let Some(v) = m.tab_cookie_store_id {
            b.tab_cookie_store_id(v);
        }
        if let Some(v) = m.tab_container_name {
            b.tab_container_name(v);
        }
        b.if_matches_multiple(m.if_matches_multiple.into());
        Ok(b.build()?)
    }
}

impl TryFrom<&CliTabMatcher> for browser_controller_client::TabMatcher {
    type Error = Error;
    fn try_from(m: &CliTabMatcher) -> Result<Self, Self::Error> {
        let mut b = Self::builder();
        if let Some(v) = m.tab_id {
            b.tab_id(v);
        }
        if let Some(ref v) = m.tab_title {
            b.tab_title(v.clone());
        }
        if let Some(ref v) = m.tab_title_regex {
            b.tab_title_regex(v.clone());
        }
        if let Some(ref v) = m.tab_url {
            b.tab_url(v.clone());
        }
        if let Some(ref v) = m.tab_url_domain {
            b.tab_url_domain(v.clone());
        }
        if let Some(ref v) = m.tab_url_regex {
            b.tab_url_regex(v.clone());
        }
        set_bool_condition!(b, tab_active, m.tab_active, m.tab_not_active);
        set_bool_condition!(b, tab_pinned, m.tab_pinned, m.tab_not_pinned);
        set_bool_condition!(b, tab_discarded, m.tab_discarded, m.tab_not_discarded);
        set_bool_condition!(b, tab_audible, m.tab_audible, m.tab_not_audible);
        set_bool_condition!(b, tab_muted, m.tab_muted, m.tab_not_muted);
        set_bool_condition!(b, tab_incognito, m.tab_incognito, m.tab_not_incognito);
        set_bool_condition!(
            b,
            tab_awaiting_auth,
            m.tab_awaiting_auth,
            m.tab_not_awaiting_auth
        );
        set_bool_condition!(
            b,
            tab_in_reader_mode,
            m.tab_in_reader_mode,
            m.tab_not_in_reader_mode
        );
        set_bool_condition!(
            b,
            tab_has_attention,
            m.tab_has_attention,
            m.tab_not_has_attention
        );
        if let Some(v) = m.tab_status {
            b.tab_status(TabStatus::from(v));
        }
        if let Some(ref v) = m.tab_cookie_store_id {
            b.tab_cookie_store_id(v.clone());
        }
        if let Some(ref v) = m.tab_container_name {
            b.tab_container_name(v.clone());
        }
        b.if_matches_multiple(m.if_matches_multiple.into());
        Ok(b.build()?)
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
    EventStream {
        /// Only show download-related events.
        #[clap(long)]
        downloads: bool,
        /// Only show window and tab events.
        #[clap(long)]
        windows_tabs: bool,
    },
    /// Manage browser windows.
    Windows(WindowsArgs),
    /// Manage tabs within a browser window.
    Tabs(Box<TabsArgs>),
    /// Manage downloads.
    Downloads(DownloadsArgs),
    /// Manage Firefox containers (contextual identities).
    Containers(ContainersArgs),
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
        browser: CliBrowserTarget,
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
    /// Load (or reload) a temporary extension in a running Firefox instance.
    ///
    /// Connects to Firefox's Remote Debugging Protocol and calls
    /// `installTemporaryAddon`. This works for both initial loading and
    /// reloading an already-loaded extension.
    ///
    /// Firefox must have remote debugging enabled. Set these in `about:config`:
    ///   - `devtools.debugger.remote-enabled` = `true`
    ///   - `devtools.chrome.enabled` = `true`
    ///   - `devtools.debugger.prompt-connection` = `false`
    ///
    /// Then start Firefox with `--start-debugger-server <port>` (space-separated,
    /// not `=`-separated), or press Shift+F2 and type `listen` without restarting.
    ///
    /// NOTE: This command is intended for development and testing of unreleased
    /// extension versions only. Temporary extensions are removed when Firefox
    /// restarts. For production use, install the extension normally through
    /// `about:addons` or the Mozilla Add-ons website.
    LoadExtension {
        /// Path to the unpacked extension directory to load.
        #[clap(long)]
        path: std::path::PathBuf,
        /// Port of Firefox's Remote Debugging Protocol server.
        #[clap(long, default_value = "6000")]
        port: u16,
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
        /// Open the window in private/incognito browsing mode.
        ///
        /// The extension must be allowed to run in private windows for tabs in the
        /// new window to be controllable.
        #[clap(long)]
        incognito: bool,
    },
    /// Close one or more browser windows.
    Close {
        /// Criteria selecting the window(s) to close.
        #[clap(flatten)]
        window: CliWindowMatcher,
    },
    /// Set the title prefix (Firefox `titlePreface`) for one or more windows.
    SetTitlePrefix {
        /// Criteria selecting the window(s) to modify.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Prefix to prepend to the window title.
        prefix: String,
    },
    /// Remove the title prefix from one or more windows.
    RemoveTitlePrefix {
        /// Criteria selecting the window(s) to modify.
        #[clap(flatten)]
        window: CliWindowMatcher,
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
        window: CliWindowMatcher,
    },
    /// Open a new tab in a window.
    Open {
        /// Criteria selecting the window in which to open the tab.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Insert the new tab immediately before the tab with this ID.
        #[clap(long, conflicts_with = "after")]
        before: Option<TabId>,
        /// Insert the new tab immediately after the tab with this ID.
        #[clap(long, conflicts_with = "before")]
        after: Option<TabId>,
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
        /// Open the tab in a specific Firefox container (cookie store ID).
        ///
        /// E.g. `firefox-container-1`. Use `containers list` to see available containers.
        /// Ignored on browsers without container support.
        #[clap(long)]
        container: Option<CookieStoreId>,
    },
    /// Activate a tab, making it the focused tab in its window.
    Activate {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to activate.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Navigate an existing tab to a new URL.
    Navigate {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to navigate.
        #[clap(flatten)]
        tab: CliTabMatcher,
        /// URL to load in the tab.
        #[clap(long)]
        url: String,
    },
    /// Close one or more tabs and reopen them in a different Firefox container.
    ///
    /// The tabs are closed and new tabs are created in the target container
    /// with the same URLs. Firefox-only.
    ReopenInContainer {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to reopen.
        #[clap(flatten)]
        tab: CliTabMatcher,
        /// Target container's cookie store ID (e.g. `firefox-container-1`).
        #[clap(long)]
        container: CookieStoreId,
    },
    /// Reload one or more tabs.
    Reload {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to reload.
        #[clap(flatten)]
        tab: CliTabMatcher,
        /// Bypass the browser cache (hard refresh).
        #[clap(long)]
        bypass_cache: bool,
    },
    /// Close one or more tabs.
    Close {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to close.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Pin one or more tabs.
    Pin {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to pin.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Unpin one or more tabs.
    Unpin {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to unpin.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Toggle Reader Mode for one or more tabs.
    ///
    /// Firefox-only. The tab must be displaying a reader-mode-compatible page.
    ToggleReaderMode {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to toggle.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Discard one or more tabs, unloading their content from memory without closing them.
    ///
    /// The tabs remain in the tab strip but their content is freed. They will be
    /// reloaded when activated. The active tab cannot be discarded.
    Discard {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to discard.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Warm up one or more discarded tabs, loading their content into memory without activating.
    Warmup {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to warm up.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Mute one or more tabs, suppressing any audio they produce.
    Mute {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to mute.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Unmute one or more tabs, allowing them to produce audio again.
    Unmute {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to unmute.
        #[clap(flatten)]
        tab: CliTabMatcher,
    },
    /// Move a tab to a new position within its window.
    Move {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to move.
        #[clap(flatten)]
        tab: CliTabMatcher,
        /// New zero-based index for the tab within its window.
        #[clap(long)]
        new_index: u32,
    },
    /// Navigate backward in a tab's session history.
    Back {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to navigate backward.
        #[clap(flatten)]
        tab: CliTabMatcher,
        /// Number of steps to go back.
        ///
        /// Values greater than 1 skip intermediate pages atomically, which is useful
        /// when those pages redirect immediately forward again.
        #[clap(long, default_value_t = 1u32)]
        steps: u32,
    },
    /// Navigate forward in a tab's session history.
    Forward {
        /// Criteria selecting the window(s) to search for tabs.
        #[clap(flatten)]
        window: CliWindowMatcher,
        /// Criteria selecting the tab(s) to navigate forward.
        #[clap(flatten)]
        tab: CliTabMatcher,
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
        window: CliWindowMatcher,
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

/// CLI argument type for download state filtering.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStateArg {
    /// Downloads that are actively receiving data.
    InProgress,
    /// Downloads that completed successfully.
    Complete,
    /// Downloads that were interrupted.
    Interrupted,
}

impl From<DownloadStateArg> for browser_controller_client::DownloadState {
    fn from(arg: DownloadStateArg) -> Self {
        match arg {
            DownloadStateArg::InProgress => Self::InProgress,
            DownloadStateArg::Complete => Self::Complete,
            DownloadStateArg::Interrupted => Self::Interrupted,
        }
    }
}

/// CLI argument type for filename conflict handling.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilenameConflictActionArg {
    /// Add a number to the filename to make it unique.
    Uniquify,
    /// Overwrite the existing file.
    Overwrite,
    /// Prompt the user.
    Prompt,
}

impl From<FilenameConflictActionArg> for browser_controller_client::FilenameConflictAction {
    fn from(arg: FilenameConflictActionArg) -> Self {
        match arg {
            FilenameConflictActionArg::Uniquify => Self::Uniquify,
            FilenameConflictActionArg::Overwrite => Self::Overwrite,
            FilenameConflictActionArg::Prompt => Self::Prompt,
        }
    }
}

/// Arguments for the `downloads` subcommand group.
#[derive(clap::Args, Debug)]
pub struct DownloadsArgs {
    /// Download operation to perform.
    #[clap(subcommand)]
    command: DownloadsCommand,
}

/// Operations on downloads.
#[derive(clap::Subcommand, Debug)]
pub enum DownloadsCommand {
    /// List downloads, optionally filtered by state.
    List {
        /// Filter by download state.
        #[clap(long)]
        state: Option<DownloadStateArg>,
        /// Maximum number of results.
        #[clap(long)]
        limit: Option<u32>,
        /// Free-text search query matching URL and filename.
        #[clap(long)]
        query: Option<String>,
    },
    /// Start a new download.
    Start {
        /// URL to download.
        #[clap(long)]
        url: String,
        /// Filename relative to the downloads folder.
        #[clap(long)]
        filename: Option<String>,
        /// Show the Save As dialog.
        #[clap(long)]
        save_as: bool,
        /// How to handle filename conflicts.
        #[clap(long)]
        conflict_action: Option<FilenameConflictActionArg>,
    },
    /// Cancel an active download.
    Cancel {
        /// Download ID to cancel.
        #[clap(long)]
        id: DownloadId,
    },
    /// Pause an active download.
    Pause {
        /// Download ID to pause.
        #[clap(long)]
        id: DownloadId,
    },
    /// Resume a paused download.
    Resume {
        /// Download ID to resume.
        #[clap(long)]
        id: DownloadId,
    },
    /// Retry an interrupted download by re-downloading from the same URL.
    Retry {
        /// Download ID to retry.
        #[clap(long)]
        id: DownloadId,
    },
    /// Remove a download from the browser's history (the file stays on disk).
    Erase {
        /// Download ID to remove.
        #[clap(long)]
        id: DownloadId,
    },
    /// Clear all downloads from history, optionally filtered by state.
    Clear {
        /// Only clear downloads in this state.
        #[clap(long)]
        state: Option<DownloadStateArg>,
    },
}

/// Arguments for the `containers` subcommand group.
#[derive(clap::Args, Debug)]
pub struct ContainersArgs {
    /// Container operation to perform.
    #[clap(subcommand)]
    command: ContainersCommand,
}

/// Operations on Firefox containers (contextual identities).
#[derive(clap::Subcommand, Debug)]
pub enum ContainersCommand {
    /// List all available containers.
    List,
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
                if let Some(ref cid) = tab.cookie_store_id {
                    if let Some(ref name) = tab.container_name {
                        println!("  Container: {name} ({cid})");
                    } else {
                        println!("  Container: {cid}");
                    }
                }
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
            if let Some(ref cid) = tab.cookie_store_id {
                println!("  Container: {cid}");
            }
        }
        CliResult::Containers { containers } => {
            for c in containers {
                println!("{} \u{2014} {} ({})", c.cookie_store_id, c.name, c.color,);
                println!("  Color: {} ({})  Icon: {}", c.color, c.color_code, c.icon);
            }
        }
        CliResult::Downloads { downloads } => {
            for dl in downloads {
                let progress = if let Some(total) = dl.total_bytes {
                    format!("{}/{} bytes", dl.bytes_received, total)
                } else {
                    format!("{} bytes", dl.bytes_received)
                };
                let error_info = dl
                    .error
                    .as_deref()
                    .map(|e| format!(" error={e}"))
                    .unwrap_or_default();
                println!(
                    "Download {} \u{2014} {} [{}]{}{}",
                    dl.id,
                    dl.state,
                    progress,
                    if dl.paused { " paused" } else { "" },
                    error_info,
                );
                println!("  URL:      {}", dl.url);
                println!("  File:     {}", dl.filename);
                if let Some(ref mime) = dl.mime {
                    println!("  MIME:     {mime}");
                }
                println!("  Started:  {}", dl.start_time);
                if let Some(ref end) = dl.end_time {
                    println!("  Ended:    {end}");
                }
                println!(
                    "  Exists: {}  Can resume: {}  Incognito: {}",
                    yn(dl.exists),
                    yn(dl.can_resume),
                    yn(dl.incognito),
                );
            }
        }
        CliResult::DownloadId { download_id } => {
            println!("Download {download_id}");
        }
        CliResult::Unit => {}
        _ => {
            // Unknown result variant — print as JSON fallback.
            println!("{}", serde_json::to_string_pretty(result)?);
        }
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

/// Connect to a mediator and stream browser events as newline-delimited JSON to stdout.
///
/// Subscribes to events via [`Client::subscribe_events_filtered`] and then
/// prints each event as a JSON line to stdout. Filtering is performed
/// server-side by the mediator, so every event received is printed.
/// Runs until the connection closes or an error occurs.
///
/// # Errors
///
/// Returns an error if the connection or I/O fails.
#[expect(
    clippy::print_stdout,
    reason = "event stream output goes to stdout by design"
)]
async fn stream_events(
    client: &Client,
    filter_downloads: bool,
    filter_windows_tabs: bool,
) -> Result<(), Error> {
    let mut events = client
        .subscribe_events_filtered(filter_windows_tabs, filter_downloads)
        .await?;
    while let Some(event) = events.next_event().await? {
        println!("{}", serde_json::to_string(&event)?);
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
            let instances = browser_controller_client::discover_instances().await?;
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
            let result = browser_controller_client::install_manifest(
                (*browser).into(),
                mediator_path.clone(),
                extension_id.clone(),
            )?;
            #[expect(
                clippy::print_stdout,
                reason = "manifest installation result goes to stdout by design"
            )]
            match cli.output {
                OutputFormat::Human => {
                    println!("Installed manifest to {}", result.manifest_path.display());
                    #[cfg(target_os = "windows")]
                    {
                        let client_browser: browser_controller_client::BrowserKind =
                            (*browser).into();
                        println!(
                            "Registered in HKCU\\{}",
                            client_browser.windows_registry_key()
                        );
                    }
                }
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            }
            return Ok(());
        }
        Command::LoadExtension { path, port } => {
            let addon_id = browser_controller_client::load_temporary_extension(path, *port).await?;
            #[expect(clippy::print_stdout, reason = "command output goes to stdout")]
            {
                println!("Loaded extension: {addon_id}");
            }
            return Ok(());
        }
        Command::Windows(_)
        | Command::Tabs(_)
        | Command::Downloads(_)
        | Command::Containers(_)
        | Command::EventStream { .. } => {}
    }

    // Commands that require a browser connection.
    let instances = browser_controller_client::discover_instances().await?;
    let dir = browser_controller_client::socket_dir()?;
    let instance =
        browser_controller_client::select_instance(&instances, cli.instance.as_deref(), &dir)?;
    tracing::debug!(
        browser = %instance.info.browser_name,
        pid = instance.info.pid,
        "Selected browser instance",
    );

    if let Command::EventStream {
        downloads,
        windows_tabs,
    } = &cli.command
    {
        // Event streaming is long-lived; use a large timeout for the
        // initial subscribe command but events themselves are unbounded.
        let client = Client::new(instance.socket_path.clone(), Duration::from_secs(30));
        stream_events(&client, *downloads, *windows_tabs).await?;
        return Ok(());
    }

    execute_command(cli, instance).await
}

/// Execute the selected command against the given browser instance.
///
/// # Errors
///
/// Returns an error if the command fails.
async fn execute_command(cli: Cli, instance: &DiscoveredInstance) -> Result<(), Error> {
    let client = Client::new(
        instance.socket_path.clone(),
        Duration::from_secs(cli.timeout),
    );
    match cli.command {
        Command::Windows(w) => match w.command {
            WindowsCommand::List => {
                let windows = client.list_windows().await?;
                print_result(&CliResult::Windows { windows }, cli.output)?;
            }
            WindowsCommand::Open {
                title_prefix,
                if_title_prefix_does_not_exist,
                incognito,
            } => {
                // Guard: skip opening if a window with the required prefix already exists.
                if if_title_prefix_does_not_exist && let Some(ref required_prefix) = title_prefix {
                    let windows = client.list_windows().await?;
                    if windows
                        .iter()
                        .any(|w| w.title_prefix.as_deref() == Some(required_prefix.as_str()))
                    {
                        return Ok(());
                    }
                }
                let window_id = client.open_window(title_prefix, incognito).await?;
                print_result(&CliResult::WindowId { window_id }, cli.output)?;
            }
            WindowsCommand::Close { window } => {
                // Zero matches is not an error for close — the desired state
                // (no matching windows exist) is already achieved.
                let window_ids = match client.resolve_windows(&((&window).try_into()?)).await {
                    Ok(ids) => ids,
                    Err(browser_controller_client::Error::NoMatchingWindow { .. }) => Vec::new(),
                    Err(e) => return Err(e.into()),
                };
                for window_id in window_ids {
                    client.close_window(window_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            WindowsCommand::SetTitlePrefix { window, prefix } => {
                let window_ids = client.resolve_windows(&((&window).try_into()?)).await?;
                for window_id in window_ids {
                    client
                        .set_window_title_prefix(window_id, prefix.clone())
                        .await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            WindowsCommand::RemoveTitlePrefix { window } => {
                let window_ids = client.resolve_windows(&((&window).try_into()?)).await?;
                for window_id in window_ids {
                    client.remove_window_title_prefix(window_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
        },
        Command::Tabs(t) => match t.command {
            TabsCommand::List { window } => {
                let window_ids = client.resolve_windows(&((&window).try_into()?)).await?;
                for window_id in window_ids {
                    let tabs = client.list_tabs(window_id).await?;
                    print_result(&CliResult::Tabs { tabs }, cli.output)?;
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
                container,
            } => {
                // Guard: skip opening if a tab with the given URL already exists anywhere.
                if if_url_does_not_exist && let Some(ref check_url) = url {
                    let windows = client.list_windows().await?;
                    let already_exists = windows
                        .iter()
                        .flat_map(|w| &w.tabs)
                        .any(|t| t.url == *check_url);
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

                let window_ids = client.resolve_windows(&((&window).try_into()?)).await?;
                for window_id in window_ids {
                    let mut b = OpenTabParamsBuilder::default();
                    b.window_id(window_id);
                    if let Some(ref v) = before {
                        b.insert_before_tab_id(*v);
                    }
                    if let Some(ref v) = after {
                        b.insert_after_tab_id(*v);
                    }
                    if let Some(ref v) = url {
                        b.url(v.clone());
                    }
                    if let Some(ref v) = username {
                        b.username(v.clone());
                    }
                    if let Some(ref v) = password {
                        b.password(v.clone());
                    }
                    b.background(background);
                    if let Some(ref v) = container {
                        b.cookie_store_id(v.clone());
                    }
                    let params = b.build()?;
                    let tab = client.open_tab(params).await?;
                    print_result(&CliResult::Tab(tab), cli.output)?;
                }
            }
            TabsCommand::Activate { window, tab } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.activate_tab(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Navigate { window, tab, url } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.navigate_tab(tab_id, url.clone()).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::ReopenInContainer {
                window,
                tab,
                container,
            } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    let tab = client
                        .reopen_tab_in_container(tab_id, container.clone())
                        .await?;
                    print_result(&CliResult::Tab(tab), cli.output)?;
                }
            }
            TabsCommand::Reload {
                window,
                tab,
                bypass_cache,
            } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.reload_tab(tab_id, bypass_cache).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Close { window, tab } => {
                // Zero matches is not an error for close — the desired state
                // (no matching tabs exist) is already achieved.
                let tab_ids = match client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await
                {
                    Ok(ids) => ids,
                    Err(browser_controller_client::Error::NoMatchingTab { .. }) => Vec::new(),
                    Err(e) => return Err(e.into()),
                };
                for tab_id in tab_ids {
                    client.close_tab(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Pin { window, tab } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.pin_tab(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Unpin { window, tab } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.unpin_tab(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::ToggleReaderMode { window, tab } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.toggle_reader_mode(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Discard { window, tab } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.discard_tab(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Warmup { window, tab } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.warmup_tab(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Mute { window, tab } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.mute_tab(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Unmute { window, tab } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    client.unmute_tab(tab_id).await?;
                    print_result(&CliResult::Unit, cli.output)?;
                }
            }
            TabsCommand::Move {
                window,
                tab,
                new_index,
            } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    let tab = client.move_tab(tab_id, new_index).await?;
                    print_result(&CliResult::Tab(tab), cli.output)?;
                }
            }
            TabsCommand::Back { window, tab, steps } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    let tab = client.go_back(tab_id, steps).await?;
                    print_result(&CliResult::Tab(tab), cli.output)?;
                }
            }
            TabsCommand::Forward { window, tab, steps } => {
                let tab_ids = client
                    .resolve_tabs(&((&window).try_into()?), &((&tab).try_into()?))
                    .await?;
                for tab_id in tab_ids {
                    let tab = client.go_forward(tab_id, steps).await?;
                    print_result(&CliResult::Tab(tab), cli.output)?;
                }
            }
            TabsCommand::Sort { window, domains } => {
                let window_ids = client.resolve_windows(&((&window).try_into()?)).await?;
                for window_id in window_ids {
                    let mut tabs = client.list_tabs(window_id).await?;

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
                            drop(client.move_tab(tab.id, new_index as u32).await?);
                        }
                    }
                }
            }
        },
        Command::Downloads(d) => match d.command {
            DownloadsCommand::List {
                state,
                limit,
                query,
            } => {
                let downloads = client
                    .list_downloads(state.map(Into::into), limit, query)
                    .await?;
                print_result(&CliResult::Downloads { downloads }, cli.output)?;
            }
            DownloadsCommand::Start {
                url,
                filename,
                save_as,
                conflict_action,
            } => {
                let download_id = client
                    .start_download(url, filename, save_as, conflict_action.map(Into::into))
                    .await?;
                print_result(&CliResult::DownloadId { download_id }, cli.output)?;
            }
            DownloadsCommand::Cancel { id } => {
                client.cancel_download(id).await?;
                print_result(&CliResult::Unit, cli.output)?;
            }
            DownloadsCommand::Pause { id } => {
                client.pause_download(id).await?;
                print_result(&CliResult::Unit, cli.output)?;
            }
            DownloadsCommand::Resume { id } => {
                client.resume_download(id).await?;
                print_result(&CliResult::Unit, cli.output)?;
            }
            DownloadsCommand::Retry { id } => {
                client.retry_download(id).await?;
                print_result(&CliResult::Unit, cli.output)?;
            }
            DownloadsCommand::Erase { id } => {
                client.erase_download(id).await?;
                print_result(&CliResult::Unit, cli.output)?;
            }
            DownloadsCommand::Clear { state } => {
                client.erase_all_downloads(state.map(Into::into)).await?;
                print_result(&CliResult::Unit, cli.output)?;
            }
        },
        Command::Containers(c) => match c.command {
            ContainersCommand::List => {
                let containers = client.list_containers().await?;
                print_result(&CliResult::Containers { containers }, cli.output)?;
            }
        },
        // Already handled above.
        Command::Instances
        | Command::EventStream { .. }
        | Command::GenerateManpage { .. }
        | Command::GenerateShellCompletion { .. }
        | Command::InstallManifest { .. }
        | Command::LoadExtension { .. } => {}
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
    use browser_controller_client::{
        MatchWith as _, TabDetails, TabId, TabStatus, WindowId, WindowState, WindowSummary,
    };

    /// Build a minimal [`WindowSummary`] for use in tests.
    fn make_window(id: u32, title: &str) -> WindowSummary {
        WindowSummary::new(
            WindowId(id),
            title.to_owned(),
            None,
            false,
            false,
            WindowState::Normal,
            vec![],
        )
    }

    /// Builder for [`TabDetails`] with sensible defaults.
    ///
    /// Only `id` and `window_id` are required; everything else defaults to
    /// a safe zero/false/None value. Call setter methods for fields your test
    /// cares about, then `.build()`.
    struct TabBuilder {
        /// The inner tab details being constructed.
        inner: TabDetails,
    }

    impl TabBuilder {
        /// Create a new builder with the given tab and window IDs.
        fn new(id: u32, window_id: u32) -> Self {
            Self {
                inner: TabDetails::new(
                    TabId(id),
                    0,
                    WindowId(window_id),
                    String::new(),
                    String::new(),
                    false,
                    false,
                    false,
                    false,
                    false,
                    TabStatus::Complete,
                    false,
                    false,
                    false,
                    false,
                    0,
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            }
        }

        /// Set the tab title.
        fn title(mut self, t: &str) -> Self {
            self.inner.title = t.to_owned();
            self
        }

        /// Set the tab URL.
        fn url(mut self, u: &str) -> Self {
            self.inner.url = u.to_owned();
            self
        }

        /// Build the final [`TabDetails`].
        fn build(self) -> TabDetails {
            self.inner
        }
    }

    /// Shorthand for building a tab with an ID, window ID, title, and URL.
    fn make_tab(id: u32, window_id: u32, title: &str, url: &str) -> TabDetails {
        TabBuilder::new(id, window_id).title(title).url(url).build()
    }

    /// Verify that `--window-id` selects exactly the window with that ID.
    #[test]
    fn match_windows_by_id() -> Result<(), crate::Error> {
        let windows = vec![make_window(1, "Window One"), make_window(2, "Window Two")];
        let m = browser_controller_client::WindowMatcher::builder()
            .window_id(WindowId(1))
            .build()?;
        let matched: Vec<WindowId> = windows.match_with(&m)?.iter().map(|w| w.id).collect();
        pretty_assertions::assert_eq!(matched, vec![WindowId(1)]);
        Ok(())
    }

    /// Verify that `--window-title` selects exactly the window with that exact title.
    #[test]
    fn match_windows_by_title() -> Result<(), crate::Error> {
        let windows = vec![make_window(1, "Work"), make_window(2, "Personal")];
        let m = browser_controller_client::WindowMatcher::builder()
            .window_title("Work")
            .build()?;
        let matched: Vec<WindowId> = windows.match_with(&m)?.iter().map(|w| w.id).collect();
        pretty_assertions::assert_eq!(matched, vec![WindowId(1)]);
        Ok(())
    }

    /// Verify that `--tab-id` selects exactly the tab with that ID.
    #[test]
    fn match_tabs_by_id() -> Result<(), crate::Error> {
        let tabs = vec![
            make_tab(10, 1, "Tab A", "https://example.com"),
            make_tab(11, 1, "Tab B", "https://other.com"),
        ];
        let m = browser_controller_client::TabMatcher::builder()
            .tab_id(TabId(10))
            .build()?;
        let matched: Vec<TabId> = tabs.match_with(&m)?.iter().map(|t| t.id).collect();
        pretty_assertions::assert_eq!(matched, vec![TabId(10)]);
        Ok(())
    }

    /// Verify that `--tab-title` selects exactly the tab with that exact title.
    #[test]
    fn match_tabs_by_title() -> Result<(), crate::Error> {
        let tabs = vec![
            make_tab(10, 1, "Dashboard", "https://example.com"),
            make_tab(11, 1, "Settings", "https://example.com/settings"),
        ];
        let m = browser_controller_client::TabMatcher::builder()
            .tab_title("Dashboard")
            .build()?;
        let matched: Vec<TabId> = tabs.match_with(&m)?.iter().map(|t| t.id).collect();
        pretty_assertions::assert_eq!(matched, vec![TabId(10)]);
        Ok(())
    }
}
