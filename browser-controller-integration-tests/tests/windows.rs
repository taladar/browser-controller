//! Window lifecycle tests: list, open, close, and title prefix.

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

/// Shared list-windows test body.
async fn list_windows_body(h: &Harness) {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");

    assert!(!windows.is_empty(), "should have at least 1 window");
    for window in &windows {
        assert!(
            !window.tabs.is_empty(),
            "window {} should have at least 1 tab",
            window.id,
        );
    }
}

#[tokio::test]
async fn list_windows_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(list_windows_body(h))).await;
}

#[tokio::test]
async fn list_windows_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(list_windows_body(h))).await;
}

/// Shared open/close window test body.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "window count arithmetic in test assertions cannot overflow in practice"
)]
async fn open_close_window_body(h: &Harness) {
    // Get initial window count
    let initial_windows = h
        .client()
        .list_windows()
        .await
        .expect("initial ListWindows should succeed");
    let initial_count = initial_windows.len();

    // Open a new window
    let new_window_id = h
        .client()
        .open_window(None, false)
        .await
        .expect("OpenWindow should succeed");

    // Verify count increased
    let after_open_windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows after open should succeed");
    pretty_assertions::assert_eq!(
        after_open_windows.len(),
        initial_count + 1,
        "window count should increase by 1 after OpenWindow",
    );

    // If niri is available, verify new window appears.
    // Wait briefly for the compositor to register the new window.
    if browser_controller_integration_tests::niri::is_available()
        && let Some(pid) = h.browser_pid
    {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let count = browser_controller_integration_tests::niri::count_windows_for_pid(pid)
            .expect("niri window count should succeed");
        assert!(
            count > initial_count,
            "niri should see more than {initial_count} windows for PID {pid}, got {count}",
        );
    }

    // Close the new window
    h.client()
        .close_window(new_window_id)
        .await
        .expect("CloseWindow should succeed");

    // Verify count restored
    let after_close_windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows after close should succeed");
    pretty_assertions::assert_eq!(
        after_close_windows.len(),
        initial_count,
        "window count should return to initial after CloseWindow",
    );
}

#[tokio::test]
async fn open_close_window_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(open_close_window_body(h))
    })
    .await;
}

#[tokio::test]
async fn open_close_window_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(open_close_window_body(h))
    })
    .await;
}

/// Shared title prefix test body — Firefox-only (Chrome does not support titlePreface).
#[expect(
    clippy::indexing_slicing,
    reason = "test asserts non-empty before indexing"
)]
async fn title_prefix_body(h: &Harness, prefix: &str) {
    // Get a window ID
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    let window_id = windows[0].id;

    // Set title prefix
    h.client()
        .set_window_title_prefix(window_id, prefix.to_owned())
        .await
        .expect("SetWindowTitlePrefix should succeed");

    // Verify via ListWindows that the prefix is reported correctly
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    let w = windows.iter().find(|w| w.id == window_id);
    assert!(w.is_some(), "window should still exist");
    pretty_assertions::assert_eq!(
        w.expect("just asserted").title_prefix.as_deref(),
        Some(prefix),
        "title_prefix should match exactly (including trailing whitespace)",
    );

    // If niri is available, verify the title prefix is visible
    if browser_controller_integration_tests::niri::is_available()
        && let Some(pid) = h.browser_pid
    {
        let has_prefix =
            browser_controller_integration_tests::niri::has_window_with_title_prefix(pid, prefix)
                .expect("niri title prefix check should succeed");
        assert!(
            has_prefix,
            "expected a window with title prefix {prefix:?} for PID {pid}",
        );
    }

    // Remove title prefix
    h.client()
        .remove_window_title_prefix(window_id)
        .await
        .expect("RemoveWindowTitlePrefix should succeed");

    // If niri is available, verify the prefix is removed
    if browser_controller_integration_tests::niri::is_available()
        && let Some(pid) = h.browser_pid
    {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let has_prefix =
            browser_controller_integration_tests::niri::has_window_with_title_prefix(pid, prefix)
                .expect("niri title prefix check should succeed");
        assert!(
            !has_prefix,
            "title prefix {prefix:?} should be removed from all windows for PID {pid}",
        );
    }
}

#[tokio::test]
async fn title_prefix_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(title_prefix_body(h, "TEST-PREFIX:"))
    })
    .await;
}

#[tokio::test]
async fn title_prefix_trailing_space_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(title_prefix_body(h, "TEST-PREFIX: "))
    })
    .await;
}
