//! Tests for the CLI's `WindowMatcher` parameters.
//!
//! Uses `tabs list` as the test command since it's read-only and accepts
//! all `WindowMatcher` flags. Each test sets up windows with known properties,
//! runs the CLI with a specific matcher flag, and verifies the correct
//! window(s) are matched.

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
use browser_controller_types::{CliResult, WindowId};

/// Run the CLI binary with the given arguments and return stdout.
///
/// Automatically adds `-o json` and `-i <pid>` to target the test browser instance.
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

/// Run the CLI binary expecting failure, and return the exit status.
async fn run_cli_expect_failure(h: &Harness, args: &[&str]) -> std::process::ExitStatus {
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
        "CLI command {args:?} was expected to fail but succeeded.\nstdout: {}",
        String::from_utf8_lossy(&output.stdout),
    );

    output.status
}

/// Helper to get the first window's ID and title via protocol.
async fn first_window_info(h: &Harness) -> (WindowId, String) {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    let w = windows.first().expect("just asserted non-empty");
    (w.id, w.title.clone())
}

// --- Test: --window-id ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_id_body(h: &Harness) {
    let (window_id, _) = first_window_info(h).await;
    let wid = window_id.to_string();
    let stdout = run_cli(h, &["tabs", "list", "--window-id", &wid]).await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(!tabs.is_empty(), "matched window should have tabs");
            // All tabs should belong to the requested window
            for tab in &tabs {
                pretty_assertions::assert_eq!(tab.window_id, window_id);
            }
        }
        other => panic!("expected Tabs, got {other:?}"),
    }
}

#[tokio::test]
async fn match_by_window_id_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_id_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_id_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_window_id_body(h))
    })
    .await;
}

// --- Test: --window-title ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_title_body(h: &Harness) {
    let (_, title) = first_window_info(h).await;
    let stdout = run_cli(h, &["tabs", "list", "--window-title", &title]).await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(!tabs.is_empty(), "matched window should have tabs");
        }
        other => panic!("expected Tabs, got {other:?}"),
    }
}

#[tokio::test]
async fn match_by_window_title_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_title_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_title_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_window_title_body(h))
    })
    .await;
}

// --- Test: --window-title-regex ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_title_regex_body(h: &Harness) {
    // ".*" matches any title
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "list",
            "--window-title-regex",
            ".*",
            "--if-matches-multiple",
            "all",
        ],
    )
    .await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(
                !tabs.is_empty(),
                "regex .* should match at least one window"
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }
}

#[tokio::test]
async fn match_by_window_title_regex_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_title_regex_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_title_regex_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_window_title_regex_body(h))
    })
    .await;
}

// --- Test: --window-title-prefix (Firefox only) ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_title_prefix_body(h: &Harness, prefix: &str) {
    let (window_id, _) = first_window_info(h).await;

    // Set a title prefix
    h.client()
        .set_window_title_prefix(window_id, prefix.to_owned())
        .await
        .expect("SetWindowTitlePrefix should succeed");

    // Give the browser a moment to update the window metadata
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Match by that prefix
    let stdout = run_cli(h, &["tabs", "list", "--window-title-prefix", prefix]).await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(!tabs.is_empty(), "prefix-matched window should have tabs");
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Clean up
    h.client()
        .remove_window_title_prefix(window_id)
        .await
        .expect("RemoveWindowTitlePrefix should succeed");
}

#[tokio::test]
async fn match_by_window_title_prefix_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_title_prefix_body(h, "MATCH-TEST:"))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_title_prefix_trailing_space_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_title_prefix_body(h, "MATCH-TEST: "))
    })
    .await;
}

// --- Test: --window-focused ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_focused_body(h: &Harness) {
    let stdout = run_cli(h, &["tabs", "list", "--window-focused"]).await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(!tabs.is_empty(), "focused window should have tabs");
        }
        other => panic!("expected Tabs, got {other:?}"),
    }
}

#[tokio::test]
async fn match_by_window_focused_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_focused_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_focused_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_window_focused_body(h))
    })
    .await;
}

// --- Test: --window-not-focused ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_not_focused_body(h: &Harness) {
    // Open a second window so there's a non-focused one
    let new_window_id = h
        .client()
        .open_window(None, false)
        .await
        .expect("OpenWindow should succeed");

    let stdout = run_cli(
        h,
        &[
            "tabs",
            "list",
            "--window-not-focused",
            "--if-matches-multiple",
            "all",
        ],
    )
    .await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(
                !tabs.is_empty(),
                "should have tabs from non-focused window(s)",
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Clean up
    h.client()
        .close_window(new_window_id)
        .await
        .expect("CloseWindow should succeed");
}

