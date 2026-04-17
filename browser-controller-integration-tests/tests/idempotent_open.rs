//! Tests for idempotent open operations:
//! - `windows open --title-prefix X --if-title-prefix-does-not-exist`
//! - `tabs open --url X --if-url-does-not-exist`

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
use browser_controller_types::WindowId;

/// Run the CLI binary with the given arguments and return (stdout, success).
async fn run_cli_raw(h: &Harness, args: &[&str]) -> (String, bool) {
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
    (stdout, output.status.success())
}

/// Run the CLI binary, asserting success.
async fn run_cli(h: &Harness, args: &[&str]) -> String {
    let (stdout, success) = run_cli_raw(h, args).await;
    assert!(success, "CLI command {args:?} failed");
    stdout
}

/// Count windows via the protocol.
async fn window_count(h: &Harness) -> usize {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    windows.len()
}

/// Get the first window ID.
async fn first_window_id(h: &Harness) -> WindowId {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    windows.first().expect("just asserted non-empty").id
}

/// Count tabs in a window via the protocol.
async fn tab_count(h: &Harness, window_id: WindowId) -> usize {
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    tabs.len()
}

// ---------------------------------------------------------------------------
// --if-title-prefix-does-not-exist (Firefox only)
// ---------------------------------------------------------------------------

/// Test that `windows open --title-prefix X --if-title-prefix-does-not-exist`
/// opens a window the first time, but is a no-op when called again.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "window count arithmetic in test assertions"
)]
async fn if_title_prefix_does_not_exist_body(h: &Harness, prefix: &str) {
    let initial_count = window_count(h).await;

    // First call: should open a new window
    run_cli(
        h,
        &[
            "windows",
            "open",
            "--title-prefix",
            prefix,
            "--if-title-prefix-does-not-exist",
        ],
    )
    .await;

    // Give Firefox a moment to apply the prefix
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let after_first = window_count(h).await;
    pretty_assertions::assert_eq!(
        after_first,
        initial_count + 1,
        "first call should open a new window",
    );

    // Second call: same prefix -> should NOT open another window
    run_cli(
        h,
        &[
            "windows",
            "open",
            "--title-prefix",
            prefix,
            "--if-title-prefix-does-not-exist",
        ],
    )
    .await;

    let after_second = window_count(h).await;
    pretty_assertions::assert_eq!(
        after_second,
        after_first,
        "second call should be a no-op (window with prefix already exists)",
    );

    // Clean up: close the window we opened
    // Find it by title prefix
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    for w in &windows {
        if w.title_prefix.as_deref() == Some(prefix) {}
    }
}

#[tokio::test]
async fn if_title_prefix_does_not_exist_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(if_title_prefix_does_not_exist_body(h, "IDEMPOTENT-TEST:"))
    })
    .await;
}

#[tokio::test]
async fn if_title_prefix_does_not_exist_trailing_space_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(if_title_prefix_does_not_exist_body(h, "IDEMPOTENT-TEST: "))
    })
    .await;
}

// Not tested on Chrome -- title prefix is Firefox-only.

// ---------------------------------------------------------------------------
// --if-url-does-not-exist
// ---------------------------------------------------------------------------

/// Test that `tabs open --url X --if-url-does-not-exist` opens a tab the first
/// time, but is a no-op when the URL already exists.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "tab count arithmetic in test assertions"
)]
async fn if_url_does_not_exist_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;
    let url = server.page2_url();
    let wid = window_id.to_string();

    let initial_count = tab_count(h, window_id).await;

    // First call: should open a new tab
    run_cli(
        h,
        &[
            "tabs",
            "open",
            "--window-id",
            &wid,
            "--url",
            &url,
            "--if-url-does-not-exist",
        ],
    )
    .await;

    // Wait for the tab to load
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let after_first = tab_count(h, window_id).await;
    pretty_assertions::assert_eq!(
        after_first,
        initial_count + 1,
        "first call should open a new tab",
    );

    // Second call: same URL -> should NOT open another tab
    run_cli(
        h,
        &[
            "tabs",
            "open",
            "--window-id",
            &wid,
            "--url",
            &url,
            "--if-url-does-not-exist",
        ],
    )
    .await;

    let after_second = tab_count(h, window_id).await;
    pretty_assertions::assert_eq!(
        after_second,
        after_first,
        "second call should be a no-op (tab with URL already exists)",
    );

    // Clean up: close the tab we opened
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    for t in &tabs {
        if t.url == url {}
    }
}

#[tokio::test]
async fn if_url_does_not_exist_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(if_url_does_not_exist_body(h))
    })
    .await;
}

#[tokio::test]
async fn if_url_does_not_exist_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(if_url_does_not_exist_body(h))
    })
    .await;
}
