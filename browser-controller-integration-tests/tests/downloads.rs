//! Download management tests: list, start, erase, clear, events.

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
use browser_controller_integration_tests::profile;
use browser_controller_integration_tests::test_server;
use browser_controller_types::{BrowserEvent, CliCommand, CliResult};

/// Run the CLI binary, asserting success.
async fn run_cli(h: &Harness, args: &[&str]) -> String {
    let cli_bin = profile::cli_binary().expect("CLI binary should be built");
    let pid = h
        .browser_pid
        .expect("browser PID should be known for CLI tests");

    let mut cmd = tokio::process::Command::new(&cli_bin);
    cmd.arg("-o").arg("json");
    cmd.arg("-i").arg(pid.to_string());
    cmd.args(args);

    let output = cmd.output().await.expect("CLI process should start");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "CLI command {args:?} failed with status {}.\nstdout: {stdout}\nstderr: {stderr}",
        output.status,
    );
    stdout
}

// --- Protocol-level: start and list ---

#[expect(clippy::panic, reason = "test assertions")]
async fn start_and_list_downloads_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let url = server.download_url("test-file.bin");

    // Start a download
    let result = h
        .send_command(CliCommand::StartDownload {
            url: url.clone(),
            filename: None,
            save_as: false,
            conflict_action: None,
        })
        .await
        .expect("StartDownload should succeed");
    let download_id = match result {
        CliResult::DownloadId { download_id } => download_id,
        other => panic!("expected DownloadId, got {other:?}"),
    };

    // Wait for download to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // List downloads and verify ours is there
    let result = h
        .send_command(CliCommand::ListDownloads {
            state: None,
            limit: None,
            query: None,
        })
        .await
        .expect("ListDownloads should succeed");
    match result {
        CliResult::Downloads { downloads } => {
            let dl = downloads.iter().find(|d| d.id == download_id);
            assert!(dl.is_some(), "our download should appear in the list");
            let dl = dl.expect("just asserted");
            assert!(
                dl.url.contains("test-file.bin"),
                "download URL should contain test-file.bin, got {}",
                dl.url,
            );
        }
        other => panic!("expected Downloads, got {other:?}"),
    }

    // Cleanup
    h.send_command(CliCommand::EraseDownload { download_id })
        .await
        .expect("EraseDownload should succeed");
}

#[tokio::test]
async fn start_and_list_downloads_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(start_and_list_downloads_body(h))
    })
    .await;
}

#[tokio::test]
async fn start_and_list_downloads_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(start_and_list_downloads_body(h))
    })
    .await;
}

// --- Protocol-level: erase ---

#[expect(clippy::panic, reason = "test assertions")]
async fn erase_download_body(h: &Harness) {
    let server = test_server::Server::start_plain();

    let result = h
        .send_command(CliCommand::StartDownload {
            url: server.download_url("erase-test.bin"),
            filename: None,
            save_as: false,
            conflict_action: None,
        })
        .await
        .expect("StartDownload should succeed");
    let download_id = match result {
        CliResult::DownloadId { download_id } => download_id,
        other => panic!("expected DownloadId, got {other:?}"),
    };

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Erase it
    h.send_command(CliCommand::EraseDownload { download_id })
        .await
        .expect("EraseDownload should succeed");

    // Verify it's gone
    let result = h
        .send_command(CliCommand::ListDownloads {
            state: None,
            limit: None,
            query: None,
        })
        .await
        .expect("ListDownloads should succeed");
    match result {
        CliResult::Downloads { downloads } => {
            assert!(
                downloads.iter().all(|d| d.id != download_id),
                "erased download should not appear in list",
            );
        }
        other => panic!("expected Downloads, got {other:?}"),
    }
}

#[tokio::test]
async fn erase_download_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(erase_download_body(h))).await;
}

#[tokio::test]
async fn erase_download_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(erase_download_body(h))).await;
}

// --- Protocol-level: erase all ---

#[expect(clippy::panic, reason = "test assertions")]
async fn erase_all_downloads_body(h: &Harness) {
    let server = test_server::Server::start_plain();

    // Start two downloads
    h.send_command(CliCommand::StartDownload {
        url: server.download_url("clear1.bin"),
        filename: None,
        save_as: false,
        conflict_action: None,
    })
    .await
    .expect("StartDownload 1 should succeed");

    h.send_command(CliCommand::StartDownload {
        url: server.download_url("clear2.bin"),
        filename: None,
        save_as: false,
        conflict_action: None,
    })
    .await
    .expect("StartDownload 2 should succeed");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Erase all
    h.send_command(CliCommand::EraseAllDownloads { state: None })
        .await
        .expect("EraseAllDownloads should succeed");

    // Verify empty
    let result = h
        .send_command(CliCommand::ListDownloads {
            state: None,
            limit: None,
            query: None,
        })
        .await
        .expect("ListDownloads should succeed");
    match result {
        CliResult::Downloads { downloads } => {
            assert!(
                downloads.is_empty(),
                "download list should be empty after clear"
            );
        }
        other => panic!("expected Downloads, got {other:?}"),
    }
}

