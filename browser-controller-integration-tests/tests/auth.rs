//! Tests for the HTTP authentication credential injection in `OpenTab`.
//!
//! Verifies that when a tab is opened with `username` and `password`, the
//! credentials are provided to the server via `onAuthRequired` and do not
//! appear in the tab's URL in any API response.

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
/// Opens a tab with username/password credentials and verifies:
/// 1. The `OpenTab` response URL does not contain credentials
/// 2. The `ListTabs` protocol response URL does not contain credentials
/// 3. The CLI `tabs list` JSON output does not contain credentials
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn auth_credentials_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open tab with username/password — the extension will strip credentials
    // from the URL and provide them via onAuthRequired when the server
    // responds with a 401 challenge.
    let url = "https://testuser:testpass@www.google.com/";
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some(url.to_owned()),
            username: Some("testuser".to_owned()),
            password: Some("testpass".to_owned()),
            background: true,
        })
        .await
        .expect("OpenTab with credentials should succeed");

    let (tab_id, returned_url) = match open_result {
        CliResult::Tab(details) => (details.id, details.url),
        other => panic!("expected Tab, got {other:?}"),
    };

    // 1. Verify the OpenTab response URL does not contain credentials
    assert!(
        !returned_url.contains("testuser"),
        "OpenTab response URL should not contain username, got {returned_url}",
    );
    assert!(
        !returned_url.contains("testpass"),
        "OpenTab response URL should not contain password, got {returned_url}",
    );
    assert!(
        returned_url.starts_with("https://www.google.com"),
        "OpenTab response URL should start with https://www.google.com, got {returned_url}",
    );

    // 2. Verify via ListTabs protocol response
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
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // 3. Verify via CLI binary `tabs list` output
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
