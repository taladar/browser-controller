//! WebDriver BiDi session helpers.
//!
//! Wraps the `webdriverbidi` crate to create sessions with appropriate
//! capabilities for Firefox and Chrome, and to install the browser-controller
//! extension.

use webdriverbidi::session::WebDriverBiDiSession;
use webdriverbidi::webdriver::capabilities::CapabilitiesRequest;

use crate::browser;

/// Error type for BiDi session operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The WebDriver BiDi session could not be created or started.
    #[error("BiDi session error: {0}")]
    Session(String),
    /// Extension installation failed.
    #[error("extension install failed: {0}")]
    ExtensionInstall(String),
}

/// Create and start a WebDriver BiDi session for the given browser.
///
/// The session connects to the WebDriver at `127.0.0.1:{port}` and configures
/// browser-specific capabilities (e.g. Firefox profile path).
///
/// # Errors
///
/// Returns an [`Error`] if the session cannot be created or started.
pub async fn create_session(
    browser: browser::Kind,
    driver_port: u16,
    profile_path: &std::path::Path,
) -> Result<WebDriverBiDiSession, Error> {
    let capabilities = build_capabilities(browser, profile_path);

    let mut session = WebDriverBiDiSession::new("127.0.0.1".to_owned(), driver_port, capabilities);

    session
        .start()
        .await
        .map_err(|e| Error::Session(format!("{e:?}")))?;

    Ok(session)
}

/// Install the browser-controller extension into the running browser session.
///
/// For Firefox, installs using the unpacked extension directory path.
///
/// # Errors
///
/// Returns an [`Error`] if extension installation fails.
pub async fn install_extension(
    session: &mut WebDriverBiDiSession,
    extension_path: &std::path::Path,
) -> Result<String, Error> {
    use webdriverbidi::model::web_extension::{ExtensionData, ExtensionPath, InstallParameters};

    let path_str = extension_path.to_string_lossy().into_owned();
    let params = InstallParameters::new(ExtensionData::ExtensionPath(ExtensionPath::new(path_str)));

    let result = session
        .web_extension_install(params)
        .await
        .map_err(|e| Error::ExtensionInstall(format!("{e:?}")))?;

    Ok(result.extension)
}

/// Build WebDriver capabilities for the given browser and profile.
fn build_capabilities(
    browser: browser::Kind,
    profile_path: &std::path::Path,
) -> CapabilitiesRequest {
    match browser {
        browser::Kind::Firefox => build_firefox_capabilities(profile_path),
        browser::Kind::Chrome => build_chrome_capabilities(profile_path),
    }
}

/// Build Firefox-specific capabilities.
///
/// Sets `moz:firefoxOptions` with a custom profile path so the test browser
/// does not interfere with the user's production Firefox.
fn build_firefox_capabilities(profile_path: &std::path::Path) -> CapabilitiesRequest {
    let mut caps = CapabilitiesRequest::default();

    let firefox_options = serde_json::json!({
        "args": ["-profile", profile_path.to_string_lossy()],
        "prefs": {
            "browser.shell.checkDefaultBrowser": false,
            "browser.startup.homepage_override.mstone": "ignore",
            "datareporting.policy.dataSubmissionEnabled": false,
            "toolkit.telemetry.reportingpolicy.firstRun": false,
            "extensions.autoDisableScopes": 0
        }
    });

    caps.add_extension("moz:firefoxOptions".to_owned(), firefox_options);

    caps
}

/// Build Chrome-specific capabilities.
fn build_chrome_capabilities(profile_path: &std::path::Path) -> CapabilitiesRequest {
    let mut caps = CapabilitiesRequest::default();

    let chrome_options = serde_json::json!({
        "args": [
            format!("--user-data-dir={}", profile_path.display()),
            "--no-first-run",
            "--disable-default-apps"
        ]
    });

    caps.add_extension("goog:chromeOptions".to_owned(), chrome_options);

    caps
}
