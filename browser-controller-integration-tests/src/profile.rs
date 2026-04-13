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
    pub fn new(_browser: browser::Kind) -> Result<Self, std::io::Error> {
        let temp_dir = tempfile::TempDir::with_prefix("browser-controller-test-")?;
        let path = temp_dir.path().to_owned();
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
fn find_workspace_root() -> Option<PathBuf> {
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
