//! Test browser profile creation.
//!
//! Creates a temporary browser profile directory so integration tests do not
//! affect the user's real browser state.

use std::path::PathBuf;

use crate::browser;

/// A temporary browser profile for testing.
///
/// The profile directory is automatically cleaned up when this value is dropped.
#[derive(Debug)]
#[expect(
    clippy::module_name_repetitions,
    reason = "clearer as profile::TestProfile externally"
)]
pub struct TestProfile {
    /// The temporary directory backing this profile. Held to prevent early cleanup.
    _temp_dir: tempfile::TempDir,
    /// The path to the profile directory.
    pub path: PathBuf,
}

impl TestProfile {
    /// Create a new temporary profile for the given browser.
    ///
    /// # Errors
    ///
    /// Returns an error if the temporary directory cannot be created.
    pub fn new(browser: browser::Kind) -> Result<Self, std::io::Error> {
        let temp_dir = tempfile::TempDir::with_prefix("browser-controller-test-")?;
        let path = temp_dir.path().to_owned();

        // Chrome requires developer mode to be enabled for unpacked extensions.
        // Pre-seed the Preferences file so the extension service worker starts.
        if browser == browser::Kind::Chrome {
            let default_dir = path.join("Default");
            fs_err::create_dir_all(&default_dir)?;
            fs_err::write(
                default_dir.join("Preferences"),
                r#"{"extensions":{"ui":{"developer_mode":true}}}"#,
            )?;
        }

        Ok(Self {
            _temp_dir: temp_dir,
            path,
        })
    }
}

/// Check that the native messaging manifest exists for the given browser.
///
/// Returns `None` if the manifest exists, or `Some(path)` if it is missing.
#[must_use]
pub fn check_manifest(browser: browser::Kind) -> Option<PathBuf> {
    let manifest_path = browser.manifest_path();
    if manifest_path.exists() {
        None
    } else {
        Some(manifest_path)
    }
}

/// Return the path to the unpacked extension directory within the workspace.
///
/// Returns `None` if the extension directory cannot be located.
#[must_use]
pub fn extension_dir() -> Option<PathBuf> {
    find_workspace_root().map(|root| root.join("extension"))
}

/// Locate the workspace root by searching upward from the current working
/// directory for a `Cargo.toml` with `[workspace]`.
#[must_use]
pub fn find_workspace_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists()
            && let Ok(contents) = fs_err::read_to_string(&cargo_toml)
            && contents.contains("[workspace]")
        {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Return the path to the built mediator binary.
///
/// Searches `target/debug/` and `target/release/` relative to the workspace root.
#[must_use]
pub fn mediator_binary() -> Option<PathBuf> {
    let root = find_workspace_root()?;
    for profile in &["debug", "release"] {
        let path = root
            .join("target")
            .join(profile)
            .join("browser-controller-mediator");
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Return the path to the built CLI binary.
///
/// Searches `target/debug/` and `target/release/` relative to the workspace root.
#[must_use]
pub fn cli_binary() -> Option<PathBuf> {
    let root = find_workspace_root()?;
    for profile in &["debug", "release"] {
        let path = root.join("target").join(profile).join("browser-controller");
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Return the path to the extension directory, verifying required files exist.
///
/// Returns `None` if background.js or manifest.json is missing.
#[must_use]
pub fn verified_extension_dir() -> Option<PathBuf> {
    let dir = extension_dir()?;
    let required = ["background.js", "manifest.json"];
    for file in &required {
        if !dir.join(file).exists() {
            return None;
        }
    }
    Some(dir)
}

/// A prepared extension directory ready for installation into a specific browser.
///
/// For Firefox, `path` points directly to the workspace `extension/` directory.
/// For Chrome, `path` points to a temporary copy with the Chrome manifest and a
/// `key` field for deterministic extension ID.
#[derive(Debug)]
pub struct PreparedExtension {
    /// Path to the unpacked extension directory.
    pub path: PathBuf,
    /// Temporary directory holding the Chrome copy (kept alive by this field).
    _temp_dir: Option<tempfile::TempDir>,
}

/// Prepare the extension directory for the given browser.
///
/// # Errors
///
/// Returns an error if the extension source directory is missing or if the
/// temporary copy for Chrome cannot be created.
pub fn prepared_extension_dir(browser: browser::Kind) -> Result<PreparedExtension, std::io::Error> {
    let source_dir = extension_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "extension directory not found",
        )
    })?;

    match browser {
        browser::Kind::Firefox => Ok(PreparedExtension {
            path: source_dir,
            _temp_dir: None,
        }),
        browser::Kind::Chrome => {
            let temp_dir = tempfile::TempDir::with_prefix("browser-controller-ext-chrome-")?;
            let dest = temp_dir.path();

            // Copy background.js
            fs_err::copy(source_dir.join("background.js"), dest.join("background.js"))?;
            // Copy Chrome offscreen document (needed for service worker keepalive)
            fs_err::copy(
                source_dir.join("offscreen.html"),
                dest.join("offscreen.html"),
            )?;
            fs_err::copy(source_dir.join("offscreen.js"), dest.join("offscreen.js"))?;
            // Copy manifest.chrome.json as manifest.json, injecting a fixed "key"
            // field so Chrome assigns a deterministic extension ID regardless of
            // the path the extension is loaded from.
            let manifest_content = fs_err::read_to_string(source_dir.join("manifest.chrome.json"))?;
            let mut manifest: serde_json::Value =
                serde_json::from_str(&manifest_content).map_err(|e| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("invalid manifest JSON: {e}"),
                    )
                })?;
            if let Some(obj) = manifest.as_object_mut() {
                obj.insert(
                    "key".to_owned(),
                    serde_json::Value::String(TEST_CHROME_EXTENSION_KEY.to_owned()),
                );
            }
            let manifest_out = serde_json::to_string_pretty(&manifest).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("manifest serialization failed: {e}"),
                )
            })?;
            fs_err::write(dest.join("manifest.json"), manifest_out)?;

            Ok(PreparedExtension {
                path: dest.to_owned(),
                _temp_dir: Some(temp_dir),
            })
        }
    }
}

