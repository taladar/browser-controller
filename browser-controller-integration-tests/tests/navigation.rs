//! Navigation history tests: GoBack and GoForward.

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

/// Shared GoBack/GoForward test body.
///
/// Opens a tab, navigates to two URLs, then goes back and forward.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn go_back_forward_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open a tab with Google (reliably available)
    let url1 = "https://www.google.com/";
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some(url1.to_owned()),
            username: None,
            password: None,
            background: false,
        })
        .await
        .expect("OpenTab should succeed");
    let tab_id = match open_result {
        CliResult::Tab(details) => details.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    // Wait for first page to fully load
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Navigate to a second URL (same domain, different path)
    let url2 = "https://www.google.com/search?q=test";
    h.send_command(CliCommand::NavigateTab {
        tab_id,
        url: url2.to_owned(),
    })
    .await
    .expect("NavigateTab should succeed");

    // Wait for second page to fully load
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    // Verify we're on url2
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    match &tabs {
        CliResult::Tabs { tabs } => {
            let tab = tabs
                .iter()
                .find(|t| t.id == tab_id)
                .expect("tab should exist");
            assert!(
                tab.url.contains("search?q=test") || tab.url.contains("search%3Fq%3Dtest"),
                "tab should be on search URL, got {}",
                tab.url,
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Go back
    h.send_command(CliCommand::GoBack { tab_id, steps: 1 })
        .await
        .expect("GoBack should succeed");

    // Wait for navigation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify we're back on the Google homepage (no search query)
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    match &tabs {
        CliResult::Tabs { tabs } => {
            let tab = tabs
                .iter()
                .find(|t| t.id == tab_id)
                .expect("tab should exist");
            assert!(
                tab.url.starts_with("https://www.google.com") && !tab.url.contains("search?q=test"),
                "tab should be back on google.com homepage after GoBack, got {}",
                tab.url,
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Go forward
    h.send_command(CliCommand::GoForward { tab_id, steps: 1 })
        .await
        .expect("GoForward should succeed");

    // Wait for navigation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify we're back on the search page
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    match &tabs {
        CliResult::Tabs { tabs } => {
            let tab = tabs
                .iter()
                .find(|t| t.id == tab_id)
                .expect("tab should exist");
            assert!(
                tab.url.contains("google.com"),
                "tab should be on google.com after GoForward, got {}",
                tab.url,
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Cleanup
    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn go_back_forward_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(go_back_forward_body(h))
    })
    .await;
}

#[tokio::test]
async fn go_back_forward_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(go_back_forward_body(h))).await;
}
