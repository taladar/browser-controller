//! End-to-end tests that spawn the actual CLI binary.
//!
//! These tests verify the CLI's JSON output format and commands that are only
//! available through the CLI (like `sort`).

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
use browser_controller_types::CliResult;

/// Run the CLI binary with the given arguments and return stdout.
///
/// Automatically adds `-o json` and `-i <pid>` to target the test browser instance.
///
/// # Panics
///
/// Panics if the CLI binary is not found or the command fails.
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

/// Test: `browser-controller windows list` outputs valid JSON.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn cli_windows_list_body(h: &Harness) {
    let stdout = run_cli(h, &["windows", "list"]).await;

    // Output is a CliResult enum: {"type":"Windows","windows":[...]}
    let result: CliResult =
        serde_json::from_str(stdout.trim()).expect("CLI JSON output should be valid CliResult");
    match result {
        CliResult::Windows { windows } => {
            assert!(!windows.is_empty(), "should have at least 1 window");
        }
        other => panic!("expected Windows result, got {other:?}"),
    }
}

#[tokio::test]
async fn cli_windows_list_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(cli_windows_list_body(h))
    })
    .await;
}

#[tokio::test]
async fn cli_windows_list_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(cli_windows_list_body(h))
    })
    .await;
}

/// Test: `browser-controller tabs list` outputs valid JSON.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn cli_tabs_list_body(h: &Harness) {
    // Get a window ID via the protocol first
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    let window_id = windows.first().expect("just asserted non-empty").id;

    let stdout = run_cli(h, &["tabs", "list", "--window-id", &window_id.to_string()]).await;

    // Output is a CliResult enum: {"type":"Tabs","tabs":[...]}
    let result: CliResult =
        serde_json::from_str(stdout.trim()).expect("CLI JSON output should be valid CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(!tabs.is_empty(), "should have at least 1 tab");
        }
        other => panic!("expected Tabs result, got {other:?}"),
    }
}

#[tokio::test]
async fn cli_tabs_list_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(cli_tabs_list_body(h))).await;
}

#[tokio::test]
async fn cli_tabs_list_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(cli_tabs_list_body(h))).await;
}

/// Test: `browser-controller tabs sort` reorders tabs by domain.
///
/// Opens tabs with different URLs, sorts by domain order, and verifies
/// the resulting tab order via the protocol.
async fn cli_tabs_sort_body(h: &Harness) {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    let window_id = windows.first().expect("just asserted non-empty").id;

    // Open tabs with different domains
    let urls = [
        "https://www.google.com/",
        "https://en.wikipedia.org/",
        "https://www.google.com/maps",
    ];
    let mut tab_ids = Vec::new();
    for url in &urls {
        let params = OpenTabParamsBuilder::default()
            .window_id(window_id)
            .url(*url)
            .background(true)
            .build()
            .expect("build OpenTabParams");
        let tab = h
            .client()
            .open_tab(params)
            .await
            .expect("OpenTab should succeed");
        tab_ids.push(tab.id);
    }

    // Wait for pages to start loading
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Sort by domain: wikipedia first, then google
    let wid = window_id.to_string();
    run_cli(
        h,
        &[
            "tabs",
            "sort",
            "--window-id",
            &wid,
            "--domains",
            "en.wikipedia.org,www.google.com",
        ],
    )
    .await;

    // Verify tab order: wikipedia tabs should come before google tabs
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");

    // Find our test tabs by ID and check their relative order
    let test_tabs: Vec<_> = tabs.iter().filter(|t| tab_ids.contains(&t.id)).collect();

    // Find first wikipedia and first google tab among our test tabs
    let wiki_idx = test_tabs
        .iter()
        .position(|t| t.url.contains("wikipedia.org"));
    let google_idx = test_tabs.iter().position(|t| t.url.contains("google.com"));

    if let (Some(wi), Some(gi)) = (wiki_idx, google_idx) {
        assert!(
            wi < gi,
            "wikipedia tab (index {wi}) should come before google tab (index {gi}) after sort",
        );
    }
}

#[tokio::test]
async fn cli_tabs_sort_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(cli_tabs_sort_body(h))).await;
}

#[tokio::test]
async fn cli_tabs_sort_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(cli_tabs_sort_body(h))).await;
}
