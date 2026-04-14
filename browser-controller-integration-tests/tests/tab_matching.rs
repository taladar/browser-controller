//! Tests for the CLI's `TabMatcher` parameters.
//!
//! Uses `tabs activate` as the primary test command since it accepts
//! `TabMatcher`, is read-safe (activating an already-active tab is harmless),
//! and returns `CliResult::Tab` for verification. Uses `tabs mute`/`tabs unmute`
//! for subset-match tests where the side effect is verifiable per-tab.

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

/// Run the CLI binary with the given arguments, asserting success.
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

/// Run the CLI binary expecting failure.
async fn run_cli_expect_failure(h: &Harness, args: &[&str]) {
    let cli_bin = profile::cli_binary().expect("CLI binary should be built");
    let pid = h
        .browser_pid
        .expect("browser PID should be known for CLI tests");

    let mut cmd = tokio::process::Command::new(&cli_bin);
    cmd.arg("-o").arg("json");
    cmd.arg("-i").arg(pid.to_string());
    cmd.args(args);

    let output = cmd.output().await.expect("CLI process should start");
    assert!(
        !output.status.success(),
        "CLI command {args:?} was expected to fail but succeeded",
    );
}

/// Get the first window ID.
#[expect(clippy::panic, reason = "test helper")]
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

/// Open a test tab (about:blank) and return its ID.
#[expect(clippy::panic, reason = "test helper")]
async fn open_blank_tab(h: &Harness, window_id: u32) -> u32 {
    let result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            username: None,
            password: None,
            background: false,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab should succeed");
    match result {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    }
}

/// Open a test tab navigated to a URL and return its ID.
#[expect(clippy::panic, reason = "test helper")]
async fn open_url_tab(h: &Harness, window_id: u32, url: &str) -> u32 {
    let result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some(url.to_owned()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab should succeed");
    match result {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    }
}

// --- --tab-id ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_id_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab_id = open_blank_tab(h, wid).await;
    let tid = tab_id.to_string();

    let stdout = run_cli(h, &["tabs", "activate", "--tab-id", &tid]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => pretty_assertions::assert_eq!(d.id, tab_id),
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_id_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_id_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_id_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(match_by_tab_id_body(h))).await;
}

// --- --tab-title ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_title_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let tab_id = open_url_tab(h, wid, &server.base_url()).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-title",
            "Test Page",
            "--tab-window-id",
            &w,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => assert!(d.title.contains("Test Page"), "got {}", d.title),
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_title_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_title_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_title_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_title_body(h))
    })
    .await;
}

// --- --tab-title-regex ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_title_regex_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let tab_id = open_url_tab(h, wid, &server.base_url()).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-title-regex",
            "Test.*",
            "--tab-window-id",
            &w,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match &result {
        CliResult::Tab(_) => {} // success
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_title_regex_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_title_regex_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_title_regex_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_title_regex_body(h))
    })
    .await;
}

// --- --tab-url ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_url_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let url = server.page2_url();
    let tab_id = open_url_tab(h, wid, &url).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-url", &url, "--tab-window-id", &w],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => assert!(d.url.contains("/page2"), "got {}", d.url),
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_url_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_url_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_url_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_url_body(h))
    })
    .await;
}

// --- --tab-url-domain ---
// Note: --tab-url-domain uses url::Url::domain() which returns None for IP
// addresses like 127.0.0.1. We use a real domain (google.com) for this test.

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_url_domain_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab_id = open_url_tab(h, wid, "https://www.google.com/").await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-url-domain",
            "www.google.com",
            "--tab-window-id",
            &w,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match &result {
        CliResult::Tab(_) => {}
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_url_domain_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_url_domain_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_url_domain_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_url_domain_body(h))
    })
    .await;
}

// --- --tab-url-regex ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_url_regex_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let tab_id = open_url_tab(h, wid, &server.base_url()).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-url-regex",
            r"http://127\.0\.0\.1:\d+.*",
            "--tab-window-id",
            &w,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match &result {
        CliResult::Tab(_) => {}
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_url_regex_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_url_regex_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_url_regex_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_url_regex_body(h))
    })
    .await;
}

// --- --tab-window-id + --tab-active ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_window_id_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let w = wid.to_string();

    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-window-id", &w, "--tab-active"],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.window_id, wid);
            assert!(d.is_active, "should be active tab");
        }
        other => panic!("expected Tab, got {other:?}"),
    }
}

#[tokio::test]
async fn match_by_tab_window_id_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_window_id_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_window_id_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_window_id_body(h))
    })
    .await;
}

