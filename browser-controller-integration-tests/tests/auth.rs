//! Tests for the HTTP authentication credential injection in `OpenTab`.
//!
//! Uses a local test HTTP server that requires Basic Auth, so the
//! `onAuthRequired` flow is exercised under realistic conditions.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are inherently outside #[cfg(test)]"
)]
#![expect(
    clippy::expect_used,
    reason = "panicking on unexpected failure is acceptable in tests"
)]

use browser_controller_client::OpenTabParamsBuilder;
use browser_controller_integration_tests::Harness;
use browser_controller_integration_tests::browser;
use browser_controller_integration_tests::harness;
use browser_controller_integration_tests::profile;
use browser_controller_integration_tests::test_server;
use browser_controller_types::WindowId;

/// Helper to get the first window ID.
async fn first_window_id(h: &Harness) -> WindowId {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    windows.first().expect("just asserted non-empty").id
}

/// Run the CLI binary with the given arguments and return stdout.
async fn run_cli(h: &Harness, args: &[&str]) -> String {
    let cli_bin = profile::cli_binary().expect("CLI binary should be built");
    let pid = h
        .browser_pid
        .expect("browser PID should be known for CLI tests");

    let mut cmd = tokio::process::Command::new(&cli_bin);
    cmd.arg("-o").arg("json");
    cmd.arg("-i").arg(pid.to_string());
    cmd.args(args);

    let output = cmd.output().await.expect("CLI process should start");

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "CLI command {args:?} failed with status {}.\nstdout: {stdout}\nstderr: {stderr}",
        output.status,
    );

    stdout
}

/// Shared auth credential injection test body.
///
/// Starts a local HTTP server that requires Basic Auth on `/auth`,
/// opens a tab with username/password credentials, waits for the auth
/// exchange to complete, and verifies:
/// 1. The page was successfully authenticated (tab title confirms it)
/// 2. The tab URL does not contain credentials
/// 3. The CLI `tabs list` output does not contain credentials
async fn auth_credentials_body(h: &Harness) {
    let server = test_server::Server::start_with_auth("testuser", "testpass");
    let window_id = first_window_id(h).await;

    // The auth URL — the server requires Basic Auth on this endpoint
    let auth_url = server.auth_url();

    // Open tab with username/password — the extension provides credentials
    // via onAuthRequired asynchronously; OpenTab returns immediately.
    let params = OpenTabParamsBuilder::default()
        .window_id(window_id)
        .url(auth_url.clone())
        .username("testuser")
        .password("testpass")
        .background(true)
        .build()
        .expect("build OpenTabParams");
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab with credentials should succeed");
    let tab_id = tab.id;

    // Wait for the auth exchange and page load to complete.
    // Chrome may need more time than Firefox for the 401 → credentials → retry cycle.
    let mut tab = None;
    for _ in 0..10u8 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let tabs = h
            .client()
            .list_tabs(window_id)
            .await
            .expect("ListTabs should succeed");
        let found = tabs.into_iter().find(|t| t.id == tab_id);
        if let Some(ref t) = found
            && t.title.contains("Auth Page")
        {
            tab = found;
            break;
        }
        tab = found;
    }
    let tab = tab.expect("opened tab should exist in ListTabs");

    // URL should not contain credentials
    assert!(
        !tab.url.contains("testuser"),
        "ListTabs URL should not contain username, got {}",
        tab.url,
    );
    assert!(
        !tab.url.contains("testpass"),
        "ListTabs URL should not contain password, got {}",
        tab.url,
    );

    // The title should be "Auth Page" (from our test server's response),
    // confirming successful authentication
    assert!(
        tab.title.contains("Auth Page"),
        "tab title should contain 'Auth Page' (indicating successful auth), got {:?}",
        tab.title,
    );

    // 2. Verify via CLI binary `tabs list` output
    let wid = window_id.to_string();
    let stdout = run_cli(h, &["tabs", "list", "--window-id", &wid]).await;
    assert!(
        !stdout.contains("testuser"),
        "CLI tabs list output should not contain username",
    );
    assert!(
        !stdout.contains("testpass"),
        "CLI tabs list output should not contain password",
    );
}

#[tokio::test]
async fn auth_credentials_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(auth_credentials_body(h))
    })
    .await;
}

#[tokio::test]
async fn auth_credentials_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(auth_credentials_body(h))
    })
    .await;
}
