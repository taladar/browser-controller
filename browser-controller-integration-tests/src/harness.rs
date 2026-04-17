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

use browser_controller_client::Client;
use futures::FutureExt as _;
use webdriverbidi::session::WebDriverBiDiSession;

use crate::bidi;
use crate::browser;
use crate::cli;
use crate::driver;
use crate::mediator;
use crate::profile::{self, PreparedExtension, TestProfile};

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
    /// The prepared extension directory (may be a temp copy for Chrome).
    _extension: PreparedExtension,
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
        // 1. Verify manifest exists (for Firefox; Chrome manifest is patched below)
        if browser == browser::Kind::Firefox
            && let Some(missing) = profile::check_manifest(browser)
        {
            return Err(Error::ManifestMissing(missing));
        }

        // 2. For Chrome, ensure Chrome for Testing is downloaded.
        let cft = match browser {
            browser::Kind::Chrome => Some(
                crate::chrome_for_testing::ensure_installed()
                    .await
                    .map_err(|e| {
                        Error::Driver(driver::Error::StartFailed(
                            "chrome-for-testing".to_owned(),
                            std::io::Error::other(e.to_string()),
                        ))
                    })?,
            ),
            browser::Kind::Firefox => None,
        };

        // 3. Prepare the extension for this browser
        let ext = profile::prepared_extension_dir(browser)?;

        // 4. Create temp profile
        let test_profile = TestProfile::new(browser)?;

        // 5. Start WebDriver (use CfT chromedriver for Chrome)
        let driver = match &cft {
            Some(paths) => {
                driver::Process::start_with_binary(browser, Some(&paths.chromedriver)).await?
            }
            None => driver::Process::start(browser).await?,
        };

        // 6. For Chrome, write the NMH manifest inside the test profile.
        //    CfT looks for manifests at <user-data-dir>/NativeMessagingHosts/.
        if browser == browser::Kind::Chrome {
            profile::write_chrome_test_nmh_manifest(&test_profile.path)?;
        }

        // 7. Snapshot existing sockets
        let sock_dir = mediator::socket_dir()?;
        let pre_sockets = mediator::list_sockets(&sock_dir);

        // 8. Create BiDi session
        //    For Chrome: pass the CfT binary path and extension path so Chrome
        //    loads the extension via --load-extension at startup.
        //    For Firefox: extension is installed post-startup via BiDi.
        let chrome_binary = cft.as_ref().map(|p| p.chrome.as_path());
        let chrome_ext_path = match browser {
            browser::Kind::Chrome => Some(ext.path.as_path()),
            browser::Kind::Firefox => None,
        };
        let mut bidi_session = bidi::create_session(
            browser,
            driver.port,
            &test_profile.path,
            chrome_binary,
            chrome_ext_path,
        )
        .await?;

        // 9. Install extension (Firefox: BiDi; Chrome: already loaded via --load-extension)
        let _extension_id = bidi::install_extension(&mut bidi_session, browser, &ext.path).await?;

        // 10. Wait for new mediator socket
        let mediator_socket =
            mediator::wait_for_new_socket(&sock_dir, &pre_sockets, Duration::from_secs(15)).await?;

        // 11. Confirm mediator is responsive and get browser PID
        let browser_pid = Self::probe_browser_pid(&mediator_socket).await;

        Ok(Self {
            browser,
            driver,
            session: bidi_session,
            mediator_socket,
            browser_pid,
            _profile: test_profile,
            _extension: ext,
        })
    }

    /// Create a [`Client`] connected to this harness's mediator.
    #[must_use]
    pub fn client(&self) -> Client {
        cli::client(&self.mediator_socket)
    }

    /// Shut down the test stack cleanly.
    ///
    /// Closes the BiDi session (which closes the browser and terminates the mediator),
    /// then kills the WebDriver process.
    pub async fn stop(mut self) {
        // Close the BiDi session (this closes the browser).
        // Use a timeout because close() can hang if Chrome already crashed.
        let _close_result =
            tokio::time::timeout(Duration::from_secs(5), self.session.close()).await;
        // Kill the driver process (this also terminates the browser if still running).
        let _kill_result = self.driver.child.kill().await;
        // If we know the browser PID, kill its entire process group as a safety
        // net. Chrome spawns many child processes (GPU, renderer, crashpad, etc.)
        // that won't die from just killing the main PID. The negative PID in
        // `kill` targets the process group.
        if let Some(pid) = self.browser_pid {
            drop(
                std::process::Command::new("kill")
                    .arg("--")
                    .arg(format!("-{pid}"))
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status(),
            );
            // Also kill the main PID directly in case it's not in its own
            // process group (e.g. when launched by chromedriver).
            drop(
                std::process::Command::new("kill")
                    .arg(pid.to_string())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status(),
            );
        }
        // The mediator removes its socket file immediately on shutdown
        // (before process exit), so no wait is needed here.
    }

    /// Probe the mediator to get the browser PID and verify the extension is responsive.
    ///
    /// Sends `GetBrowserInfo` (handled locally by the mediator) to get the PID,
    /// then sends `ListWindows` (forwarded to the extension) to confirm the
    /// extension service worker is alive. This prevents a race where the service
    /// worker terminates before the first real test command arrives.
    ///
    /// Returns `None` if the commands fail (e.g. mediator not ready yet).
    async fn probe_browser_pid(socket: &std::path::Path) -> Option<u32> {
        let client = cli::client(socket);
        // Retry a few times since the mediator may still be initializing
        let mut pid = None;
        for _ in 0..5u8 {
            match client.browser_info().await {
                Ok(info) => {
                    pid = Some(info.pid);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
            }
        }
        // Send a command that actually reaches the extension to confirm
        // the service worker is alive and keep it warm for the test.
        if pid.is_some() {
            for _attempt in 0..3u8 {
                match client.list_windows().await {
                    Ok(_) => break,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        }
        pid
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
    #[expect(clippy::panic, reason = "test harness failure is unrecoverable")]
    let harness = match Harness::start(browser).await {
        Ok(h) => h,
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