// --- --tab-active ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_active_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let _tab1 = open_blank_tab(h, wid).await;
    let tab2 = open_blank_tab(h, wid).await; // tab2 becomes active
    let w = wid.to_string();

    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-active", "--tab-window-id", &w],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab2, "active tab should be the last opened");
            assert!(d.is_active, "tab should be active");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: _tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_active_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_active_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_active_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_active_body(h))
    })
    .await;
}

// --- --tab-not-active (use mute to verify subset) ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_not_active_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab1 = open_blank_tab(h, wid).await;
    let tab2 = open_blank_tab(h, wid).await; // tab2 is active
    let w = wid.to_string();

    // Mute all non-active tabs
    run_cli(
        h,
        &[
            "tabs",
            "mute",
            "--tab-not-active",
            "--tab-window-id",
            &w,
            "--if-matches-multiple",
            "all",
        ],
    )
    .await;

    // Verify: tab1 should be muted (not active), tab2 should NOT be muted (active)
    let result = h
        .send_command(CliCommand::ListTabs { window_id: wid })
        .await
        .expect("ListTabs");
    match result {
        CliResult::Tabs { tabs } => {
            let t1 = tabs.iter().find(|t| t.id == tab1).expect("tab1 exists");
            let t2 = tabs.iter().find(|t| t.id == tab2).expect("tab2 exists");
            assert!(t1.is_muted, "tab1 (not-active) should be muted");
            assert!(!t2.is_muted, "tab2 (active) should NOT be muted");
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Cleanup: unmute and close
    h.send_command(CliCommand::UnmuteTab { tab_id: tab1 })
        .await
        .expect("unmute");
    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_not_active_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_not_active_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_not_active_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_not_active_body(h))
    })
    .await;
}

// --- --tab-pinned ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_pinned_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab_id = open_blank_tab(h, wid).await;
    h.send_command(CliCommand::PinTab { tab_id })
        .await
        .expect("pin");
    let w = wid.to_string();

    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-pinned", "--tab-window-id", &w],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => assert!(d.is_pinned, "matched tab should be pinned"),
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::UnpinTab { tab_id })
        .await
        .expect("unpin");
    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_pinned_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_pinned_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_pinned_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_pinned_body(h))
    })
    .await;
}

// --- --tab-not-pinned ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_not_pinned_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab1 = open_blank_tab(h, wid).await;
    let tab2 = open_blank_tab(h, wid).await;
    h.send_command(CliCommand::PinTab { tab_id: tab1 })
        .await
        .expect("pin");
    let w = wid.to_string();

    // Mute all unpinned tabs
    run_cli(
        h,
        &[
            "tabs",
            "mute",
            "--tab-not-pinned",
            "--tab-window-id",
            &w,
            "--if-matches-multiple",
            "all",
        ],
    )
    .await;

    // Verify tab2 (unpinned) got muted, tab1 (pinned) did not
    let result = h
        .send_command(CliCommand::ListTabs { window_id: wid })
        .await
        .expect("ListTabs");
    match result {
        CliResult::Tabs { tabs } => {
            let t1 = tabs.iter().find(|t| t.id == tab1).expect("tab1");
            let t2 = tabs.iter().find(|t| t.id == tab2).expect("tab2");
            assert!(!t1.is_muted, "pinned tab should NOT be muted");
            assert!(t2.is_muted, "unpinned tab should be muted");
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    h.send_command(CliCommand::UnmuteTab { tab_id: tab2 })
        .await
        .expect("unmute");
    h.send_command(CliCommand::UnpinTab { tab_id: tab1 })
        .await
        .expect("unpin");
    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_not_pinned_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_not_pinned_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_not_pinned_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_not_pinned_body(h))
    })
    .await;
}

// --- --tab-muted ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_muted_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab_id = open_blank_tab(h, wid).await;
    h.send_command(CliCommand::MuteTab { tab_id })
        .await
        .expect("mute");
    let w = wid.to_string();

    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-muted", "--tab-window-id", &w],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => assert!(d.is_muted, "matched tab should be muted"),
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::UnmuteTab { tab_id })
        .await
        .expect("unmute");
    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_muted_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_muted_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_muted_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_muted_body(h))
    })
    .await;
}

// --- --tab-not-muted ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_not_muted_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab1 = open_blank_tab(h, wid).await;
    let tab2 = open_blank_tab(h, wid).await;
    h.send_command(CliCommand::MuteTab { tab_id: tab1 })
        .await
        .expect("mute");
    let t2 = tab2.to_string();

    // Activate the non-muted tab by matching --tab-not-muted and --tab-id
    let stdout = run_cli(h, &["tabs", "activate", "--tab-not-muted", "--tab-id", &t2]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab2);
            assert!(!d.is_muted, "tab should not be muted");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::UnmuteTab { tab_id: tab1 })
        .await
        .expect("unmute");
    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_not_muted_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_not_muted_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_not_muted_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_not_muted_body(h))
    })
    .await;
}

