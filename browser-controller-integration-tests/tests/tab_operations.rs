//! Tab operation tests: pin/unpin, mute/unmute, move, activate.

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

/// Helper to open a new tab and return its ID.
#[expect(clippy::panic, reason = "test helper panics on unexpected variants")]
async fn open_test_tab(h: &Harness, window_id: u32) -> u32 {
    let result = h
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
    match result {
        CliResult::Tab(details) => details.id,
        other => panic!("expected Tab, got {other:?}"),
    }
}

/// Shared pin/unpin test body.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn pin_unpin_body(h: &Harness) {
    let window_id = first_window_id(h).await;
    let tab_id = open_test_tab(h, window_id).await;

    // Pin the tab
    let pin_result = h
        .send_command(CliCommand::PinTab { tab_id })
        .await
        .expect("PinTab should succeed");
    match &pin_result {
        CliResult::Tab(details) => {
            assert!(details.is_pinned, "tab should be pinned after PinTab");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    // Verify via ListTabs
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    match &tabs {
        CliResult::Tabs { tabs } => {
            let tab = tabs.iter().find(|t| t.id == tab_id);
            assert!(tab.is_some(), "tab should exist");
            assert!(
                tab.expect("just asserted").is_pinned,
                "tab should be pinned in ListTabs",
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Unpin the tab
    let unpin_result = h
        .send_command(CliCommand::UnpinTab { tab_id })
        .await
        .expect("UnpinTab should succeed");
    match &unpin_result {
        CliResult::Tab(details) => {
            assert!(
                !details.is_pinned,
                "tab should not be pinned after UnpinTab"
            );
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    // Cleanup
    h.send_command(CliCommand::CloseTab { tab_id })
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
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn mute_unmute_body(h: &Harness) {
    let window_id = first_window_id(h).await;
    let tab_id = open_test_tab(h, window_id).await;

    // Mute the tab
    let mute_result = h
        .send_command(CliCommand::MuteTab { tab_id })
        .await
        .expect("MuteTab should succeed");
    match &mute_result {
        CliResult::Tab(details) => {
            assert!(details.is_muted, "tab should be muted after MuteTab");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    // Unmute the tab
    let unmute_result = h
        .send_command(CliCommand::UnmuteTab { tab_id })
        .await
        .expect("UnmuteTab should succeed");
    match &unmute_result {
        CliResult::Tab(details) => {
            assert!(!details.is_muted, "tab should not be muted after UnmuteTab");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    // Cleanup
    h.send_command(CliCommand::CloseTab { tab_id })
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
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn activate_tab_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open two tabs
    let tab1 = open_test_tab(h, window_id).await;
    let tab2 = open_test_tab(h, window_id).await;

    // tab2 should be active (last opened)
    // Activate tab1
    h.send_command(CliCommand::ActivateTab { tab_id: tab1 })
        .await
        .expect("ActivateTab should succeed");

    // Verify tab1 is now active
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    match &tabs {
        CliResult::Tabs { tabs } => {
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
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    // Cleanup
    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("CloseTab should succeed");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
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
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn move_tab_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open two extra tabs so we have something to reorder
    let tab1 = open_test_tab(h, window_id).await;
    let tab2 = open_test_tab(h, window_id).await;

    // Get tab2's current index
    let tabs = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    let tab2_index = match &tabs {
        CliResult::Tabs { tabs } => {
            let t = tabs
                .iter()
                .find(|t| t.id == tab2)
                .expect("tab2 should exist");
            t.index
        }
        other => panic!("expected Tabs, got {other:?}"),
    };

    // Move tab2 to index 0
    let move_result = h
        .send_command(CliCommand::MoveTab {
            tab_id: tab2,
            new_index: 0,
        })
        .await
        .expect("MoveTab should succeed");
    match &move_result {
        CliResult::Tab(details) => {
            pretty_assertions::assert_eq!(details.index, 0, "tab2 should be at index 0 after move");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    // Move it back
    h.send_command(CliCommand::MoveTab {
        tab_id: tab2,
        new_index: tab2_index,
    })
    .await
    .expect("MoveTab back should succeed");

    // Cleanup
    h.send_command(CliCommand::CloseTab { tab_id: tab2 })
        .await
        .expect("CloseTab should succeed");
    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
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
