//! Tab operation tests: pin/unpin, mute/unmute, move, activate, discard/warmup.

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
use browser_controller_integration_tests::test_server;
use browser_controller_types::{TabId, WindowId};

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

/// Helper to open a new tab and return its ID.
async fn open_test_tab(h: &Harness, window_id: WindowId) -> TabId {
    let mut params = OpenTabParams::new(window_id);
    params.url = Some("about:blank".to_owned());
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");
    tab.id
}

/// Shared pin/unpin test body.
async fn pin_unpin_body(h: &Harness) {
    let window_id = first_window_id(h).await;
    let tab_id = open_test_tab(h, window_id).await;

    // Pin the tab
    h.client()
        .pin_tab(tab_id)
        .await
        .expect("PinTab should succeed");

    // Verify via ListTabs
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs.iter().find(|t| t.id == tab_id);
    assert!(tab.is_some(), "tab should exist");
    assert!(
        tab.expect("just asserted").is_pinned,
        "tab should be pinned after PinTab",
    );

    // Unpin the tab
    h.client()
        .unpin_tab(tab_id)
        .await
        .expect("UnpinTab should succeed");

    // Verify via ListTabs
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs.iter().find(|t| t.id == tab_id);
    assert!(tab.is_some(), "tab should exist");
    assert!(
        !tab.expect("just asserted").is_pinned,
        "tab should not be pinned after UnpinTab",
    );

    // Cleanup
    h.client()
        .close_tab(tab_id)
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn pin_unpin_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(pin_unpin_body(h))).await;
}

#[tokio::test]
async fn pin_unpin_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(pin_unpin_body(h))).await;
}

/// Shared mute/unmute test body.
async fn mute_unmute_body(h: &Harness) {
    let window_id = first_window_id(h).await;
    let tab_id = open_test_tab(h, window_id).await;

    // Mute the tab
    h.client()
        .mute_tab(tab_id)
        .await
        .expect("MuteTab should succeed");

    // Verify via ListTabs
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs.iter().find(|t| t.id == tab_id).expect("tab exists");
    assert!(tab.is_muted, "tab should be muted after MuteTab");

    // Unmute the tab
    h.client()
        .unmute_tab(tab_id)
        .await
        .expect("UnmuteTab should succeed");

    // Verify via ListTabs
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs.iter().find(|t| t.id == tab_id).expect("tab exists");
    assert!(!tab.is_muted, "tab should not be muted after UnmuteTab");

    // Cleanup
    h.client()
        .close_tab(tab_id)
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn mute_unmute_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(mute_unmute_body(h))).await;
}

#[tokio::test]
async fn mute_unmute_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(mute_unmute_body(h))).await;
}

/// Shared activate tab test body.
async fn activate_tab_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open two tabs
    let tab1 = open_test_tab(h, window_id).await;
    let tab2 = open_test_tab(h, window_id).await;

    // tab2 should be active (last opened)
    // Activate tab1
    h.client()
        .activate_tab(tab1)
        .await
        .expect("ActivateTab should succeed");

    // Verify tab1 is now active
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let t1 = tabs.iter().find(|t| t.id == tab1);
    assert!(t1.is_some(), "tab1 should exist");
    assert!(
        t1.expect("just asserted").is_active,
        "tab1 should be active after ActivateTab",
    );
    let t2 = tabs.iter().find(|t| t.id == tab2);
    assert!(t2.is_some(), "tab2 should exist");
    assert!(
        !t2.expect("just asserted").is_active,
        "tab2 should not be active after activating tab1",
    );

    // Cleanup
    h.client()
        .close_tab(tab2)
        .await
        .expect("CloseTab should succeed");
    h.client()
        .close_tab(tab1)
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn activate_tab_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(activate_tab_body(h))).await;
}

#[tokio::test]
async fn activate_tab_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(activate_tab_body(h))).await;
}

/// Shared move tab test body.
async fn move_tab_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open two extra tabs so we have something to reorder
    let tab1 = open_test_tab(h, window_id).await;
    let tab2 = open_test_tab(h, window_id).await;

    // Get tab2's current index
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab2_index = tabs
        .iter()
        .find(|t| t.id == tab2)
        .expect("tab2 should exist")
        .index;

    // Move tab2 to index 0
    let moved = h
        .client()
        .move_tab(tab2, 0)
        .await
        .expect("MoveTab should succeed");
    pretty_assertions::assert_eq!(moved.index, 0, "tab2 should be at index 0 after move");

    // Move it back
    h.client()
        .move_tab(tab2, tab2_index)
        .await
        .expect("MoveTab back should succeed");

    // Cleanup
    h.client()
        .close_tab(tab2)
        .await
        .expect("CloseTab should succeed");
    h.client()
        .close_tab(tab1)
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn move_tab_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(move_tab_body(h))).await;
}

#[tokio::test]
async fn move_tab_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(move_tab_body(h))).await;
}

/// Shared discard/warmup test body.
async fn discard_warmup_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;
    // Open a tab in background (can't discard the active tab)
    let tab_id = open_test_tab(h, window_id).await;
    // Navigate it to a real page so it has content to discard
    h.client()
        .navigate_tab(tab_id, server.base_url())
        .await
        .expect("NavigateTab should succeed");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Make sure tab is not active before discarding
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs.iter().find(|t| t.id == tab_id).expect("tab exists");
    // If the tab is active, activate the first tab instead
    if tab.is_active
        && let Some(other) = tabs.iter().find(|t| t.id != tab_id)
    {
        h.client()
            .activate_tab(other.id)
            .await
            .expect("ActivateTab should succeed");
    }

    // Discard the tab
    h.client()
        .discard_tab(tab_id)
        .await
        .expect("DiscardTab should succeed");

    // Verify it's discarded
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs.iter().find(|t| t.id == tab_id).expect("tab exists");
    assert!(tab.is_discarded, "tab should be discarded after DiscardTab");

    // Warm up the tab (Firefox-only, no-op on Chrome)
    h.client()
        .warmup_tab(tab_id)
        .await
        .expect("WarmupTab should succeed");

    // Cleanup
    h.client()
        .close_tab(tab_id)
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn discard_warmup_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(discard_warmup_body(h))).await;
}

#[tokio::test]
async fn discard_warmup_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(discard_warmup_body(h))).await;
}

/// Shared reload test body.
async fn reload_tab_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;
    let tab_id = open_test_tab(h, window_id).await;

    // Navigate to a real page first
    h.client()
        .navigate_tab(tab_id, server.base_url())
        .await
        .expect("NavigateTab should succeed");
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Normal reload
    h.client()
        .reload_tab(tab_id, false)
        .await
        .expect("ReloadTab should succeed");

    // Verify URL is still correct
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs.iter().find(|t| t.id == tab_id).expect("tab exists");
    assert!(
        tab.url.starts_with(&server.base_url()),
        "URL should still be the test server after reload, got {}",
        tab.url,
    );

    // Force reload (bypass cache)
    h.client()
        .reload_tab(tab_id, true)
        .await
        .expect("ReloadTab with bypass_cache should succeed");

    // Verify URL is still correct
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs.iter().find(|t| t.id == tab_id).expect("tab exists");
    assert!(
        tab.url.starts_with(&server.base_url()),
        "URL should still be the test server after force reload, got {}",
        tab.url,
    );

    // Cleanup
    h.client()
        .close_tab(tab_id)
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn reload_tab_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(reload_tab_body(h))).await;
}

#[tokio::test]
async fn reload_tab_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(reload_tab_body(h))).await;
}
