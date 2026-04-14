//! Event subscription tests: subscribe and verify events fire correctly for
//! all BrowserEvent variants.

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
use browser_controller_integration_tests::test_server;
use browser_controller_types::{BrowserEvent, CliCommand, CliResult};

/// Helper: read events until we find one matching the predicate, or timeout.
async fn wait_for_event<F>(
    subscription: &mut EventSubscription,
    predicate: F,
) -> Option<BrowserEvent>
where
    F: Fn(&BrowserEvent) -> bool,
{
    let deadline = tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_secs(5))
        .unwrap_or_else(tokio::time::Instant::now);

    while tokio::time::Instant::now() < deadline {
        let event =
            tokio::time::timeout(std::time::Duration::from_secs(2), subscription.next_event())
                .await;
        match event {
            Ok(Ok(ref e)) if predicate(e) => return Some(e.clone()),
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }
    None
}

/// Helper: get the first window ID.
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

// --- TabOpened ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_tab_opened_body(h: &Harness) {
    let window_id = first_window_id(h).await;
    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    let tab_id = match h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab")
    {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    let found = wait_for_event(
        &mut sub,
        |e| matches!(e, BrowserEvent::TabOpened { tab_id: tid, .. } if *tid == tab_id),
    )
    .await;
    assert!(found.is_some(), "expected TabOpened event for tab {tab_id}");

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn event_tab_opened_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_tab_opened_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_tab_opened_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_tab_opened_body(h))
    })
    .await;
}

// --- TabClosed ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_tab_closed_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    let tab_id = match h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab")
    {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("CloseTab");

    let found = wait_for_event(
        &mut sub,
        |e| matches!(e, BrowserEvent::TabClosed { tab_id: tid, .. } if *tid == tab_id),
    )
    .await;
    assert!(found.is_some(), "expected TabClosed event for tab {tab_id}");
}

#[tokio::test]
async fn event_tab_closed_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_tab_closed_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_tab_closed_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_tab_closed_body(h))
    })
    .await;
}

// --- TabActivated ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_tab_activated_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    let tab1 = match h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab")
    {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    h.send_command(CliCommand::ActivateTab { tab_id: tab1 })
        .await
        .expect("ActivateTab");

    let found = wait_for_event(
        &mut sub,
        |e| matches!(e, BrowserEvent::TabActivated { tab_id: tid, .. } if *tid == tab1),
    )
    .await;
    assert!(
        found.is_some(),
        "expected TabActivated event for tab {tab1}"
    );

    h.send_command(CliCommand::CloseTab { tab_id: tab1 })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn event_tab_activated_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_tab_activated_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_tab_activated_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_tab_activated_body(h))
    })
    .await;
}

// --- TabNavigated ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_tab_navigated_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    let tab_id = match h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab")
    {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    let target = server.base_url();
    h.send_command(CliCommand::NavigateTab {
        tab_id,
        url: target.clone(),
    })
    .await
    .expect("NavigateTab");

    let found = wait_for_event(&mut sub, |e| {
        matches!(e, BrowserEvent::TabNavigated { tab_id: tid, url, .. } if *tid == tab_id && url.starts_with(&target))
    })
    .await;
    assert!(
        found.is_some(),
        "expected TabNavigated event for tab {tab_id}",
    );

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn event_tab_navigated_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_tab_navigated_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_tab_navigated_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_tab_navigated_body(h))
    })
    .await;
}

// --- TabTitleChanged ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_tab_title_changed_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    let tab_id = match h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab")
    {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    // Navigate to a page with a title — this should trigger TabTitleChanged
    h.send_command(CliCommand::NavigateTab {
        tab_id,
        url: server.base_url(),
    })
    .await
    .expect("NavigateTab");

    let found = wait_for_event(&mut sub, |e| {
        matches!(e, BrowserEvent::TabTitleChanged { tab_id: tid, title, .. } if *tid == tab_id && title.contains("Test Page"))
    })
    .await;
    assert!(
        found.is_some(),
        "expected TabTitleChanged event for tab {tab_id}",
    );

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn event_tab_title_changed_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_tab_title_changed_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_tab_title_changed_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_tab_title_changed_body(h))
    })
    .await;
}

// --- TabStatusChanged ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_tab_status_changed_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    let tab_id = match h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some("about:blank".to_owned()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: None,
        })
        .await
        .expect("OpenTab")
    {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    // Navigate to trigger status changes (loading → complete)
    h.send_command(CliCommand::NavigateTab {
        tab_id,
        url: server.base_url(),
    })
    .await
    .expect("NavigateTab");

    let found = wait_for_event(
        &mut sub,
        |e| matches!(e, BrowserEvent::TabStatusChanged { tab_id: tid, .. } if *tid == tab_id),
    )
    .await;
    assert!(
        found.is_some(),
        "expected TabStatusChanged event for tab {tab_id}",
    );

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn event_tab_status_changed_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_tab_status_changed_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_tab_status_changed_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_tab_status_changed_body(h))
    })
    .await;
}

