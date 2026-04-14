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
use browser_controller_types::{CliCommand, CliResult};

/// Shared list-windows test body.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn list_windows_body(h: &Harness) {
    let result = h
        .send_command(CliCommand::ListWindows)
        .await
        .expect("ListWindows should succeed");

    match result {
        CliResult::Windows { windows } => {
            assert!(!windows.is_empty(), "should have at least 1 window");
            for window in &windows {
                assert!(
                    !window.tabs.is_empty(),
                    "window {} should have at least 1 tab",
                    window.id,
                );
            }
        }
        other => panic!("expected Windows, got {other:?}"),
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
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "window count arithmetic in test assertions cannot overflow in practice"
)]
async fn open_close_window_body(h: &Harness) {
    // Get initial window count
    let initial = h
        .send_command(CliCommand::ListWindows)
        .await
        .expect("initial ListWindows should succeed");
    let initial_count = match &initial {
        CliResult::Windows { windows } => windows.len(),
        other => panic!("expected Windows, got {other:?}"),
    };

    // Open a new window
    let open_result = h
        .send_command(CliCommand::OpenWindow {
            title_prefix: None,
            incognito: false,
        })
        .await
        .expect("OpenWindow should succeed");
    let new_window_id = match open_result {
        CliResult::WindowId { window_id } => window_id,
        other => panic!("expected WindowId, got {other:?}"),
    };

    // Verify count increased
    let after_open = h
        .send_command(CliCommand::ListWindows)
        .await
        .expect("ListWindows after open should succeed");
    match &after_open {
        CliResult::Windows { windows } => {
            pretty_assertions::assert_eq!(
                windows.len(),
                initial_count + 1,
                "window count should increase by 1 after OpenWindow",
            );
        }
        other => panic!("expected Windows, got {other:?}"),
    }

    // If niri is available, verify new window appears
    if browser_controller_integration_tests::niri::is_available()
        && let Some(pid) = h.browser_pid
    {
        let count = browser_controller_integration_tests::niri::count_windows_for_pid(pid)
            .expect("niri window count should succeed");
        assert!(
            count > initial_count,
            "niri should see more than {initial_count} windows for PID {pid}, got {count}",
        );
    }

    // Close the new window
    h.send_command(CliCommand::CloseWindow {
        window_id: new_window_id,
    })
    .await
    .expect("CloseWindow should succeed");

    // Verify count restored
    let after_close = h
        .send_command(CliCommand::ListWindows)
        .await
        .expect("ListWindows after close should succeed");
    match &after_close {
        CliResult::Windows { windows } => {
            pretty_assertions::assert_eq!(
                windows.len(),
                initial_count,
                "window count should return to initial after CloseWindow",
            );
        }
        other => panic!("expected Windows, got {other:?}"),
    }
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
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
#[expect(
    clippy::indexing_slicing,
    reason = "test asserts non-empty before indexing"
)]
async fn title_prefix_body(h: &Harness, prefix: &str) {
    // Get a window ID
    let result = h
        .send_command(CliCommand::ListWindows)
        .await
        .expect("ListWindows should succeed");
    let window_id = match &result {
        CliResult::Windows { windows } => {
            assert!(!windows.is_empty(), "need at least 1 window");
            windows[0].id
        }
        other => panic!("expected Windows, got {other:?}"),
    };

    // Set title prefix
    h.send_command(CliCommand::SetWindowTitlePrefix {
        window_id,
        prefix: prefix.to_owned(),
    })
    .await
    .expect("SetWindowTitlePrefix should succeed");

    // Verify via ListWindows that the prefix is reported correctly
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let result = h
        .send_command(CliCommand::ListWindows)
        .await
        .expect("ListWindows should succeed");
    match &result {
        CliResult::Windows { windows } => {
            let w = windows.iter().find(|w| w.id == window_id);
            assert!(w.is_some(), "window should still exist");
            pretty_assertions::assert_eq!(
                w.expect("just asserted").title_prefix.as_deref(),
                Some(prefix),
                "title_prefix should match exactly (including trailing whitespace)",
            );
        }
        other => panic!("expected Windows, got {other:?}"),
    }

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
    h.send_command(CliCommand::RemoveWindowTitlePrefix { window_id })
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
