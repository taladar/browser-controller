//! Tests for `tabs open` position and background flags:
//! `--before`, `--after`, `--background`.

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

/// Run the CLI binary, asserting success.
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
            background: true,
        })
        .await
        .expect("OpenTab should succeed");
    match result {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    }
}

// --- --before ---

#[expect(clippy::panic, reason = "test assertions")]
async fn open_tab_before_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let tab1 = open_blank_tab(h, wid).await;
    let tab2 = open_blank_tab(h, wid).await;

    // Get tab2's current index
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id: wid })
        .await
        .expect("ListTabs");
    let tab2_index = match &tabs {
        CliResult::Tabs { tabs } => tabs.iter().find(|t| t.id == tab2).expect("tab2").index,
        other => panic!("expected Tabs, got {other:?}"),
    };

    // Open a new tab before tab2 via CLI
    let w = wid.to_string();
    let t2 = tab2.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "open",
            "--window-id",
            &w,
            "--before",
            &t2,
            "--url",
            &server.base_url(),
        ],
    )
    .await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    let new_tab_id = match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(
                d.index,
                tab2_index,
                "new tab should be at tab2's original index",
            );
            d.id
        }
        other => panic!("expected Tab, got {other:?}"),
    };

    h.send_command(CliCommand::CloseTab { tab_id: new_tab_id })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn open_tab_before_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(open_tab_before_body(h))
    })
    .await;
}
#[tokio::test]
async fn open_tab_before_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(open_tab_before_body(h))).await;
}

// --- --after ---

#[expect(clippy::panic, reason = "test assertions")]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "tab index arithmetic in test assertions"
)]
async fn open_tab_after_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;
    let tab1 = open_blank_tab(h, wid).await;

    // Get tab1's current index
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id: wid })
        .await
        .expect("ListTabs");
    let tab1_index = match &tabs {
        CliResult::Tabs { tabs } => tabs.iter().find(|t| t.id == tab1).expect("tab1").index,
        other => panic!("expected Tabs, got {other:?}"),
    };

    // Open a new tab after tab1 via CLI
    let w = wid.to_string();
    let t1 = tab1.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "open",
            "--window-id",
            &w,
            "--after",
            &t1,
            "--url",
            &server.base_url(),
        ],
    )
    .await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    let new_tab_id = match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(
                d.index,
                tab1_index + 1,
                "new tab should be right after tab1",
            );
            d.id
        }
        other => panic!("expected Tab, got {other:?}"),
    };

    h.send_command(CliCommand::CloseTab { tab_id: new_tab_id })
        .await
        .expect("cleanup");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn open_tab_after_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(open_tab_after_body(h))).await;
}
#[tokio::test]
async fn open_tab_after_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(open_tab_after_body(h))).await;
}

// --- --background ---

#[expect(clippy::panic, reason = "test assertions")]
async fn open_tab_background_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let wid = first_window_id(h).await;

    // Get the currently active tab
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id: wid })
        .await
        .expect("ListTabs");
    let active_tab_id = match &tabs {
        CliResult::Tabs { tabs } => {
            tabs.iter()
                .find(|t| t.is_active)
                .expect("should have an active tab")
                .id
        }
        other => panic!("expected Tabs, got {other:?}"),
    };

    // Open a tab in the background via CLI
    let w = wid.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "open",
            "--window-id",
            &w,
            "--background",
            "--url",
            &server.base_url(),
        ],
    )
    .await;

    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    let new_tab_id = match result {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    // Verify the original tab is still active
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id: wid })
        .await
        .expect("ListTabs");
    match &tabs {
        CliResult::Tabs { tabs } => {
            let active = tabs
                .iter()
                .find(|t| t.is_active)
                .expect("should have active tab");
            pretty_assertions::assert_eq!(
                active.id,
                active_tab_id,
                "original tab should still be active after --background open",
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id: new_tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn open_tab_background_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(open_tab_background_body(h))
    })
    .await;
}
#[tokio::test]
async fn open_tab_background_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(open_tab_background_body(h))
    })
    .await;
}