// --- WindowOpened ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_window_opened_body(h: &Harness) {
    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    let new_window_id = match h
        .send_command(CliCommand::OpenWindow {
            title_prefix: None,
            incognito: false,
        })
        .await
        .expect("OpenWindow")
    {
        CliResult::WindowId { window_id } => window_id,
        other => panic!("expected WindowId, got {other:?}"),
    };

    let found = wait_for_event(&mut sub, |e| {
        matches!(e, BrowserEvent::WindowOpened { window_id, .. } if *window_id == new_window_id)
    })
    .await;
    assert!(
        found.is_some(),
        "expected WindowOpened event for window {new_window_id}",
    );

    h.send_command(CliCommand::CloseWindow {
        window_id: new_window_id,
    })
    .await
    .expect("cleanup");
}

#[tokio::test]
async fn event_window_opened_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_window_opened_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_window_opened_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_window_opened_body(h))
    })
    .await;
}

// --- WindowClosed ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_window_closed_body(h: &Harness) {
    let new_window_id = match h
        .send_command(CliCommand::OpenWindow {
            title_prefix: None,
            incognito: false,
        })
        .await
        .expect("OpenWindow")
    {
        CliResult::WindowId { window_id } => window_id,
        other => panic!("expected WindowId, got {other:?}"),
    };

    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    h.send_command(CliCommand::CloseWindow {
        window_id: new_window_id,
    })
    .await
    .expect("CloseWindow");

    let found = wait_for_event(
        &mut sub,
        |e| matches!(e, BrowserEvent::WindowClosed { window_id } if *window_id == new_window_id),
    )
    .await;
    assert!(
        found.is_some(),
        "expected WindowClosed event for window {new_window_id}",
    );
}

#[tokio::test]
async fn event_window_closed_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_window_closed_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_window_closed_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_window_closed_body(h))
    })
    .await;
}

// --- DownloadCreated (already tested in downloads.rs, but include here for completeness) ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_download_created_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    let download_id = match h
        .send_command(CliCommand::StartDownload {
            url: server.download_url("event-dl.bin"),
            filename: None,
            save_as: false,
            conflict_action: None,
        })
        .await
        .expect("StartDownload")
    {
        CliResult::DownloadId { download_id } => download_id,
        other => panic!("expected DownloadId, got {other:?}"),
    };

    let found = wait_for_event(&mut sub, |e| {
        matches!(e, BrowserEvent::DownloadCreated { download_id: did, .. } if *did == download_id)
    })
    .await;
    assert!(
        found.is_some(),
        "expected DownloadCreated event for download {download_id}",
    );

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    drop(
        h.send_command(CliCommand::EraseDownload { download_id })
            .await,
    );
}

#[tokio::test]
async fn event_download_created_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_download_created_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_download_created_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_download_created_body(h))
    })
    .await;
}

// --- DownloadChanged (state → complete) ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_download_changed_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    let download_id = match h
        .send_command(CliCommand::StartDownload {
            url: server.download_url("event-dlc.bin"),
            filename: None,
            save_as: false,
            conflict_action: None,
        })
        .await
        .expect("StartDownload")
    {
        CliResult::DownloadId { download_id } => download_id,
        other => panic!("expected DownloadId, got {other:?}"),
    };

    // Wait for a DownloadChanged event with state = complete
    let found = wait_for_event(&mut sub, |e| {
        matches!(
            e,
            BrowserEvent::DownloadChanged {
                download_id: did,
                state: Some(browser_controller_types::DownloadState::Complete),
                ..
            } if *did == download_id
        )
    })
    .await;
    assert!(
        found.is_some(),
        "expected DownloadChanged(complete) event for download {download_id}",
    );

    drop(
        h.send_command(CliCommand::EraseDownload { download_id })
            .await,
    );
}

#[tokio::test]
async fn event_download_changed_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_download_changed_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_download_changed_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_download_changed_body(h))
    })
    .await;
}

// --- DownloadErased ---

#[expect(clippy::panic, reason = "test assertions")]
async fn event_download_erased_body(h: &Harness) {
    let server = test_server::Server::start_plain();

    let download_id = match h
        .send_command(CliCommand::StartDownload {
            url: server.download_url("event-dle.bin"),
            filename: None,
            save_as: false,
            conflict_action: None,
        })
        .await
        .expect("StartDownload")
    {
        CliResult::DownloadId { download_id } => download_id,
        other => panic!("expected DownloadId, got {other:?}"),
    };

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let mut sub = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("open");

    h.send_command(CliCommand::EraseDownload { download_id })
        .await
        .expect("EraseDownload");

    let found = wait_for_event(
        &mut sub,
        |e| matches!(e, BrowserEvent::DownloadErased { download_id: did } if *did == download_id),
    )
    .await;
    assert!(
        found.is_some(),
        "expected DownloadErased event for download {download_id}",
    );
}

#[tokio::test]
async fn event_download_erased_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_download_erased_body(h))
    })
    .await;
}
#[tokio::test]
async fn event_download_erased_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_download_erased_body(h))
    })
    .await;
}
