//! Chrome for Testing (CfT) download and caching.
//!
//! Downloads a Chrome for Testing build and its matching chromedriver into
//! `<workspace>/target/chrome-for-testing/` so integration tests can load
//! unpacked extensions via `--load-extension` (which is disabled in release
//! builds of Google Chrome but enabled in CfT).

use std::path::{Path, PathBuf};

use crate::profile;

/// The CfT JSON endpoint listing the latest known-good versions with downloads.
const CFT_VERSIONS_URL: &str = "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

/// The channel to download (Stable tracks the current Chrome stable release).
const CFT_CHANNEL: &str = "Stable";

/// The platform identifier for Linux x86-64.
const CFT_PLATFORM: &str = "linux64";

/// Error type for CfT operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(String),
    /// JSON parsing failed.
    #[error("JSON error: {0}")]
    Json(String),
    /// No download URL found for the requested channel/platform.
    #[error("no CfT download found for {0}/{1}")]
    NoDownload(String, String),
    /// I/O error during download or extraction.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The workspace root could not be found.
    #[error("workspace root not found")]
    NoWorkspace,
    /// The zip archive could not be extracted.
    #[error("zip extraction failed: {0}")]
    Zip(String),
}

/// Paths to the cached Chrome for Testing binaries.
#[derive(Debug, Clone)]
pub struct CftPaths {
    /// Path to the `chrome` binary.
    pub chrome: PathBuf,
    /// Path to the `chromedriver` binary.
    pub chromedriver: PathBuf,
}

/// Return the directory where CfT binaries are cached.
fn cache_dir() -> Result<PathBuf, Error> {
    let root = profile::find_workspace_root().ok_or(Error::NoWorkspace)?;
    Ok(root.join("target").join("chrome-for-testing"))
}

/// Ensure Chrome for Testing and its matching chromedriver are downloaded.
///
/// Returns paths to both binaries. Downloads are cached in
/// `<workspace>/target/chrome-for-testing/` and reused across runs.
///
/// # Errors
///
/// Returns an error if the download or extraction fails.
pub async fn ensure_installed() -> Result<CftPaths, Error> {
    let dir = cache_dir()?;
    let chrome_path = dir.join("chrome-linux64").join("chrome");
    let chromedriver_path = dir.join("chromedriver-linux64").join("chromedriver");

    if chrome_path.exists() && chromedriver_path.exists() {
        return Ok(CftPaths {
            chrome: chrome_path,
            chromedriver: chromedriver_path,
        });
    }

    fs_err::create_dir_all(&dir)?;

    // Fetch the version manifest.
    tracing::info!("[chrome-for-testing] Fetching version manifest...");
    let manifest = fetch_json(CFT_VERSIONS_URL).await?;

    let channel = manifest
        .pointer(&format!("/channels/{CFT_CHANNEL}"))
        .ok_or_else(|| Error::Json(format!("no channel {CFT_CHANNEL} in manifest")))?;

    let version = channel
        .pointer("/version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Download Chrome.
    if !chrome_path.exists() {
        let chrome_url = find_download_url(channel, "chrome")?;
        tracing::info!("[chrome-for-testing] Downloading Chrome {version} from {chrome_url}");
        download_and_extract(&chrome_url, &dir).await?;
    }

    // Download chromedriver.
    if !chromedriver_path.exists() {
        let driver_url = find_download_url(channel, "chromedriver")?;
        tracing::info!("[chrome-for-testing] Downloading chromedriver {version} from {driver_url}");
        download_and_extract(&driver_url, &dir).await?;
    }

    // Make all extracted files executable (Chrome ships helper binaries like
    // chrome_crashpad_handler that also need the execute bit).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        for subdir in ["chrome-linux64", "chromedriver-linux64"] {
            let subdir_path = dir.join(subdir);
            if let Ok(entries) = fs_err::read_dir(&subdir_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file()
                        && let Ok(meta) = fs_err::metadata(&path)
                    {
                        let mut perms = meta.permissions();
                        if perms.mode() & 0o111 == 0 {
                            perms.set_mode(0o755);
                            drop(fs_err::set_permissions(&path, perms));
                        }
                    }
                }
            }
        }
    }

    if !chrome_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Chrome binary not found at {}", chrome_path.display()),
        )));
    }
    if !chromedriver_path.exists() {
        return Err(Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "chromedriver binary not found at {}",
                chromedriver_path.display()
            ),
        )));
    }

    tracing::info!("[chrome-for-testing] Chrome {version} ready.");

    Ok(CftPaths {
        chrome: chrome_path,
        chromedriver: chromedriver_path,
    })
}

/// Find the download URL for a given product (chrome/chromedriver) in a channel.
fn find_download_url(channel: &serde_json::Value, product: &str) -> Result<String, Error> {
    let downloads = channel
        .pointer(&format!("/downloads/{product}"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::NoDownload(product.to_owned(), CFT_CHANNEL.to_owned()))?;

    downloads
        .iter()
        .find(|d| d.get("platform").and_then(|v| v.as_str()) == Some(CFT_PLATFORM))
        .and_then(|d| d.get("url").and_then(|v| v.as_str()))
        .map(str::to_owned)
        .ok_or_else(|| Error::NoDownload(product.to_owned(), CFT_PLATFORM.to_owned()))
}

/// Fetch a JSON document from a URL.
async fn fetch_json(url: &str) -> Result<serde_json::Value, Error> {
    let bytes = fetch_bytes(url).await?;
    serde_json::from_slice(&bytes).map_err(|e| Error::Json(format!("{e}")))
}

/// Fetch raw bytes from a URL using a simple HTTP GET.
async fn fetch_bytes(url: &str) -> Result<Vec<u8>, Error> {
    // Use reqwest if available, otherwise fall back to curl.
    let output = tokio::process::Command::new("curl")
        .args(["-fsSL", url])
        .output()
        .await
        .map_err(|e| Error::Http(format!("curl failed to start: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Http(format!(
            "curl failed with {}: {stderr}",
            output.status
        )));
    }

    Ok(output.stdout)
}

/// Download a zip file and extract it into `dest_dir`.
async fn download_and_extract(url: &str, dest_dir: &Path) -> Result<(), Error> {
    let zip_bytes = fetch_bytes(url).await?;

    // Extract using the zip crate.
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| Error::Zip(format!("invalid zip: {e}")))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| Error::Zip(format!("zip entry {i}: {e}")))?;

        let Some(enclosed_name) = file.enclosed_name() else {
            continue;
        };
        let out_path = dest_dir.join(enclosed_name);

        if file.is_dir() {
            fs_err::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs_err::create_dir_all(parent)?;
            }
            let mut out_file = fs_err::File::create(&out_path)?;
            std::io::copy(&mut file, &mut out_file)?;
        }
    }

    Ok(())
}
