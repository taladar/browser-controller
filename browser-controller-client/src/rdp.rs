//! Firefox Remote Debugging Protocol helpers for loading temporary extensions.

use std::path::Path;

use crate::Error;

/// Load (or reload) a temporary extension via Firefox's Remote Debugging Protocol.
///
/// Connects to Firefox's debugger server, gets the root actor, finds the
/// addons actor, and calls `installTemporaryAddon` with the given path.
///
/// # Errors
///
/// Returns an error if the connection fails, the protocol exchange fails,
/// or the addon installation is rejected.
pub async fn load_temporary_extension(path: &Path, port: u16) -> Result<String, Error> {
    use tokio::net::TcpStream;

    let canonical = fs_err::canonicalize(path)?;

    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.map_err(|e| {
        Error::CommandFailed(format!(
            "cannot connect to Firefox debugger on port {port}: {e}. \
                 Start Firefox with --start-debugger-server {port} or enable \
                 devtools.debugger.remote-enabled in about:config"
        ))
    })?;

    // Read the initial server hello
    let hello = rdp_read(&mut stream).await.map_err(|e| {
        Error::CommandFailed(format!(
            "failed to read RDP hello from Firefox on port {port}: {e}"
        ))
    })?;
    tracing::debug!(hello = %hello, "RDP hello");

    // Get the root actor to find the addons actor
    let root_response = rdp_call(&mut stream, r#"{"type":"getRoot","to":"root"}"#)
        .await
        .map_err(|e| Error::CommandFailed(format!("RDP getRoot failed: {e}")))?;
    tracing::debug!(root = %root_response, "RDP getRoot");

    // Parse the addonsActor name from the root response
    let root: serde_json::Value = serde_json::from_str(&root_response).map_err(|e| {
        Error::CommandFailed(format!(
            "failed to parse RDP getRoot response as JSON: {e}; response was: {root_response}"
        ))
    })?;

    // Check for error in root response
    if let Some(err) = root.get("error") {
        let message = root
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(Error::CommandFailed(format!(
            "RDP getRoot failed: {err}: {message}"
        )));
    }

    let addons_actor = root
        .get("addonsActor")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            Error::CommandFailed(format!(
                "Firefox RDP response does not contain addonsActor \
                 (ensure devtools.debugger.remote-enabled and devtools.chrome.enabled \
                 are set to true in about:config); response was: {root_response}"
            ))
        })?;

    // Call installTemporaryAddon
    let install_msg = serde_json::json!({
        "type": "installTemporaryAddon",
        "to": addons_actor,
        "addonPath": canonical.to_string_lossy(),
    });
    let install_response = rdp_call(&mut stream, &install_msg.to_string())
        .await
        .map_err(|e| Error::CommandFailed(format!("RDP installTemporaryAddon call failed: {e}")))?;
    tracing::debug!(install = %install_response, "RDP installTemporaryAddon");

    // Check for errors in the response
    let install: serde_json::Value = serde_json::from_str(&install_response).map_err(|e| {
        Error::CommandFailed(format!(
            "failed to parse installTemporaryAddon response as JSON: {e}; \
                 response was: {install_response}"
        ))
    })?;
    if let Some(err) = install.get("error") {
        let message = install
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(Error::CommandFailed(format!(
            "installTemporaryAddon failed: {err}: {message}"
        )));
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
async fn rdp_read(stream: &mut tokio::net::TcpStream) -> Result<String, Error> {
    use tokio::io::AsyncReadExt as _;

    // Read the length prefix (digits followed by ':')
    let mut length_buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream.read_exact(&mut byte).await?;
        if byte[0] == b':' {
            break;
        }
        length_buf.push(byte[0]);
    }

    let length_str = String::from_utf8_lossy(&length_buf);
    let length: usize = length_str
        .parse()
        .map_err(|_e| Error::CommandFailed(format!("invalid RDP length prefix: {length_str}")))?;

    // Read exactly `length` bytes of JSON
    let mut json_buf = vec![0u8; length];
    stream.read_exact(&mut json_buf).await?;

    String::from_utf8(json_buf)
        .map_err(|e| Error::CommandFailed(format!("invalid UTF-8 in RDP response: {e}")))
}

/// Send an RDP message and read the response.
async fn rdp_call(stream: &mut tokio::net::TcpStream, message: &str) -> Result<String, Error> {
    use tokio::io::AsyncWriteExt as _;

    let payload = format!("{}:{message}", message.len());
    stream.write_all(payload.as_bytes()).await?;
    stream.flush().await?;

    rdp_read(stream).await
}
