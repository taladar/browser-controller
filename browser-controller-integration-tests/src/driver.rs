//! WebDriver process lifecycle management.
//!
//! Launches geckodriver or chromedriver on a free port and waits for it to accept
//! connections before returning.

use std::net::TcpListener;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::browser;

/// Error type for driver operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The WebDriver binary was not found on `$PATH`.
    #[error("{0} not found on $PATH; install it to run integration tests")]
    NotFound(String),
    /// Failed to start the WebDriver process.
    #[error("failed to start {0}: {1}")]
    StartFailed(String, std::io::Error),
    /// The driver did not accept connections within the timeout.
    #[error("{0} did not become ready within {1:?}")]
    Timeout(String, Duration),
}

/// A running WebDriver process.
#[derive(Debug)]
pub struct Process {
    /// The child process handle.
    pub child: Child,
    /// The port the driver is listening on.
    pub port: u16,
    /// Which browser this driver is for.
    pub browser: browser::Kind,
}

impl Process {
    /// Start geckodriver or chromedriver on a free port.
    ///
    /// Blocks until the driver is accepting TCP connections or the timeout expires.
    ///
    /// # Errors
    ///
    /// Returns an error if the driver binary is not found, fails to start, or does
    /// not become ready within the timeout.
    pub async fn start(browser: browser::Kind) -> Result<Self, Error> {
        Self::start_with_binary(browser, None).await
    }

    /// Start a WebDriver process using a specific binary path.
    ///
    /// If `binary_path` is `None`, the default binary name from `$PATH` is used.
    ///
    /// # Errors
    ///
    /// Returns an error if the binary is not found, fails to start, or does
    /// not become ready within the timeout.
    pub async fn start_with_binary(
        browser: browser::Kind,
        binary_path: Option<&std::path::Path>,
    ) -> Result<Self, Error> {
        let binary: std::borrow::Cow<'_, str> = match binary_path {
            Some(p) => p.to_string_lossy(),
            None => {
                let name = browser.driver_binary_name();
                which::which(name).map_err(|_e| Error::NotFound(name.to_owned()))?;
                std::borrow::Cow::Borrowed(name)
            }
        };

        let port = find_free_port();

        let mut child = Command::new(binary.as_ref())
            .arg(format!("--port={port}"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::StartFailed(binary.to_string(), e))?;

        let timeout = Duration::from_secs(10);
        if wait_for_port(port, timeout).await == Err(()) {
            // Capture whatever the driver wrote before the timeout.
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let mut stdout_buf = String::new();
            let mut stderr_buf = String::new();
            if let Some(mut out) = stdout {
                drop(tokio::io::AsyncReadExt::read_to_string(&mut out, &mut stdout_buf).await);
            }
            if let Some(mut err) = stderr {
                drop(tokio::io::AsyncReadExt::read_to_string(&mut err, &mut stderr_buf).await);
            }
            return Err(Error::Timeout(
                format!("{binary} (port {port})\nstdout: {stdout_buf}\nstderr: {stderr_buf}"),
                timeout,
            ));
        }

        Ok(Self {
            child,
            port,
            browser,
        })
    }
}

/// Find a free TCP port by binding to port 0 and reading the assigned port.
///
/// # Panics
///
/// Panics if binding to port 0 fails (should not happen on a healthy system).
#[must_use]
pub fn find_free_port() -> u16 {
    #[expect(
        clippy::expect_used,
        reason = "binding to port 0 should always succeed"
    )]
    let listener = TcpListener::bind("127.0.0.1:0")
        .expect("binding to port 0 should always succeed on localhost");
    #[expect(
        clippy::expect_used,
        reason = "bound listener always has a local address"
    )]
    let port = listener
        .local_addr()
        .expect("bound listener should have a local address")
        .port();
    port
}

/// Poll until a TCP connection to `127.0.0.1:port` succeeds, or the timeout expires.
async fn wait_for_port(port: u16, timeout: Duration) -> Result<(), ()> {
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(tokio::time::Instant::now);
    let poll_interval = Duration::from_millis(100);

    loop {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(());
        }
        sleep(poll_interval).await;
    }
}
