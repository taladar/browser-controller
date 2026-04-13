//! Browser kind definitions and browser-specific configuration.

use std::path::PathBuf;

/// Which browser to test against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Mozilla Firefox (requires geckodriver).
    Firefox,
    /// Google Chrome / Chromium (requires chromedriver).
    Chrome,
}

impl Kind {
    /// Returns the name of the WebDriver binary for this browser.
    #[must_use]
    pub const fn driver_binary_name(self) -> &'static str {
        match self {
            Self::Firefox => "geckodriver",
            Self::Chrome => "chromedriver",
        }
    }

    /// Returns the path to the native messaging manifest for this browser.
    #[must_use]
    pub fn manifest_path(self) -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        let home = std::path::Path::new(&home);
        match self {
            Self::Firefox => home.join(".mozilla/native-messaging-hosts/browser_controller.json"),
            Self::Chrome => {
                home.join(".config/google-chrome/NativeMessagingHosts/browser_controller.json")
            }
        }
    }

    /// Returns the extension ID used in the native messaging manifest.
    #[must_use]
    pub const fn extension_id(self) -> &'static str {
        match self {
            Self::Firefox => "browser-controller@taladar.net",
            Self::Chrome => "", // Chrome assigns ID based on extension path
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Firefox => write!(f, "Firefox"),
            Self::Chrome => write!(f, "Chrome"),
        }
    }
}