#[tokio::test]
async fn erase_all_downloads_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(erase_all_downloads_body(h))
    })
    .await;
}

#[tokio::test]
async fn erase_all_downloads_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(erase_all_downloads_body(h))
    })
    .await;
}

// --- Protocol-level: download events ---

#[expect(clippy::panic, reason = "test assertions")]
async fn download_events_body(h: &Harness) {
    let server = test_server::Server::start_plain();

    // Open event subscription
    let mut subscription = EventSubscription::open(&h.mediator_socket)
        .await
        .expect("EventSubscription should open");

    // Start a download
    let result = h
        .send_command(CliCommand::StartDownload {
            url: server.download_url("event-test.bin"),
            filename: None,
            save_as: false,
            conflict_action: None,
        })
        .await
        .expect("StartDownload should succeed");
    let download_id = match result {
        CliResult::DownloadId { download_id } => download_id,
        other => panic!("expected DownloadId, got {other:?}"),
    };

    // Read events with a timeout — expect DownloadCreated
    let mut found_created = false;
    let deadline = tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_secs(5))
        .unwrap_or_else(tokio::time::Instant::now);

    while tokio::time::Instant::now() < deadline {
        let event =
            tokio::time::timeout(std::time::Duration::from_secs(2), subscription.next_event())
                .await;

        match event {
            Ok(Ok(BrowserEvent::DownloadCreated {
                download_id: eid, ..
            })) if eid == download_id => {
                found_created = true;
                break;
            }
            Ok(Ok(_)) => continue,
            Ok(Err(e)) => panic!("error reading event: {e}"),
            Err(_timeout) => break,
        }
    }

    assert!(
        found_created,
        "expected DownloadCreated event for download {download_id}",
    );

    // Cleanup
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    h.send_command(CliCommand::EraseDownload { download_id })
        .await
        .expect("EraseDownload should succeed");
}

#[tokio::test]
async fn download_events_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(download_events_body(h))
    })
    .await;
}

#[tokio::test]
async fn download_events_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(download_events_body(h))).await;
}

// --- CLI: downloads list ---

#[expect(clippy::panic, reason = "test assertions")]
async fn cli_downloads_list_body(h: &Harness) {
    let stdout = run_cli(h, &["downloads", "list"]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Downloads { .. } => {}
        other => panic!("expected Downloads, got {other:?}"),
    }
}

#[tokio::test]
async fn cli_downloads_list_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(cli_downloads_list_body(h))
    })
    .await;
}

#[tokio::test]
async fn cli_downloads_list_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(cli_downloads_list_body(h))
    })
    .await;
}

// --- CLI: downloads start + list ---

#[expect(clippy::panic, reason = "test assertions")]
async fn cli_downloads_start_and_list_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let url = server.download_url("cli-test.bin");

    let stdout = run_cli(h, &["downloads", "start", "--url", &url]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    let download_id = match result {
        CliResult::DownloadId { download_id } => download_id,
        other => panic!("expected DownloadId, got {other:?}"),
    };

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let stdout = run_cli(h, &["downloads", "list"]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Downloads { downloads } => {
            assert!(
                downloads.iter().any(|d| d.id == download_id),
                "started download should appear in list",
            );
        }
        other => panic!("expected Downloads, got {other:?}"),
    }

    // Cleanup
    let did = download_id.to_string();
    run_cli(h, &["downloads", "erase", "--id", &did]).await;
}

#[tokio::test]
async fn cli_downloads_start_and_list_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(cli_downloads_start_and_list_body(h))
    })
    .await;
}

#[tokio::test]
async fn cli_downloads_start_and_list_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(cli_downloads_start_and_list_body(h))
    })
    .await;
}

// --- CLI: downloads clear ---

#[expect(clippy::panic, reason = "test assertions")]
async fn cli_downloads_clear_body(h: &Harness) {
    let server = test_server::Server::start_plain();

    // Start a download via protocol
    h.send_command(CliCommand::StartDownload {
        url: server.download_url("clear-cli.bin"),
        filename: None,
        save_as: false,
        conflict_action: None,
    })
    .await
    .expect("StartDownload should succeed");

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Clear via CLI
    run_cli(h, &["downloads", "clear"]).await;

    // Verify empty
    let result = h
        .send_command(CliCommand::ListDownloads {
            state: None,
            limit: None,
            query: None,
        })
        .await
        .expect("ListDownloads should succeed");
    match result {
        CliResult::Downloads { downloads } => {
            assert!(downloads.is_empty(), "should be empty after clear");
        }
        other => panic!("expected Downloads, got {other:?}"),
    }
}

#[tokio::test]
async fn cli_downloads_clear_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(cli_downloads_clear_body(h))
    })
    .await;
}

#[tokio::test]
async fn cli_downloads_clear_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(cli_downloads_clear_body(h))
    })
    .await;
}
