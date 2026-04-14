//! Firefox container (contextual identity) tests.
//!
//! These tests are Firefox-only since Chrome doesn't support containers.

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
use browser_controller_integration_tests::test_server;
use browser_controller_types::{CliCommand, CliResult};

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

// --- ListContainers ---

#[expect(clippy::panic, reason = "test assertions")]
async fn list_containers_body(h: &Harness) {
    let result = h
        .send_command(CliCommand::ListContainers)
        .await
        .expect("ListContainers should succeed");
    match result {
        CliResult::Containers { containers } => {
            // Firefox has default containers (Personal, Work, Banking, Shopping)
            assert!(
                !containers.is_empty(),
                "Firefox should have at least one container",
            );
            // Verify container fields are populated
            let first = containers.first().expect("just asserted non-empty");
            assert!(
                !first.cookie_store_id.is_empty(),
                "cookie_store_id should not be empty",
            );
            assert!(!first.name.is_empty(), "name should not be empty");
            assert!(!first.color.is_empty(), "color should not be empty");
        }
        other => panic!("expected Containers, got {other:?}"),
    }
}

#[tokio::test]
async fn list_containers_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(list_containers_body(h))
    })
    .await;
}

// --- Open tab in container ---

#[expect(clippy::panic, reason = "test assertions")]
async fn open_tab_in_container_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    // Get the first container's cookie store ID
    let containers_result = h
        .send_command(CliCommand::ListContainers)
        .await
        .expect("ListContainers should succeed");
    let container_id = match &containers_result {
        CliResult::Containers { containers } => {
            assert!(!containers.is_empty(), "need at least one container");
            containers
                .first()
                .expect("just asserted")
                .cookie_store_id
                .clone()
        }
        other => panic!("expected Containers, got {other:?}"),
    };

    // Open a tab in that container
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some(server.base_url()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: Some(container_id.clone()),
        })
        .await
        .expect("OpenTab in container should succeed");

    let tab_id = match open_result {
        CliResult::Tab(details) => {
            pretty_assertions::assert_eq!(
                details.cookie_store_id.as_deref(),
                Some(container_id.as_str()),
                "tab should be in the requested container",
            );
            details.id
        }
        other => panic!("expected Tab, got {other:?}"),
    };

    // Verify via ListTabs
    let tabs_result = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    match &tabs_result {
        CliResult::Tabs { tabs } => {
            let tab = tabs
                .iter()
                .find(|t| t.id == tab_id)
                .expect("tab should exist");
            pretty_assertions::assert_eq!(
                tab.cookie_store_id.as_deref(),
                Some(container_id.as_str()),
                "ListTabs should show the container",
            );
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn open_tab_in_container_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(open_tab_in_container_body(h))
    })
    .await;
}

// --- Reopen tab in container ---

#[expect(clippy::panic, reason = "test assertions")]
async fn reopen_tab_in_container_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    // Get two different containers
    let containers_result = h
        .send_command(CliCommand::ListContainers)
        .await
        .expect("ListContainers should succeed");
    let (container1, container2) = match &containers_result {
        CliResult::Containers { containers } => {
            assert!(
                containers.len() >= 2,
                "need at least 2 containers for this test",
            );
            let c1 = containers.first().expect("just asserted len >= 2");
            let c2 = containers.get(1).expect("just asserted len >= 2");
            (c1.cookie_store_id.clone(), c2.cookie_store_id.clone())
        }
        other => panic!("expected Containers, got {other:?}"),
    };

    // Open a tab in container1
    let url = server.base_url();
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some(url.clone()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: Some(container1),
        })
        .await
        .expect("OpenTab should succeed");
    let tab_id = match open_result {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Reopen in container2
    let reopen_result = h
        .send_command(CliCommand::ReopenTabInContainer {
            tab_id,
            cookie_store_id: container2.clone(),
        })
        .await
        .expect("ReopenTabInContainer should succeed");
    let new_tab_id = match reopen_result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(
                d.cookie_store_id.as_deref(),
                Some(container2.as_str()),
                "reopened tab should be in container2",
            );
            assert!(
                d.url.starts_with(&url) || d.url == "about:blank",
                "reopened tab should have the same URL, got {}",
                d.url,
            );
            d.id
        }
        other => panic!("expected Tab, got {other:?}"),
    };

    h.send_command(CliCommand::CloseTab { tab_id: new_tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn reopen_tab_in_container_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(reopen_tab_in_container_body(h))
    })
    .await;
}

// --- cookie_store_id in tab listing (both browsers) ---

#[expect(clippy::panic, reason = "test assertions")]
async fn tab_cookie_store_id_in_listing_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open a tab
    let open_result = h
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
        .expect("OpenTab should succeed");
    let tab_id = match open_result {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    // List tabs and check cookie_store_id field exists (may be null on Chrome)
    let tabs_result = h
        .send_command(CliCommand::ListTabs { window_id })
        .await
        .expect("ListTabs should succeed");
    match &tabs_result {
        CliResult::Tabs { tabs } => {
            let tab = tabs
                .iter()
                .find(|t| t.id == tab_id)
                .expect("tab should exist");
            // On Firefox, cookie_store_id should be Some(...); on Chrome, it may be None
            match h.browser {
                browser::Kind::Firefox => {
                    assert!(
                        tab.cookie_store_id.is_some(),
                        "Firefox tabs should have a cookie_store_id",
                    );
                }
                browser::Kind::Chrome => {
                    // Chrome doesn't support containers; cookie_store_id may be None
                }
            }
        }
        other => panic!("expected Tabs, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn tab_cookie_store_id_in_listing_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(tab_cookie_store_id_in_listing_body(h))
    })
    .await;
}

#[tokio::test]
async fn tab_cookie_store_id_in_listing_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(tab_cookie_store_id_in_listing_body(h))
    })
    .await;
}

// --- CLI: containers list ---

#[expect(clippy::panic, reason = "test assertions")]
async fn cli_containers_list_body(h: &Harness) {
    let stdout = run_cli(h, &["containers", "list"]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Containers { containers } => {
            assert!(!containers.is_empty(), "should have containers");
        }
        other => panic!("expected Containers, got {other:?}"),
    }
}

#[tokio::test]
async fn cli_containers_list_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(cli_containers_list_body(h))
    })
    .await;
}

// --- CLI: --tab-container matcher ---

#[expect(clippy::panic, reason = "test assertions")]
async fn cli_tab_container_matcher_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    // Get a container
    let containers_result = h
        .send_command(CliCommand::ListContainers)
        .await
        .expect("ListContainers should succeed");
    let container_id = match &containers_result {
        CliResult::Containers { containers } => {
            assert!(!containers.is_empty(), "need a container");
            containers
                .first()
                .expect("just asserted")
                .cookie_store_id
                .clone()
        }
        other => panic!("expected Containers, got {other:?}"),
    };

    // Open tab in that container
    let open_result = h
        .send_command(CliCommand::OpenTab {
            window_id,
            insert_before_tab_id: None,
            insert_after_tab_id: None,
            url: Some(server.base_url()),
            username: None,
            password: None,
            background: true,
            cookie_store_id: Some(container_id.clone()),
        })
        .await
        .expect("OpenTab should succeed");
    let tab_id = match open_result {
        CliResult::Tab(d) => d.id,
        other => panic!("expected Tab, got {other:?}"),
    };

    // Match by container via CLI
    let w = window_id.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-container",
            &container_id,
            "--tab-window-id",
            &w,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab_id, "should match our container tab");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.send_command(CliCommand::CloseTab { tab_id })
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn cli_tab_container_matcher_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(cli_tab_container_matcher_body(h))
    })
    .await;
}
