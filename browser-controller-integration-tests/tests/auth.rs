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

use browser_controller_integration_tests::Harness;
use browser_controller_integration_tests::browser;
use browser_controller_integration_tests::harness;
use browser_controller_integration_tests::profile;
use browser_controller_integration_tests::test_server;
use browser_controller_types::{CliCommand, CliResult};

/// Helper to get the first window ID.
#[expect(clippy::panic, reason = "test helper panics on unexpected variants")]
async fn first_window_id(h: &Harness) -> u32 {
    let result = h
        .send_command(CliCommand::ListWindows)
        .await
        .expect("ListWindows should succeed");
    match result {
        CliResult::Windows { windows } => {
            assert!(!windows.is_empty(), "need at least 1 window");
            windows.first().expect("just asserted non-empty").id
        }
        other => panic!("expected Windows, got {other:?}"),
    }
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
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn auth_credentials_body(h: &Harness) {
    let server = test_server::Server::start_with_auth("testuser", "testpass");
    let window_id = first_window_id(h).await;

    // The auth URL — the server requires Basic Auth on this endpoint
    let auth_url = server.auth_url();

    // Open tab with username/password — the extension provides credentials
    // via onAuthRequired asynchronously; OpenTab returns immediately.
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some(auth_url.clone()),
            username: Some("testuser".to_owned()),
            password: Some("testpass".to_owned()),
            background: true,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab with credentials should succeed");

    let tab_id = match open_result {
        CliResult::Tab(details) => details.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    // Wait for the auth exchange and page load to complete
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // 1. Verify the page loaded successfully via ListTabs
    let list_result = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");

    match &list_result {
        CliResult::Tabs { tabs } => {
            let tab = tabs
                .iter()
                .find(|t| t.id == tab_id)
                .expect("opened tab should exist in ListTabs");

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
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

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

    // Cleanup
    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("CloseTab should succeed");
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
