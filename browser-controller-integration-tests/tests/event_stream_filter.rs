//! Tests for event-stream --downloads and --windows-tabs filter flags.

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
use browser_controller_integration_tests::profile;
use browser_controller_integration_tests::test_server;
use browser_controller_types::{DownloadId, TabId, WindowId};

/// Spawn the CLI event-stream command as a background process with the given
/// extra flags, returning the child process. The caller must kill it when done.
async fn spawn_event_stream(h: &Harness, extra_args: &[&str]) -> tokio::process::Child {
    let cli_bin = profile::cli_binary().expect("CLI binary should be built");
    let pid = h
        .browser_pid
        .expect("browser PID should be known for CLI tests");

    let mut cmd = tokio::process::Command::new(&cli_bin);
    cmd.arg("-i").arg(pid.to_string());
    cmd.arg("event-stream");
    cmd.args(extra_args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::null());

    cmd.spawn().expect("event-stream process should start")
}

/// Read all available output from the child's stdout within a timeout.
async fn collect_output(child: &mut tokio::process::Child, timeout: std::time::Duration) -> String {
    use tokio::io::AsyncReadExt as _;

    let stdout = child.stdout.as_mut().expect("stdout should be piped");
    let mut buf = vec![0u8; 0x0001_0000];
    let mut collected = String::new();

    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(tokio::time::Instant::now);

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(std::time::Duration::from_millis(500), stdout.read(&mut buf))
            .await
        {
            Ok(Ok(0) | Err(_)) => break,
            Ok(Ok(n)) => {
                collected.push_str(&String::from_utf8_lossy(buf.get(..n).unwrap_or(&[])));
            }
            Err(_timeout) => continue,
        }
    }

    collected
}

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

/// Open a tab and return its ID.
async fn open_test_tab(h: &Harness, window_id: WindowId) -> TabId {
    let params = OpenTabParamsBuilder::default()
        .window_id(window_id)
        .url("about:blank")
        .background(true)
        .build()
        .expect("build OpenTabParams");
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");
    tab.id
}

/// Start a download and return its ID.
async fn start_test_download(h: &Harness, url: &str) -> DownloadId {
    h.client()
        .start_download(url.to_owned(), None, false, None)
        .await
        .expect("StartDownload should succeed")
}

// --- --downloads flag: only download events ---

async fn event_stream_downloads_only_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    let mut child = spawn_event_stream(h, &["--downloads"]).await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Trigger a tab event (should NOT appear)
    let tab_id = open_test_tab(h, window_id).await;

    // Trigger a download event (should appear)
    let download_id = start_test_download(h, &server.download_url("filter-test.bin")).await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let output = collect_output(&mut child, std::time::Duration::from_secs(2)).await;
    drop(child.kill().await);

    assert!(
        output.contains("DownloadCreated") || output.contains("DownloadChanged"),
        "output should contain download events, got: {output}",
    );
    assert!(
        !output.contains("TabOpened"),
        "output should NOT contain TabOpened with --downloads filter, got: {output}",
    );

    // Cleanup
    if tab_id > TabId(0) {
        drop(h.client().close_tab(tab_id).await);
    }
    if download_id > DownloadId(0) {
        drop(h.client().erase_download(download_id).await);
    }
}

#[tokio::test]
async fn event_stream_downloads_only_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_stream_downloads_only_body(h))
    })
    .await;
}

#[tokio::test]
async fn event_stream_downloads_only_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_stream_downloads_only_body(h))
    })
    .await;
}

// --- --windows-tabs flag: only window/tab events ---

async fn event_stream_windows_tabs_only_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    let mut child = spawn_event_stream(h, &["--windows-tabs"]).await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Trigger a download event (should NOT appear)
    let download_id = start_test_download(h, &server.download_url("filter-test2.bin")).await;

    // Trigger a tab event (should appear)
    let tab_id = open_test_tab(h, window_id).await;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let output = collect_output(&mut child, std::time::Duration::from_secs(2)).await;
    drop(child.kill().await);

    assert!(
        output.contains("TabOpened"),
        "output should contain TabOpened events, got: {output}",
    );
    assert!(
        !output.contains("DownloadCreated") && !output.contains("DownloadChanged"),
        "output should NOT contain download events with --windows-tabs filter, got: {output}",
    );

    // Cleanup
    if tab_id > TabId(0) {
        drop(h.client().close_tab(tab_id).await);
    }
    if download_id > DownloadId(0) {
        drop(h.client().erase_download(download_id).await);
    }
}

#[tokio::test]
async fn event_stream_windows_tabs_only_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(event_stream_windows_tabs_only_body(h))
    })
    .await;
}

#[tokio::test]
async fn event_stream_windows_tabs_only_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(event_stream_windows_tabs_only_body(h))
    })
    .await;
}
