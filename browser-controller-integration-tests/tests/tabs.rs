//! Tab lifecycle tests: open, close, navigate, and list tabs.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are inherently outside #[cfg(test)]"
)]
#![expect(
    clippy::expect_used,
    reason = "panicking on unexpected failure is acceptable in tests"
)]

use browser_controller_client::OpenTabParams;
use browser_controller_integration_tests::Harness;
use browser_controller_integration_tests::browser;
use browser_controller_integration_tests::harness;
use browser_controller_types::WindowId;

/// Helper to get the first window ID.
///
/// # Panics
///
/// Panics if `ListWindows` fails or returns no windows.
async fn first_window_id(h: &Harness) -> WindowId {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    windows.first().expect("just asserted non-empty").id
}

/// Helper to count tabs in a window.
///
/// # Panics
///
/// Panics if `ListTabs` fails.
async fn tab_count(h: &Harness, window_id: WindowId) -> usize {
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    tabs.len()
}

/// Shared open/close tab test body.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "tab count arithmetic in test assertions cannot overflow in practice"
)]
async fn open_close_tab_body(h: &Harness) {
    let window_id = first_window_id(h).await;
    let initial_count = tab_count(h, window_id).await;

    // Open a new tab
    let mut params = OpenTabParams::new(window_id);
    params.url = Some("about:blank".to_owned());
    let details = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");

    pretty_assertions::assert_eq!(
        details.window_id,
        window_id,
        "new tab should be in the requested window",
    );
    let new_tab_id = details.id;

    // Verify tab count increased
    let after_open_count = tab_count(h, window_id).await;
    pretty_assertions::assert_eq!(
        after_open_count,
        initial_count + 1,
        "tab count should increase by 1 after OpenTab",
    );

    // Close the tab
    h.client()
        .close_tab(new_tab_id)
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
async fn navigate_tab_body(h: &Harness) {
    let server = browser_controller_integration_tests::test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    // Open a tab with about:blank
    let mut params = OpenTabParams::new(window_id);
    params.url = Some("about:blank".to_owned());
    let details = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");
    let tab_id = details.id;

    // Navigate to local test server
    let target_url = server.base_url();
    h.client()
        .navigate_tab(tab_id, target_url.clone())
        .await
        .expect("NavigateTab should succeed");

    // Give the tab time to finish navigation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify the URL changed
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");

    let tab = tabs.iter().find(|t| t.id == tab_id);
    assert!(tab.is_some(), "tab {tab_id} should still exist");
    let tab = tab.expect("just asserted it exists");
    assert!(
        tab.url.starts_with(&target_url),
        "tab URL should start with {target_url} after NavigateTab, got {}",
        tab.url,
    );

    // Cleanup: close the tab
    h.client()
        .close_tab(tab_id)
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
