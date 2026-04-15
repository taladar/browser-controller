//! Navigation history tests: GoBack and GoForward.

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
use browser_controller_integration_tests::test_server;
use browser_controller_types::WindowId;

/// Helper to get the first window ID.
async fn first_window_id(h: &Harness) -> WindowId {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    windows.first().expect("just asserted non-empty").id
}

/// Shared GoBack/GoForward test body.
///
/// Opens a tab, navigates to a second page, then goes back and forward.
async fn go_back_forward_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    // Open a tab on the test server's main page
    let url1 = server.base_url();
    let params = OpenTabParamsBuilder::default()
        .window_id(window_id)
        .url(url1.clone())
        .build()
        .expect("build OpenTabParams");
    let details = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");
    let tab_id = details.id;

    // Wait for first page to fully load
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Navigate to the second page
    let url2 = server.page2_url();
    h.client()
        .navigate_tab(tab_id, url2.clone())
        .await
        .expect("NavigateTab should succeed");

    // Wait for second page to fully load
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify we're on page2
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs
        .iter()
        .find(|t| t.id == tab_id)
        .expect("tab should exist");
    assert!(
        tab.url.contains("/page2"),
        "tab should be on /page2, got {}",
        tab.url,
    );

    // Go back
    h.client()
        .go_back(tab_id, 1)
        .await
        .expect("GoBack should succeed");

    // Wait for navigation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify we're back on the main page (no /page2)
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs
        .iter()
        .find(|t| t.id == tab_id)
        .expect("tab should exist");
    assert!(
        !tab.url.contains("/page2"),
        "tab should be on main page after GoBack, got {}",
        tab.url,
    );

    // Go forward
    h.client()
        .go_forward(tab_id, 1)
        .await
        .expect("GoForward should succeed");

    // Wait for navigation
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Verify we're back on page2
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs
        .iter()
        .find(|t| t.id == tab_id)
        .expect("tab should exist");
    assert!(
        tab.url.contains("/page2"),
        "tab should be on /page2 after GoForward, got {}",
        tab.url,
    );

    // Cleanup
    h.client()
        .close_tab(tab_id)
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
