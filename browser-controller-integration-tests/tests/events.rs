//! Event subscription tests: subscribe and verify events arrive.

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
use browser_controller_integration_tests::cli::EventSubscription;
use browser_controller_integration_tests::harness;
use browser_controller_types::{BrowserEvent, CliCommand, CliResult};

/// Shared event subscription test body.
///
/// Opens an event subscription, then opens a new tab and verifies
/// that a `TabOpened` event is received.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn subscribe_events_body(h: &Harness) {
    // Get a window ID first
    let result = h
        .send_command(CliCommand::ListWindows)
        .await
        .expect("ListWindows should succeed");
    let window_id = match result {
        CliResult::Windows { windows } => {
            assert!(!windows.is_empty(), "need at least 1 window");
            windows.first().expect("just asserted non-empty").id
        }
        other => panic!("expected Windows, got {other:?}"),
    };

    // Open event subscription
    let mut subscription = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("EventSubscription::open should succeed");

    // Open a new tab — this should generate a TabOpened event
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
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

    // Read events with a timeout — we expect at least one TabOpened event
    let mut found_tab_opened = false;
    let deadline = tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_secs(5))
        .unwrap_or_else(tokio::time::Instant::now);

    while tokio::time::Instant::now() < deadline {
        let event =
            tokio::time::timeout(std::time::Duration::from_secs(2), subscription.next_event())
                .await;

        match event {
            Ok(Ok(BrowserEvent::TabOpened {
                tab_id: event_tab_id,
                ..
            })) if event_tab_id == tab_id => {
                found_tab_opened = true;
                break;
            }
            Ok(Ok(_other_event)) => {
                // Other events (TabActivated, etc.) may arrive first; keep reading
                continue;
            }
            Ok(Err(e)) => panic!("error reading event: {e}"),
            Err(_timeout) => break,
        }
    }

    assert!(
        found_tab_opened,
        "expected TabOpened event for tab {tab_id}",
    );

    // Cleanup
    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("CloseTab should succeed");
}

#[tokio::test]
async fn subscribe_events_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(subscribe_events_body(h))
    })
    .await;
}

#[tokio::test]
async fn subscribe_events_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(subscribe_events_body(h))
    })
    .await;
}
