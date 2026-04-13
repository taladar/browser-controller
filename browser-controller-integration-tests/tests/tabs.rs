//! Tab lifecycle tests: open, close, navigate, and list tabs.

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
use browser_controller_types::{CliCommand, CliResult};

/// Helper to get the first window ID.
///
/// # Panics
///
/// Panics if `ListWindows` fails or returns no windows.
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

/// Helper to count tabs in a window.
///
/// # Panics
///
/// Panics if `ListTabs` fails or returns unexpected variant.
#[expect(clippy::panic, reason = "test helper panics on unexpected variants")]
async fn tab_count(h: &Harness, window_id: u32) -> usize {
    let result = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    match result {
        CliResult::Tabs { tabs } => tabs.len(),
        other => panic!("expected Tabs, got {other:?}"),
    }
}

/// Shared open/close tab test body.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "tab count arithmetic in test assertions cannot overflow in practice"
)]
async fn open_close_tab_body(h: &Harness) {
    let window_id = first_window_id(h).await;
    let initial_count = tab_count(h, window_id).await;

    // Open a new tab
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            strip_credentials: false,
            background: false,
        })
        .await
        .expect("OpenTab should succeed");

    let new_tab_id = match open_result {
        CliResult::Tab(details) => {
            pretty_assertions::assert_eq!(
                details.window_id,
                window_id,
                "new tab should be in the requested window",
            );
            details.id
        }
        other => panic!("expected Tab, got {other:?}"),
    };

    // Verify tab count increased
    let after_open_count = tab_count(h, window_id).await;
    pretty_assertions::assert_eq!(
        after_open_count,
        initial_count + 1,
        "tab count should increase by 1 after OpenTab",
    );

    // Close the tab
    h.send_command(CliCommand::CloseTab { tab_id: new_tab_id })
        .await
        .expect("CloseTab should succeed");

    // Verify tab count restored
    let after_close_count = tab_count(h, window_id).await;
    pretty_assertions::assert_eq!(
        after_close_count,
        initial_count,
        "tab count should return to initial after CloseTab",
    );
}

#[tokio::test]
async fn open_close_tab_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(open_close_tab_body(h))).await;
}

#[tokio::test]
async fn open_close_tab_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(open_close_tab_body(h))).await;
}

/// Shared navigate tab test body.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn navigate_tab_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open a tab with about:blank
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            strip_credentials: false,
            background: false,
        })
        .await
        .expect("OpenTab should succeed");

    let tab_id = match open_result {
        CliResult::Tab(details) => details.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    // Navigate to example.com (a stable, always-available test URL)
    let target_url = "https://example.com/";
    h.send_command(CliCommand::NavigateTab {
        tab_id,
        url: target_url.to_owned(),
    })
    .await
    .expect("NavigateTab should succeed");

    // Give the tab time to finish navigation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify the URL changed
    let tabs_result = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");

    match tabs_result {
        CliResult::Tabs { tabs } => {
            let tab = tabs.iter().find(|t| t.id == tab_id);
            assert!(tab.is_some(), "tab {tab_id} should still exist");
            let tab = tab.expect("just asserted it exists");
            assert!(
                tab.url.starts_with("https://example.com"),
                "tab URL should start with https://example.com after NavigateTab, got {}",
                tab.url,
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Cleanup: close the tab
    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn navigate_tab_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(navigate_tab_body(h))).await;
}

#[tokio::test]
async fn navigate_tab_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(navigate_tab_body(h))).await;
}