#[tokio::test]
async fn match_by_window_not_focused_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_not_focused_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_not_focused_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_window_not_focused_body(h))
    })
    .await;
}

// --- Test: --window-last-focused ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_last_focused_body(h: &Harness) {
    let stdout = run_cli(h, &["tabs", "list", "--window-last-focused"]).await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(!tabs.is_empty(), "last-focused window should have tabs",);
        }
        other => panic!("expected Tabs, got {other:?}"),
    }
}

#[tokio::test]
async fn match_by_window_last_focused_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_last_focused_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_last_focused_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_window_last_focused_body(h))
    })
    .await;
}

// --- Test: --window-not-last-focused ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_not_last_focused_body(h: &Harness) {
    // Open a second window so there's a non-last-focused one
    let new_window_id = h
        .client()
        .open_window(None, false)
        .await
        .expect("OpenWindow should succeed");

    let stdout = run_cli(
        h,
        &[
            "tabs",
            "list",
            "--window-not-last-focused",
            "--if-matches-multiple",
            "all",
        ],
    )
    .await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(
                !tabs.is_empty(),
                "should have tabs from non-last-focused window(s)",
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    h.client()
        .close_window(new_window_id)
        .await
        .expect("CloseWindow should succeed");
}

#[tokio::test]
async fn match_by_window_not_last_focused_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_not_last_focused_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_not_last_focused_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_window_not_last_focused_body(h))
    })
    .await;
}

// --- Test: --window-state ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_by_window_state_body(h: &Harness) {
    // Default windows should be in normal state
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "list",
            "--window-state",
            "normal",
            "--if-matches-multiple",
            "all",
        ],
    )
    .await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
    match result {
        CliResult::Tabs { tabs } => {
            assert!(!tabs.is_empty(), "normal-state windows should have tabs",);
        }
        other => panic!("expected Tabs, got {other:?}"),
    }
}

#[tokio::test]
async fn match_by_window_state_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_by_window_state_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_by_window_state_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_by_window_state_body(h))
    })
    .await;
}

// --- Test: --if-matches-multiple abort (default behavior) ---

async fn match_multiple_abort_body(h: &Harness) {
    // Open a second window so there are multiple
    let new_window_id = h
        .client()
        .open_window(None, false)
        .await
        .expect("OpenWindow should succeed");

    // tabs list with --window-title-regex ".*" matches all windows;
    // default --if-matches-multiple is abort, so this should fail
    run_cli_expect_failure(h, &["tabs", "list", "--window-title-regex", ".*"]).await;

    // Clean up
    h.client()
        .close_window(new_window_id)
        .await
        .expect("CloseWindow should succeed");
}

#[tokio::test]
async fn match_multiple_abort_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_multiple_abort_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_multiple_abort_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_multiple_abort_body(h))
    })
    .await;
}

// --- Test: --if-matches-multiple all ---

#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn match_multiple_all_body(h: &Harness) {
    // Open a second window
    let new_window_id = h
        .client()
        .open_window(None, false)
        .await
        .expect("OpenWindow should succeed");

    // tabs list with regex matching all + --if-matches-multiple all
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "list",
            "--window-title-regex",
            ".*",
            "--if-matches-multiple",
            "all",
        ],
    )
    .await;

    // When listing tabs from multiple windows, the CLI outputs one pretty-printed
    // CliResult::Tabs JSON per window. Use a streaming JSON deserializer to parse
    // multiple top-level values from the output.
    let deserializer = serde_json::Deserializer::from_str(stdout.trim()).into_iter::<CliResult>();
    let mut window_count = 0usize;
    for result in deserializer {
        let result = result.expect("should parse as CliResult");
        match result {
            CliResult::Tabs { .. } => {
                window_count = window_count.checked_add(1).expect("window count overflow");
            }
            other => panic!("expected Tabs, got {other:?}"),
        }
    }

    assert!(
        window_count >= 2,
        "expected tabs from at least 2 windows, got {window_count}",
    );

    // Clean up
    h.client()
        .close_window(new_window_id)
        .await
        .expect("CloseWindow should succeed");
}

#[tokio::test]
async fn match_multiple_all_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(match_multiple_all_body(h))
    })
    .await;
}

#[tokio::test]
async fn match_multiple_all_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(match_multiple_all_body(h))
    })
    .await;
}
