//! Firefox Remote Debugging Protocol helpers for loading temporary extensions.

use std::path::Path;

/// Errors that can occur during Firefox RDP communication.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum RdpError {
    /// Failed to canonicalize the extension path.
    #[error("failed to canonicalize extension path: {0}")]
    CanonicalizePath(std::io::Error),
    /// Failed to connect to the Firefox debugger port.
    #[error(
        "cannot connect to Firefox debugger on port {port}: {source}; \
         start Firefox with --start-debugger-server {port} or enable \
         devtools.debugger.remote-enabled in about:config"
    )]
    Connect {
        /// The port that was attempted.
        port: u16,
        /// The underlying connection error.
        source: std::io::Error,
    },
    /// Failed to read the RDP length prefix bytes.
    #[error("failed to read RDP length prefix: {0}")]
    ReadLengthPrefix(std::io::Error),
    /// The RDP length prefix was not a valid number.
    #[error("invalid RDP length prefix: {raw:?}")]
    InvalidLengthPrefix {
        /// The raw bytes received as the length prefix.
        raw: String,
    },
    /// Failed to read the RDP message body.
    #[error("failed to read RDP message body: {0}")]
    ReadBody(std::io::Error),
    /// The RDP message body was not valid UTF-8.
    #[error("invalid UTF-8 in RDP response: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    /// Failed to write an RDP message.
    #[error("failed to write RDP message: {0}")]
    Write(std::io::Error),
    /// Failed to flush the RDP stream.
    #[error("failed to flush RDP stream: {0}")]
    Flush(std::io::Error),
    /// Failed to parse an RDP response as JSON.
    #[error("failed to parse RDP response as JSON: {source}; raw response: {raw}")]
    ParseJson {
        /// The JSON parse error.
        source: serde_json::Error,
        /// The raw response string that failed to parse.
        raw: String,
    },
    /// The getRoot RDP call returned an error.
    #[error("RDP getRoot returned error: {response}")]
    GetRootError {
        /// The full JSON response containing the error.
        response: serde_json::Value,
    },
    /// The getRoot response did not contain an addonsActor field.
    #[error(
        "Firefox RDP response does not contain addonsActor \
         (ensure devtools.debugger.remote-enabled and devtools.chrome.enabled \
         are set to true in about:config); response was: {response}"
    )]
    MissingAddonsActor {
        /// The full JSON response that was missing the field.
        response: serde_json::Value,
    },
    /// The installTemporaryAddon RDP call returned an error.
    #[error("installTemporaryAddon failed: {response}")]
    InstallAddonError {
        /// The full JSON response containing the error.
        response: serde_json::Value,
    },
}

/// Load (or reload) a temporary extension via Firefox's Remote Debugging Protocol.
///
/// Connects to Firefox's debugger server, gets the root actor, finds the
/// addons actor, and calls `installTemporaryAddon` with the given path.
///
/// # Errors
///
/// Returns an [`RdpError`] if the connection fails, the protocol exchange
/// fails, or the addon installation is rejected.
pub async fn load_temporary_extension(path: &Path, port: u16) -> Result<String, RdpError> {
    use tokio::net::TcpStream;

    let canonical = fs_err::canonicalize(path).map_err(RdpError::CanonicalizePath)?;

    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|source| RdpError::Connect { port, source })?;

    // Read the initial server hello
    let hello = rdp_read(&mut stream).await?;
    tracing::debug!(hello = %hello, "RDP hello");

    // Get the root actor to find the addons actor
    let root_response = rdp_call(&mut stream, r#"{"type":"getRoot","to":"root"}"#).await?;
    tracing::debug!(root = %root_response, "RDP getRoot");

    // Parse the addonsActor name from the root response
    let root: serde_json::Value =
        serde_json::from_str(&root_response).map_err(|source| RdpError::ParseJson {
            source,
            raw: root_response.clone(),
        })?;

    // Check for error in root response
    if root.get("error").is_some() {
        return Err(RdpError::GetRootError { response: root });
    }

    let addons_actor = root
        .get("addonsActor")
        .and_then(|v| v.as_str())
        .ok_or_else(|| RdpError::MissingAddonsActor {
            response: root.clone(),
        })?;

    // Call installTemporaryAddon
    let install_msg = serde_json::json!({
        "type": "installTemporaryAddon",
        "to": addons_actor,
        "addonPath": canonical.to_string_lossy(),
    });
    let install_response = rdp_call(&mut stream, &install_msg.to_string()).await?;
    tracing::debug!(install = %install_response, "RDP installTemporaryAddon");

    // Check for errors in the response
    let install: serde_json::Value =
        serde_json::from_str(&install_response).map_err(|source| RdpError::ParseJson {
            source,
            raw: install_response,
        })?;
    if install.get("error").is_some() {
        return Err(RdpError::InstallAddonError { response: install });
    }

    let addon_id = install
        .get("addon")
        .and_then(|a| a.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>");

    Ok(addon_id.to_owned())
}

/// Read a single RDP message from the stream.
///
/// The Firefox RDP protocol prefixes each message with its byte length
/// followed by a colon, e.g. `30:{"type":"greeting","from":"root"}`.
async fn rdp_read(stream: &mut tokio::net::TcpStream) -> Result<String, RdpError> {
    use tokio::io::AsyncReadExt as _;

    // Read the length prefix (digits followed by ':')
    let mut length_buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream
            .read_exact(&mut byte)
            .await
            .map_err(RdpError::ReadLengthPrefix)?;
        if byte[0] == b':' {
            break;
        }
        length_buf.push(byte[0]);
    }

    let length_str = String::from_utf8_lossy(&length_buf);
    let length: usize = length_str
        .parse()
        .map_err(|_e| RdpError::InvalidLengthPrefix {
            raw: length_str.into_owned(),
        })?;

    // Read exactly `length` bytes of JSON
    let mut json_buf = vec![0u8; length];
    stream
        .read_exact(&mut json_buf)
        .await
        .map_err(RdpError::ReadBody)?;

    Ok(String::from_utf8(json_buf)?)
}

/// Send an RDP message and read the response.
async fn rdp_call(stream: &mut tokio::net::TcpStream, message: &str) -> Result<String, RdpError> {
    use tokio::io::AsyncWriteExt as _;

    let payload = format!("{}:{message}", message.len());
    stream
        .write_all(payload.as_bytes())
        .await
        .map_err(RdpError::Write)?;
    stream.flush().await.map_err(RdpError::Flush)?;

    rdp_read(stream).await
}
