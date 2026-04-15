//! Native messaging host manifest installation for various browsers.

use std::path::{Path, PathBuf};

use crate::matchers::BrowserKind;

/// Errors that can occur during manifest installation.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// The user's home directory could not be determined.
    #[error("could not determine home directory for manifest installation")]
    NoBrowserHome,
    /// No mediator binary path was given and none could be found automatically.
    #[error("mediator binary not found next to this executable; use a specific path")]
    MediatorNotFound,
    /// Failed to determine the path of the current executable.
    #[error("failed to determine current executable path: {0}")]
    CurrentExe(std::io::Error),
    /// Failed to create the manifest directory.
    #[error("failed to create manifest directory: {0}")]
    CreateDir(std::io::Error),
    /// Failed to serialize the manifest JSON.
    #[error("failed to serialize manifest JSON: {0}")]
    Serialize(serde_json::Error),
    /// Failed to write the manifest file.
    #[error("failed to write manifest file: {0}")]
    WriteManifest(std::io::Error),
    /// `extension_id` is required for Chromium-family browsers but was not supplied.
    #[error(
        "Chromium-family browsers require an extension ID; \
         find the ID on chrome://extensions after loading the unpacked extension \
         (a 32-character lowercase letter string)"
    )]
    ChromiumExtensionIdRequired,
    /// Failed to create or write the Windows registry key.
    #[cfg(target_os = "windows")]
    #[error("failed to set Windows registry key: {0}")]
    Registry(std::io::Error),
}

/// The native messaging protocol family, which determines the JSON manifest format.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserFamily {
    /// Gecko-based browsers (Firefox and its forks); manifest uses `allowed_extensions`.
    Gecko,
    /// Chromium-based browsers (Chrome, Chromium, Brave, Edge, …); manifest uses `allowed_origins`.
    Chromium,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "manifest-specific methods live next to the manifest logic"
)]
impl BrowserKind {
    /// Return the native messaging protocol family used by this browser.
    #[must_use]
    pub const fn family(self) -> BrowserFamily {
        match self {
            Self::Firefox | Self::Librewolf | Self::Waterfox => BrowserFamily::Gecko,
            Self::Chrome | Self::Chromium | Self::Brave | Self::Edge => BrowserFamily::Chromium,
        }
    }

    /// Return the directory where this browser looks for native messaging host manifests.
    #[must_use]
    pub fn manifest_dir(self, base: &directories::BaseDirs) -> PathBuf {
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

        #[cfg(target_os = "windows")]
        {
            let _base = base;
            let appdata = std::env::var("APPDATA").unwrap_or_default();
            let localappdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
            match self {
                Self::Firefox | Self::Librewolf | Self::Waterfox => {
                    Path::new(&appdata).join("Mozilla/NativeMessagingHosts")
                }
                Self::Chrome => Path::new(&localappdata).join("Google/Chrome/NativeMessagingHosts"),
                Self::Chromium => Path::new(&localappdata).join("Chromium/NativeMessagingHosts"),
                Self::Brave => Path::new(&localappdata)
                    .join("BraveSoftware/Brave-Browser/NativeMessagingHosts"),
                Self::Edge => Path::new(&localappdata).join("Microsoft/Edge/NativeMessagingHosts"),
            }
        }
    }

    /// Return the Windows registry subkey path for this browser's native messaging host.
    #[cfg(target_os = "windows")]
    #[must_use]
    pub const fn windows_registry_key(self) -> &'static str {
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

/// JSON structure of a Gecko-family native messaging host manifest.
#[derive(Debug, serde::Serialize)]
struct GeckoManifest<'a> {
    /// The registered name of the native messaging host.
    name: &'a str,
    /// Human-readable description of the host.
    description: &'a str,
    /// Absolute path to the native messaging host binary.
    path: &'a Path,
    /// Transport type; always `"stdio"` for native messaging hosts.
    #[serde(rename = "type")]
    kind: &'a str,
    /// Extension IDs allowed to connect to this host.
    allowed_extensions: &'a [&'a str],
}

/// JSON structure of a Chromium-family native messaging host manifest.
#[derive(Debug, serde::Serialize)]
struct ChromiumManifest<'a> {
    /// The registered name of the native messaging host.
    name: &'a str,
    /// Human-readable description of the host.
    description: &'a str,
    /// Absolute path to the native messaging host binary.
    path: &'a Path,
    /// Transport type; always `"stdio"` for native messaging hosts.
    #[serde(rename = "type")]
    kind: &'a str,
    /// Extension origin URLs allowed to connect to this host.
    allowed_origins: &'a [String],
}

/// Result of a successful manifest installation.
#[derive(Debug, serde::Serialize)]
pub struct InstallManifestResult {
    /// Absolute path where the manifest was written.
    pub manifest_path: PathBuf,
    /// Absolute path to the mediator binary recorded in the manifest.
    pub mediator_path: PathBuf,
}

/// Install the native messaging host manifest for the given browser.
///
/// Returns the paths where the manifest and mediator binary were written.
///
/// # Errors
///
/// Returns an error if the home directory cannot be determined, the mediator binary cannot
/// be found automatically, the manifest directory cannot be created, the manifest file
/// cannot be written, or a Chromium-family browser is selected without `extension_id`.
pub fn install_manifest(
    browser: BrowserKind,
    mediator_path: Option<PathBuf>,
    extension_id: Option<String>,
) -> Result<InstallManifestResult, ManifestError> {
    let base = directories::BaseDirs::new().ok_or(ManifestError::NoBrowserHome)?;

    let mediator_path = match mediator_path {
        Some(p) => p,
        None => {
            let exe = std::env::current_exe().map_err(ManifestError::CurrentExe)?;
            let candidate = exe
                .parent()
                .map(|dir| dir.join("browser-controller-mediator"));
            match candidate {
                Some(p) if p.exists() => p,
                _ => return Err(ManifestError::MediatorNotFound),
            }
        }
    };

    let manifest_dir = browser.manifest_dir(&base);
    fs_err::create_dir_all(&manifest_dir).map_err(ManifestError::CreateDir)?;
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
            serde_json::to_string_pretty(&manifest).map_err(ManifestError::Serialize)?
        }
        BrowserFamily::Chromium => {
            let id = extension_id.ok_or(ManifestError::ChromiumExtensionIdRequired)?;
            let origin = format!("chrome-extension://{id}/");
            let manifest = ChromiumManifest {
                name: "browser_controller",
                description: "Browser Controller Mediator",
                path: &mediator_path,
                kind: "stdio",
                allowed_origins: &[origin],
            };
            serde_json::to_string_pretty(&manifest).map_err(ManifestError::Serialize)?
        }
    };

    fs_err::write(&manifest_path, json.as_bytes()).map_err(ManifestError::WriteManifest)?;

    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(browser.windows_registry_key())
            .map_err(|e| ManifestError::Registry(e.into()))?;
        key.set_value("", &manifest_path.to_string_lossy().as_ref())
            .map_err(|e| ManifestError::Registry(e.into()))?;
    }

    Ok(InstallManifestResult {
        manifest_path,
        mediator_path,
    })
}
