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

use browser_controller_client::OpenTabParams;
use browser_controller_integration_tests::Harness;
use browser_controller_integration_tests::browser;
use browser_controller_integration_tests::harness;
use browser_controller_integration_tests::profile;
use browser_controller_integration_tests::test_server;
use browser_controller_types::{CliResult, WindowId};

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

async fn first_window_id(h: &Harness) -> WindowId {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    windows.first().expect("just asserted non-empty").id
}

// --- ListContainers ---

async fn list_containers_body(h: &Harness) {
    let containers = h
        .client()
        .list_containers()
        .await
        .expect("ListContainers should succeed");

    // Firefox has default containers (Personal, Work, Banking, Shopping)
    assert!(
        !containers.is_empty(),
        "Firefox should have at least one container",
    );
    // Verify container fields are populated
    let first = containers.first().expect("just asserted non-empty");
    assert!(
        !first.cookie_store_id.0.is_empty(),
        "cookie_store_id should not be empty",
    );
    assert!(!first.name.is_empty(), "name should not be empty");
    assert!(!first.color.is_empty(), "color should not be empty");
}

#[tokio::test]
async fn list_containers_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(list_containers_body(h))
    })
    .await;
}

// --- Open tab in container ---

async fn open_tab_in_container_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    // Get the first container's cookie store ID
    let containers = h
        .client()
        .list_containers()
        .await
        .expect("ListContainers should succeed");
    assert!(!containers.is_empty(), "need at least one container");
    let container_id = containers
        .first()
        .expect("just asserted")
        .cookie_store_id
        .clone();

    // Open a tab in that container
    let mut params = OpenTabParams::new(window_id);
    params.url = Some(server.base_url());
    params.background = true;
    params.cookie_store_id = Some(container_id.clone());
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab in container should succeed");

    pretty_assertions::assert_eq!(
        tab.cookie_store_id,
        Some(container_id.clone()),
        "tab should be in the requested container",
    );
    let tab_id = tab.id;

    // Verify via ListTabs
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let tab = tabs
        .iter()
        .find(|t| t.id == tab_id)
        .expect("tab should exist");
    pretty_assertions::assert_eq!(
        tab.cookie_store_id,
        Some(container_id.clone()),
        "ListTabs should show the container",
    );

    h.client().close_tab(tab_id).await.expect("cleanup");
}

#[tokio::test]
async fn open_tab_in_container_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(open_tab_in_container_body(h))
    })
    .await;
}

// --- Reopen tab in container ---

async fn reopen_tab_in_container_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    // Get two different containers
    let containers = h
        .client()
        .list_containers()
        .await
        .expect("ListContainers should succeed");
    assert!(
        containers.len() >= 2,
        "need at least 2 containers for this test",
    );
    let container1 = containers
        .first()
        .expect("just asserted len >= 2")
        .cookie_store_id
        .clone();
    let container2 = containers
        .get(1)
        .expect("just asserted len >= 2")
        .cookie_store_id
        .clone();

    // Open a tab in container1
    let url = server.base_url();
    let mut params = OpenTabParams::new(window_id);
    params.url = Some(url.clone());
    params.background = true;
    params.cookie_store_id = Some(container1);
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");
    let tab_id = tab.id;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Reopen in container2
    let new_tab = h
        .client()
        .reopen_tab_in_container(tab_id, container2.clone())
        .await
        .expect("ReopenTabInContainer should succeed");

    pretty_assertions::assert_eq!(
        new_tab.cookie_store_id,
        Some(container2.clone()),
        "reopened tab should be in container2",
    );
    assert!(
        new_tab.url.starts_with(&url) || new_tab.url == "about:blank",
        "reopened tab should have the same URL, got {}",
        new_tab.url,
    );
    let new_tab_id = new_tab.id;

    h.client().close_tab(new_tab_id).await.expect("cleanup");
}