/// RSA public key (DER, base64-encoded) used to give the Chrome test extension a
/// deterministic ID regardless of the path it is loaded from.
///
/// The corresponding extension ID is [`TEST_CHROME_EXTENSION_ID`].
const TEST_CHROME_EXTENSION_KEY: &str = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAudKxEwE1m/8nloKiVO3Jc/q3q0WS50wy9i9LoatVFf2RuvQuwSokmbZgPicSDRLreICozqNb38s1rxYEuDUsj21ciUKXhIv98aovOru6O5ZLAMM9+qAnFAL94DkbO/IF4t9NDg62aChnnPgScBUQPvbIQdaZxF265aoqkxRG7tDCu2rYdXH0p0LMV8kZdMNyEttqC0QWKZCUgt1iZ9GLNnuJK1TDtB1KOISeAMF39UxgK8a6yAyl1QNabztCnanK2mkBDzO+O5E3BYMgnLCp7JXiJovIm2ZSyQhaPZZBSHEeD7H5bLZi3i2/qY/n8Eq4v5vOomDWbqQ9nCBbFF8gTQIDAQAB";

/// The Chrome extension ID corresponding to [`TEST_CHROME_EXTENSION_KEY`].
///
/// Derived from the SHA-256 of the DER-encoded public key, with each nibble
/// mapped to `a`-`p`.
pub const TEST_CHROME_EXTENSION_ID: &str = "aicknojbcfnjicbieegnnmecfmeldbhd";

/// Write a native messaging manifest for the test extension into the
/// Chrome test profile directory.
///
/// Chrome for Testing looks for NMH manifests at
/// `<user-data-dir>/NativeMessagingHosts/<name>.json`, so the manifest
/// must be inside the test profile (not in `~/.config/`).
///
/// # Errors
///
/// Returns an error if the manifest cannot be written or the mediator
/// binary cannot be found.
pub fn write_chrome_test_nmh_manifest(profile_dir: &std::path::Path) -> Result<(), std::io::Error> {
    let mediator_path = mediator_binary()
        .or_else(|| which::which("browser-controller-mediator").ok())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "browser-controller-mediator binary not found",
            )
        })?;

    let nmh_dir = profile_dir.join("NativeMessagingHosts");
    fs_err::create_dir_all(&nmh_dir)?;

    let manifest_content = serde_json::to_string_pretty(&serde_json::json!({
        "name": "browser_controller",
        "description": "Browser Controller Mediator",
        "path": mediator_path.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [
            format!("chrome-extension://{TEST_CHROME_EXTENSION_ID}/")
        ]
    }))
    .map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("JSON serialization failed: {e}"),
        )
    })?;

    fs_err::write(nmh_dir.join("browser_controller.json"), manifest_content)?;
    Ok(())
}
