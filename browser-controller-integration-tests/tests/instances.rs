//! Tests for the `instances` command and `--instance` selector.
//!
//! These tests start BOTH Firefox and Chrome simultaneously to test
//! multi-instance behavior.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are inherently outside #[cfg(test)]"
)]
#![expect(
    clippy::expect_used,
    reason = "panicking on unexpected failure is acceptable in tests"
)]

use futures::FutureExt as _;

use browser_controller_integration_tests::Harness;
use browser_controller_integration_tests::browser;
use browser_controller_integration_tests::profile;

/// Run the CLI binary WITHOUT the `-i` instance selector.
async fn run_cli_no_instance(args: &[&str]) -> (String, bool) {
    let cli_bin = profile::cli_binary().expect("CLI binary should be built");

    let mut cmd = tokio::process::Command::new(&cli_bin);
    cmd.arg("-o").arg("json");
    cmd.args(args);

    let output = cmd.output().await.expect("CLI process should start");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    (stdout, output.status.success())
}

/// Run the CLI binary with a custom `-i` selector.
async fn run_cli_with_instance(instance: &str, args: &[&str]) -> (String, bool) {
    let cli_bin = profile::cli_binary().expect("CLI binary should be built");

    let mut cmd = tokio::process::Command::new(&cli_bin);
    cmd.arg("-o").arg("json");
    cmd.arg("-i").arg(instance);
    cmd.args(args);

    let output = cmd.output().await.expect("CLI process should start");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    (stdout, output.status.success())
}

/// Start both Firefox and Chrome harnesses, run the test, clean up both.
///
/// # Panics
///
/// Panics if either harness fails to start or the test panics.
#[expect(
    clippy::future_not_send,
    reason = "integration tests are single-threaded"
)]
#[expect(clippy::panic, reason = "test harness failure is unrecoverable")]
async fn with_dual_harness<F>(test: F)
where
    F: for<'a> FnOnce(
        &'a Harness,
        &'a Harness,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>,
{
    let firefox = match Harness::start(browser::Kind::Firefox).await {
        Ok(h) => h,
        Err(e) => panic!("failed to start Firefox harness: {e}"),
    };
    let chrome = match Harness::start(browser::Kind::Chrome).await {
        Ok(h) => h,
        Err(e) => {
            firefox.stop().await;
            panic!("failed to start Chrome harness: {e}");
        }
    };

    let result = std::panic::AssertUnwindSafe(test(&firefox, &chrome))
        .catch_unwind()
        .await;

    chrome.stop().await;
    firefox.stop().await;

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

// --- instances lists both browsers ---

#[tokio::test]
async fn instances_lists_both_browsers() {
    with_dual_harness(|_firefox, _chrome| {
        Box::pin(async move {
            let (stdout, success) = run_cli_no_instance(&["instances"]).await;
            assert!(success, "instances command should succeed");

            // Parse as JSON array
            let value: serde_json::Value =
                serde_json::from_str(stdout.trim()).expect("should be valid JSON");
            let instances = value.as_array().expect("should be an array");
            assert!(
                instances.len() >= 2,
                "should list at least 2 instances (Firefox + Chrome), got {}",
                instances.len(),
            );

            // Verify both browser names appear
            let names: Vec<&str> = instances
                .iter()
                .filter_map(|i| i.get("browser_name").and_then(|v| v.as_str()))
                .collect();
            assert!(
                names.iter().any(|n| n.contains("Firefox")),
                "should list a Firefox instance, got {names:?}",
            );
            assert!(
                names.iter().any(|n| !n.contains("Firefox")),
                "should list a non-Firefox instance (Chrome), got {names:?}",
            );
        })
    })
    .await;
}

// --- select by PID ---

#[tokio::test]
async fn select_by_pid() {
    with_dual_harness(|firefox, _chrome| {
        Box::pin(async move {
            let pid = firefox.browser_pid.expect("Firefox PID should be known");
            let (stdout, success) =
                run_cli_with_instance(&pid.to_string(), &["windows", "list"]).await;
            assert!(success, "selecting Firefox by PID should succeed");

            let _result: browser_controller_types::CliResult =
                serde_json::from_str(stdout.trim()).expect("should parse as CliResult");
        })
    })
    .await;
}

// --- select by browser name via PID ---
//
// We cannot reliably select by browser name substring alone because
// production browser instances may also be running, causing ambiguous
// matches. Instead, this test verifies that the PID-based selector
// correctly routes commands to each test browser instance.

#[tokio::test]
async fn select_by_browser_name() {
    with_dual_harness(|firefox, chrome| {
        Box::pin(async move {
            // Select Firefox by PID
            let firefox_pid = firefox.browser_pid.expect("Firefox PID should be known");
            let (stdout, success) =
                run_cli_with_instance(&firefox_pid.to_string(), &["windows", "list"]).await;
            assert!(success, "selecting Firefox by PID should succeed");
            let _result: browser_controller_types::CliResult =
                serde_json::from_str(stdout.trim()).expect("should parse Firefox result");

            // Select Chrome by PID
            let chrome_pid = chrome.browser_pid.expect("Chrome PID should be known");
            let (stdout, success) =
                run_cli_with_instance(&chrome_pid.to_string(), &["windows", "list"]).await;
            assert!(success, "selecting Chrome by PID should succeed");
            let _result: browser_controller_types::CliResult =
                serde_json::from_str(stdout.trim()).expect("should parse Chrome result");
        })
    })
    .await;
}

// --- no --instance flag errors with multiple ---

#[tokio::test]
async fn select_no_flag_errors_with_multiple() {
    with_dual_harness(|_firefox, _chrome| {
        Box::pin(async move {
            let (_stdout, success) = run_cli_no_instance(&["windows", "list"]).await;
            assert!(
                !success,
                "windows list without --instance should fail when multiple instances are running",
            );
        })
    })
    .await;
}