#[tokio::test]
async fn reopen_tab_in_container_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(reopen_tab_in_container_body(h))
    })
    .await;
}

// --- cookie_store_id in tab listing (both browsers) ---

async fn tab_cookie_store_id_in_listing_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open a tab
    let mut params = OpenTabParams::new(window_id);
    params.url = Some("about:blank".to_owned());
    params.background = true;
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");
    let tab_id = tab.id;

    // List tabs and check cookie_store_id field exists (may be null on Chrome)
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
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

    h.client().close_tab(tab_id).await.expect("cleanup");
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

// --- container_name populated in tab details ---

async fn container_name_in_tab_details_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    let containers = h
        .client()
        .list_containers()
        .await
        .expect("ListContainers should succeed");
    assert!(!containers.is_empty(), "need a container");
    let c = containers.first().expect("just asserted");
    let container_id = c.cookie_store_id.clone();
    let container_name = c.name.clone();

    let mut params = OpenTabParams::new(window_id);
    params.url = Some(server.base_url());
    params.background = true;
    params.cookie_store_id = Some(container_id);
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");

    pretty_assertions::assert_eq!(
        tab.container_name.as_deref(),
        Some(container_name.as_str()),
        "OpenTab response should include container_name",
    );
    let tab_id = tab.id;

    h.client().close_tab(tab_id).await.expect("cleanup");
}

#[tokio::test]
async fn container_name_in_tab_details_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(container_name_in_tab_details_body(h))
    })
    .await;
}

// --- CLI: --tab-cookie-store-id matcher ---

#[expect(clippy::panic, reason = "test assertions")]
async fn cli_tab_cookie_store_id_matcher_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    // Get a container
    let containers = h
        .client()
        .list_containers()
        .await
        .expect("ListContainers should succeed");
    assert!(!containers.is_empty(), "need a container");
    let container_id = containers
        .first()
        .expect("just asserted")
        .cookie_store_id
        .clone();

    // Open tab in that container
    let mut params = OpenTabParams::new(window_id);
    params.url = Some(server.base_url());
    params.background = true;
    params.cookie_store_id = Some(container_id.clone());
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");
    let tab_id = tab.id;

    // Match by container via CLI
    let w = window_id.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-cookie-store-id",
            &container_id.0,
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

    h.client().close_tab(tab_id).await.expect("cleanup");
}

#[tokio::test]
async fn cli_tab_cookie_store_id_matcher_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(cli_tab_cookie_store_id_matcher_body(h))
    })
    .await;
}

// --- CLI: --tab-container-name matcher ---

#[expect(clippy::panic, reason = "test assertions")]
async fn cli_tab_container_name_matcher_body(h: &Harness) {
    let server = test_server::Server::start_plain();
    let window_id = first_window_id(h).await;

    let containers = h
        .client()
        .list_containers()
        .await
        .expect("ListContainers should succeed");
    assert!(!containers.is_empty(), "need a container");
    let c = containers.first().expect("just asserted");
    let container_id = c.cookie_store_id.clone();
    let container_name = c.name.clone();

    let mut params = OpenTabParams::new(window_id);
    params.url = Some(server.base_url());
    params.background = true;
    params.cookie_store_id = Some(container_id);
    let tab = h
        .client()
        .open_tab(params)
        .await
        .expect("OpenTab should succeed");
    let tab_id = tab.id;

    let w = window_id.to_string();
    let stdout = run_cli(
        h,
        &[
            "tabs",
            "activate",
            "--tab-container-name",
            &container_name,
            "--tab-window-id",
            &w,
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::Tab(d) => {
            pretty_assertions::assert_eq!(d.id, tab_id, "should match by container name");
        }
        other => panic!("expected Tab, got {other:?}"),
    }

    h.client().close_tab(tab_id).await.expect("cleanup");
}

#[tokio::test]
async fn cli_tab_container_name_matcher_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(cli_tab_container_name_matcher_body(h))
    })
    .await;
}