// --- Negative boolean matchers (trivially true for normal tabs) ---

/// Test --tab-discarded: discard a tab via protocol, then match it.
#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_discarded_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    // Open a tab in the background (can't discard the active tab)
    let tab_id = open_url_tab(h, wid, &server.base_url()).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Discard the tab
    h.send_command(CliCommand::DiscardTab { tab_id })
        .await
        .expect("DiscardTab should succeed");

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-discarded", "--tab-window-id", &w],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab_id, "should match the discarded tab");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_discarded_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_discarded_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_discarded_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_discarded_body(h))
    })
    .await;
}

/// Test --tab-not-discarded with a mix of discarded and non-discarded tabs.
#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_not_discarded_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let tab1 = open_url_tab(h, wid, &server.base_url()).await;
    let tab2 = open_blank_tab(h, wid).await; // tab2 becomes active
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Discard tab1 (it's not active)
    h.send_command(CliCommand::DiscardTab { tab_id: tab1 })
        .await
        .expect("DiscardTab");

    // Activate a non-discarded tab by matching --tab-not-discarded + --tab-id
    let t2 = tab2.to_string();
    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-not-discarded", "--tab-id", &t2],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab2, "should match non-discarded tab");
            assert!(!d.is_discarded, "tab should not be discarded");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_not_discarded_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_not_discarded_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_not_discarded_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_not_discarded_body(h))
    })
    .await;
}

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_audible_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let tab_id = open_url_tab(h, wid, &server.audio_url()).await;
    // Give the browser time to start playing audio and mark the tab as audible
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-audible", "--tab-window-id", &w],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab_id, "should match the audible tab");
            assert!(d.is_audible, "tab should be audible");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_audible_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_audible_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_audible_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_audible_body(h))
    })
    .await;
}

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_not_audible_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    // Open an audio tab and a silent tab
    let audio_tab = open_url_tab(h, wid, &server.audio_url()).await;
    let silent_tab = open_blank_tab(h, wid).await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Activate the non-audible tab
    let t = silent_tab.to_string();
    let stdout = run_cli(
        h,
        &["tabs", "activate", "--tab-not-audible", "--tab-id", &t],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, silent_tab, "should match the silent tab");
            assert!(!d.is_audible, "tab should not be audible");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id: silent_tab })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: audio_tab })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_not_audible_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_not_audible_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_not_audible_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_not_audible_body(h))
    })
    .await;
}

// --tab-incognito is NOT tested because Firefox bug 1729315 prevents
// extensions.allowPrivateBrowsingByDefault from working for temporary
// extensions installed via WebDriver. The extension can't access tabs
// in private windows in the test environment.
// The --incognito flag on `windows open` and the --tab-incognito matcher
// work correctly in production when the user grants private browsing access.

/// Test --tab-not-incognito: match a tab in a regular (non-private) window.
async fn match_by_tab_not_incognito_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab_id = open_blank_tab(h, wid).await;
    let t = tab_id.to_string();

    run_cli(
        h,
        &["tabs", "activate", "--tab-not-incognito", "--tab-id", &t],
    )
    .await;

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_not_incognito_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_not_incognito_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_not_incognito_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_not_incognito_body(h))
    })
    .await;
}

/// Test --tab-in-reader-mode: open a known reader-compatible page, toggle reader mode.
/// Firefox-only — Chrome doesn't support Reader Mode.
/// Uses a real website because Firefox's readability algorithm is strict about
/// content structure and local test server pages may not qualify.
#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_in_reader_mode_body(h: &Harness) {
    let wid = first_window_id(h).await;
    // Use a real article URL that Firefox is known to consider reader-compatible
    let server = test_server::Server::start_plain();
    // Open as active tab — Firefox only analyzes active tabs for readability
    let result = h
        .send_command(CliCommand::OpenTab {
            window_id: wid,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some(server.article_url()),
            username: None,
            password: None,
            background: false,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab should succeed");
    let tab_id = match result {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };
    // Wait for the page to fully load so Firefox analyzes it for readability
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Toggle reader mode on
    h.send_command(CliCommand::ToggleReaderMode { tab_id })
        .await
        .expect("ToggleReaderMode should succeed");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-in-reader-mode",
            "--tab-window-id",
            &w,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab_id, "should match the reader-mode tab");
            assert!(d.is_in_reader_mode, "tab should be in reader mode");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    // Toggle reader mode off before closing
    h.send_command(CliCommand::ToggleReaderMode { tab_id })
        .await
        .expect("ToggleReaderMode off should succeed");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_in_reader_mode_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_in_reader_mode_body(h))
    })
    .await;
}

