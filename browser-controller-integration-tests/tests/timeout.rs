//! Tests for the `--timeout` / `-t` CLI flag.

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
use browser_controller_integration_tests::profile;

/// Run the CLI binary with custom args.
async fn run_cli(h: &Harness, args: &[&str]) -> bool {
    let cli_bin = profile::cli_binary().expect("CLI binary should be built");
    let pid = h
        .browser_pid
        .expect("browser PID should be known for CLI tests");

    let mut cmd = tokio::process::Command::new(&cli_bin);
    cmd.arg("-o").arg("json");
    cmd.arg("-i").arg(pid.to_string());
    cmd.args(args);

    let output = cmd.output().await.expect("CLI process should start");
    output.status.success()
}

async fn timeout_flag_body(h: &Harness) {
    // Explicit timeout of 30s
    assert!(
        run_cli(h, &["-t", "30", "windows", "list"]).await,
        "--timeout 30 should succeed",
    );

    // Disabled timeout (0)
    assert!(
        run_cli(h, &["-t", "0", "windows", "list"]).await,
        "--timeout 0 should succeed",
    );
}

#[tokio::test]
async fn timeout_flag_firefox() {
    harness::run(browser::Kind::Firefox, |h| Box::pin(timeout_flag_body(h))).await;
}

#[tokio::test]
async fn timeout_flag_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(timeout_flag_body(h))).await;
}
