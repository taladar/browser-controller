//! Smoke test: proves the full stack works end-to-end.

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

/// Shared smoke test body: sends `GetBrowserInfo` and verifies the response.
#[expect(
    clippy::panic,
    reason = "test assertions use panic on unexpected variants"
)]
async fn smoke_body(h: &Harness) {
    let result = h.send_command(CliCommand::GetBrowserInfo).await;
    let result = result.expect("GetBrowserInfo should succeed");

    match result {
        CliResult::BrowserInfo(info) => {
            match h.browser {
                browser::Kind::Firefox => assert!(
                    info.browser_name.contains("Firefox"),
                    "expected browser_name containing Firefox, got {}",
                    info.browser_name,
                ),
                browser::Kind::Chrome => assert!(
                    !info.browser_name.contains("Firefox"),
                    "expected non-Firefox browser_name for Chrome, got {}",
                    info.browser_name,
                ),
            }
            assert!(info.pid > 0, "browser PID should be non-zero");
            assert!(
                !info.browser_version.is_empty(),
                "browser version should not be empty",
            );
        }
        other => panic!("expected BrowserInfo, got {other:?}"),
    }

    // If niri is available, verify the browser window exists
    if browser_controller_integration_tests::niri::is_available()
        && let Some(pid) = h.browser_pid
    {
        let count = browser_controller_integration_tests::niri::count_windows_for_pid(pid)
            .expect("niri window count should succeed");
        assert!(
            count >= 1,
            "expected at least 1 window for browser PID {pid}, got {count}",
        );
    }
}

#[tokio::test]
async fn smoke_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(smoke_body(h))).await;
}

#[tokio::test]
async fn smoke_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(smoke_body(h))).await;
}
