//! Minimal HTTP test server for integration tests.
//!
//! Provides plain pages and a Basic Auth-protected endpoint so tests do not
//! depend on external websites.

use std::net::TcpListener;

use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

/// A running test HTTP server.
#[derive(Debug)]
pub struct Server {
    /// The port the server is listening on.
    pub port: u16,
    /// Handle to abort the server task on drop.
    _abort_handle: tokio::task::AbortHandle,
}

impl Server {
    /// Start a plain HTTP server (no authentication required).
    ///
    /// Serves simple HTML pages at `/` and `/page2`.
    ///
    /// # Panics
    ///
    /// Panics if binding to a free port fails.
    #[must_use]
    pub fn start_plain() -> Self {
        let port = find_free_port();
        let handle = tokio::spawn(run_server(port, None));
        Self {
            port,
            _abort_handle: handle.abort_handle(),
        }
    }

    /// Start an HTTP server that requires Basic Auth on `/auth`.
    ///
    /// - `/` and `/page2` are served without authentication.
    /// - `/auth` requires HTTP Basic Auth with the given username and password.
    ///
    /// # Panics
    ///
    /// Panics if binding to a free port fails.
    #[must_use]
    pub fn start_with_auth(username: &str, password: &str) -> Self {
        let port = find_free_port();
        let creds = format!("{username}:{password}");
        let handle = tokio::spawn(run_server(port, Some(creds)));
        Self {
            port,
            _abort_handle: handle.abort_handle(),
        }
    }

    /// Return the base URL for this server (e.g. `http://127.0.0.1:12345`).
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Return the URL for the auth-protected endpoint.
    #[must_use]
    pub fn auth_url(&self) -> String {
        format!("http://127.0.0.1:{}/auth", self.port)
    }

    /// Return the URL for the second page (for navigation history tests).
    #[must_use]
    pub fn page2_url(&self) -> String {
        format!("http://127.0.0.1:{}/page2", self.port)
    }
}

/// Find a free TCP port by binding to port 0.
fn find_free_port() -> u16 {
    #[expect(
        clippy::expect_used,
        reason = "binding to port 0 should always succeed"
    )]
    let listener =
        TcpListener::bind("127.0.0.1:0").expect("binding to port 0 should always succeed");
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

/// Main server loop.
async fn run_server(port: u16, auth_credentials: Option<String>) {
    #[expect(clippy::expect_used, reason = "test server must bind successfully")]
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("test server should bind to port");

    loop {
        if let Ok((stream, _addr)) = listener.accept().await {
            let creds = auth_credentials.clone();
            tokio::spawn(async move {
                drop(handle_connection(stream, creds).await);
            });
        }
    }
}

/// Handle a single HTTP connection.
async fn handle_connection(
    stream: tokio::net::TcpStream,
    auth_credentials: Option<String>,
) -> Result<(), std::io::Error> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Read the request line
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    // Parse method and path
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();

    // Read all headers (until empty line)
    let mut authorization = None;
    loop {
        let mut header_line = String::new();
        reader.read_line(&mut header_line).await?;
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Authorization: ") {
            authorization = Some(value.to_owned());
        }
    }

    // Route the request
    let response = match path.as_str() {
        "/" => ok_response("Test Page", "This is the test page."),
        "/page2" => ok_response("Test Page 2", "This is the second test page."),
        "/auth" => handle_auth(auth_credentials.as_deref(), authorization.as_deref()),
        _ => not_found_response(),
    };

    write_half.write_all(response.as_bytes()).await?;
    write_half.flush().await?;

    Ok(())
}

/// Build a 200 OK HTML response.
fn ok_response(title: &str, body: &str) -> String {
    let html = format!(
        "<!DOCTYPE html><html><head><title>{title}</title></head>\
         <body><h1>{body}</h1></body></html>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\
         Cache-Control: max-age=3600\r\nConnection: close\r\n\r\n{html}",
        html.len(),
    )
}

/// Build a 404 Not Found response.
fn not_found_response() -> String {
    let body = "Not Found";
    format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len(),
    )
}

/// Handle the `/auth` endpoint with Basic Auth checking.
fn handle_auth(expected_credentials: Option<&str>, authorization: Option<&str>) -> String {
    let Some(expected) = expected_credentials else {
        // No auth configured on this server; serve the page freely
        return ok_response("Auth Page", "Authenticated successfully.");
    };

    // Check the Authorization header
    if let Some(auth_header) = authorization
        && let Some(encoded) = auth_header.strip_prefix("Basic ")
        && let Ok(decoded_bytes) = base64_decode(encoded.trim())
        && let Ok(decoded) = String::from_utf8(decoded_bytes)
        && decoded == expected
    {
        return ok_response("Auth Page", "Authenticated successfully.");
    }

    // Return 401 with WWW-Authenticate header
    let body = "Unauthorized";
    format!(
        "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"test\"\r\n\
         Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
}

/// Minimal base64 decoder (avoids adding a dependency for tests).
fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn decode_char(c: u8) -> Result<u8, ()> {
        TABLE
            .iter()
            .position(|&b| b == c)
            .and_then(|p| u8::try_from(p).ok())
            .ok_or(())
    }

    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut output = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let remaining = bytes.len().saturating_sub(i);
        let b0 = decode_char(*bytes.get(i).ok_or(())?)?;
        let b1 = if remaining > 1 {
            decode_char(*bytes.get(i.saturating_add(1)).ok_or(())?)?
        } else {
            0
        };
        let b2 = if remaining > 2 {
            decode_char(*bytes.get(i.saturating_add(2)).ok_or(())?)?
        } else {
            0
        };
        let b3 = if remaining > 3 {
            decode_char(*bytes.get(i.saturating_add(3)).ok_or(())?)?
        } else {
            0
        };

        output.push((b0 << 2) | (b1 >> 4));
        if remaining > 2 {
            output.push((b1 << 4) | (b2 >> 2));
        }
        if remaining > 3 {
            output.push((b2 << 6) | b3);
        }

        i = i.saturating_add(4);
    }
    Ok(output)
}
