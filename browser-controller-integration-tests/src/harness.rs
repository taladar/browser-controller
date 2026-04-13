//! Test harness orchestrating the full browser-controller stack.
//!
//! [`Harness`] manages the lifecycle of:
//! 1. A WebDriver process (geckodriver or chromedriver)
//! 2. A browser instance with the extension installed (via WebDriver BiDi)
//! 3. The mediator process (launched by the browser via native messaging)
//!
//! Tests use [`run`] to get a harness with guaranteed cleanup.

use std::path::PathBuf;
use std::time::Duration;

use browser_controller_types::{CliCommand, CliResult};
use futures::FutureExt as _;
use webdriverbidi::session::WebDriverBiDiSession;

use crate::bidi;
use crate::browser;
use crate::cli;
use crate::driver;
use crate::mediator;
use crate::profile::{self, TestProfile};

/// Error type for harness operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The WebDriver binary was not found or failed to start.
    #[error("driver error: {0}")]
    Driver(#[from] driver::Error),
    /// Failed to create the test browser profile.
    #[error("profile error: {0}")]
    Profile(#[from] std::io::Error),
    /// The WebDriver BiDi session failed.
    #[error("BiDi error: {0}")]
    Bidi(#[from] bidi::Error),
    /// Mediator socket discovery failed.
    #[error("mediator error: {0}")]
    Mediator(#[from] mediator::Error),
    /// CLI command failed.
    #[error("CLI error: {0}")]
    Cli(#[from] cli::Error),
    /// The native messaging manifest is not installed.
    #[error(
        "native messaging manifest not found at {0}; run `browser-controller install-manifest` first"
    )]
    ManifestMissing(PathBuf),
    /// The extension directory could not be found.
    #[error("extension directory not found in workspace")]
    ExtensionNotFound,
}

/// The central integration test fixture.
///
/// Holds all resources needed for a test: the browser session, the mediator
/// socket path, and the browser PID for niri-based verification.
#[expect(
    missing_debug_implementations,
    reason = "WebDriverBiDiSession does not implement Debug"
)]
pub struct Harness {
    /// Which browser is under test.
    pub browser: browser::Kind,
    /// The WebDriver child process.
    driver: driver::Process,
    /// The WebDriver BiDi session.
    pub session: WebDriverBiDiSession,
    /// Path to the mediator's Unix Domain Socket.
    pub mediator_socket: PathBuf,
    /// PID of the browser process (for niri window filtering).
    pub browser_pid: Option<u32>,
    /// The test profile directory (dropped on cleanup).
    _profile: TestProfile,
}

impl Harness {
    /// Start the full test stack for the given browser.
    ///
    /// # Steps
    ///
    /// 1. Verify the native messaging manifest exists
    /// 2. Create a temporary browser profile
    /// 3. Start the WebDriver (geckodriver/chromedriver)
    /// 4. Snapshot the mediator socket directory
    /// 5. Create a BiDi session and install the extension
    /// 6. Wait for the mediator socket to appear
    /// 7. Confirm the mediator is responsive via `GetBrowserInfo`
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if any step fails.
    pub async fn start(browser: browser::Kind) -> Result<Self, Error> {
        // 1. Verify manifest exists
        if let Some(missing) = profile::check_manifest(browser) {
            return Err(Error::ManifestMissing(missing));
        }

        // 2. Locate the extension
        let ext_dir = profile::verified_extension_dir().ok_or(Error::ExtensionNotFound)?;

        // 3. Create temp profile
        let test_profile = TestProfile::new(browser)?;

        // 4. Start WebDriver
        let driver = driver::Process::start(browser).await?;

        // 5. Snapshot existing sockets
        let sock_dir = mediator::socket_dir()?;
        let pre_sockets = mediator::list_sockets(&sock_dir);

        // 6. Create BiDi session and install extension
        let mut session = bidi::create_session(browser, driver.port, &test_profile.path).await?;
        let _extension_id = bidi::install_extension(&mut session, &ext_dir).await?;

        // 7. Wait for new mediator socket
        let mediator_socket =
            mediator::wait_for_new_socket(&sock_dir, &pre_sockets, Duration::from_secs(15)).await?;

        // 8. Confirm mediator is responsive and get browser PID
        let browser_pid = Self::probe_browser_pid(&mediator_socket).await;

        Ok(Self {
            browser,
            driver,
            session,
            mediator_socket,
            browser_pid,
            _profile: test_profile,
        })
    }

    /// Send a CLI command to the mediator and return the result.
    ///
    /// # Errors
    ///
    /// Returns a [`cli::Error`] on communication or command failure.
    pub async fn send_command(&self, command: CliCommand) -> Result<CliResult, cli::Error> {
        cli::send_command(&self.mediator_socket, command).await
    }

    /// Shut down the test stack cleanly.
    ///
    /// Closes the BiDi session (which closes the browser and terminates the mediator),
    /// then kills the WebDriver process.
    pub async fn stop(mut self) {
        // Close the BiDi session (this closes the browser)
        let _close_result = self.session.close().await;
        // Kill the driver process
        let _kill_result = self.driver.child.kill().await;
    }

    /// Try to get the browser PID by sending `GetBrowserInfo`.
    ///
    /// Returns `None` if the command fails (e.g. mediator not ready yet).
    async fn probe_browser_pid(socket: &std::path::Path) -> Option<u32> {
        // Retry a few times since the mediator may still be initializing
        for _ in 0..5u8 {
            match cli::send_command(socket, CliCommand::GetBrowserInfo).await {
                Ok(CliResult::BrowserInfo(info)) => return Some(info.pid),
                _ => tokio::time::sleep(Duration::from_millis(500)).await,
            }
        }
        None
    }
}

/// Run a test with a [`Harness`], guaranteeing cleanup even on panic.
///
/// The test closure receives a `&Harness` and returns a pinned future.
/// Cleanup runs even if the test panics.
///
/// # Panics
///
/// Panics if the harness cannot be started (after skipping known-benign
/// errors like missing driver or manifest). Re-panics if the test closure panics,
/// after performing cleanup.
#[expect(
    clippy::future_not_send,
    reason = "integration tests are single-threaded"
)]
pub async fn run<F>(browser: browser::Kind, test: F)
where
    F: for<'a> FnOnce(&'a Harness) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>,
{
    #[expect(clippy::print_stderr, reason = "test skip messages go to stderr")]
    let harness = match Harness::start(browser).await {
        Ok(h) => h,
        Err(Error::Driver(driver::Error::NotFound(name))) => {
            eprintln!("SKIP: {name} not found, skipping test");
            return;
        }
        Err(Error::ManifestMissing(path)) => {
            eprintln!(
                "SKIP: native messaging manifest not found at {}; run `browser-controller install-manifest` first",
                path.display()
            );
            return;
        }
        #[expect(clippy::panic, reason = "test harness failure is unrecoverable")]
        Err(e) => panic!("failed to start test harness: {e}"),
    };

    let result = std::panic::AssertUnwindSafe(test(&harness))
        .catch_unwind()
        .await;

    harness.stop().await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}