/// Test --tab-not-in-reader-mode.
async fn match_by_tab_not_in_reader_mode_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab_id = open_blank_tab(h, wid).await;
    let t = tab_id.to_string();

    run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-not-in-reader-mode",
            "--tab-id",
            &t,
        ],
    )
    .await;

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_not_in_reader_mode_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_not_in_reader_mode_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_not_in_reader_mode_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_not_in_reader_mode_body(h))
    })
    .await;
}

// --- --tab-awaiting-auth ---
// Open a tab to the test server's /auth endpoint WITHOUT credentials,
// causing the browser to show its auth prompt (is_awaiting_auth = true).

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_awaiting_auth_body(h: &Harness) {
    let server = test_server::Server::start_with_auth("user", "pass");
    let wid = first_window_id(h).await;

    // Open tab to auth endpoint WITHOUT credentials — triggers 401 prompt
    let tab_id = open_url_tab(h, wid, &server.auth_url()).await;
    // Give the browser time to fire the auth challenge
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-awaiting-auth",
            "--tab-window-id",
            &w,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab_id, "should match the auth-waiting tab");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_awaiting_auth_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_awaiting_auth_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_awaiting_auth_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_awaiting_auth_body(h))
    })
    .await;
}

// --- --tab-not-awaiting-auth ---

async fn match_by_tab_not_awaiting_auth_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab_id = open_blank_tab(h, wid).await;
    let t = tab_id.to_string();

    // A blank tab should not be awaiting auth
    run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-not-awaiting-auth",
            "--tab-id",
            &t,
        ],
    )
    .await;

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_not_awaiting_auth_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_not_awaiting_auth_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_not_awaiting_auth_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_not_awaiting_auth_body(h))
    })
    .await;
}

// --- --tab-status complete ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_by_tab_status_complete_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let tab_id = open_url_tab(h, wid, &server.base_url()).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let t = tab_id.to_string();

    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-status",
            "complete",
            "--tab-id",
            &t,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match &result {
        CliResult::Tab(_) => {}
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_by_tab_status_complete_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_tab_status_complete_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_by_tab_status_complete_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_tab_status_complete_body(h))
    })
    .await;
}

// --- --if-matches-multiple abort (tabs) ---

async fn match_tab_multiple_abort_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab1 = open_blank_tab(h, wid).await;
    let tab2 = open_blank_tab(h, wid).await;
    let w = wid.to_string();

    // With 3+ tabs (original + 2 new), matching all should fail with abort
    run_cli_expect_failure(
        h,
        &[
            "tabs",
            "activate",
            "--tab-window-id",
            &w,
            "--tab-url-regex",
            ".*",
        ],
    )
    .await;

    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_tab_multiple_abort_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_tab_multiple_abort_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_tab_multiple_abort_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_tab_multiple_abort_body(h))
    })
    .await;
}

// --- --if-matches-multiple all (tabs) ---

#[expect(clippy::panic, reason = "test assertions")]
async fn match_tab_multiple_all_body(h: &Harness) {
    let wid = first_window_id(h).await;
    let tab1 = open_blank_tab(h, wid).await;
    let tab2 = open_blank_tab(h, wid).await;
    let w = wid.to_string();

    // Mute all tabs in the window
    run_cli(
        h,
        &[
            "tabs",
            "mute",
            "--tab-window-id",
            &w,
            "--tab-url-regex",
            ".*",
            "--if-matches-multiple",
            "all",
        ],
    )
    .await;

    // Verify all test tabs are muted
    let result = h
        .send_command(CliCommand::ListTabs { window_id: wid })
        .await
        .expect("ListTabs");
    match result {
        CliResult::Tabs { tabs } => {
            let t1 = tabs.iter().find(|t| t.id == tab1).expect("tab1");
            let t2 = tabs.iter().find(|t| t.id == tab2).expect("tab2");
            assert!(t1.is_muted, "tab1 should be muted");
            assert!(t2.is_muted, "tab2 should be muted");
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Cleanup
    h.send_command(CliCommand::UnmuteTab { tab_id: tab1 })
        .await
        .expect("unmute");
    h.send_command(CliCommand::UnmuteTab { tab_id: tab2 })
        .await
        .expect("unmute");
    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn match_tab_multiple_all_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_tab_multiple_all_body(h))
    })
    .await;
}
#[tokio::test]
async fn match_tab_multiple_all_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_tab_multiple_all_body(h))
    })
    .await;
}
