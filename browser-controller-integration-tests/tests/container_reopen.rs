//! A tab that another extension reopens in a different container stays in the
//! window it was opened in.
//!
//! Multi-Account Containers reopens a site assigned to a container by creating
//! a new tab in that container without a `windowId` and closing the original;
//! Firefox puts such a tab into the most recently activated window. The test
//! installs a minimal stand-in doing the same for the test server's second
//! page. Firefox-only, since Chrome has no containers.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are inherently outside #[cfg(test)]"
)]
#![expect(
    clippy::expect_used,
    reason = "panicking on unexpected failure is acceptable in tests"
)]

use std::time::Duration;

use browser_controller_client::OpenTabParamsBuilder;
use browser_controller_integration_tests::Harness;
use browser_controller_integration_tests::browser;
use browser_controller_integration_tests::harness;
use browser_controller_integration_tests::test_server;
use browser_controller_types::{TabDetails, WindowId};

/// The container the stand-in reopens the page in.
const CONTAINER: &str = "firefox-container-1";

/// Manifest of the Multi-Account Containers stand-in.
const REOPEN_MANIFEST: &str = r#"{
  "manifest_version": 2,
  "name": "container reopen stand-in",
  "version": "1.0",
  "browser_specific_settings": { "gecko": { "id": "container-reopen@test.invalid" } },
  "permissions": ["tabs", "cookies", "contextualIdentities"],
  "background": { "scripts": ["background.js"] }
}
"#;

/// Background script of the stand-in: like Multi-Account Containers, reopen
/// `/page2` loaded outside the container in a new tab in the container,
/// created without a `windowId`, and close the original.
const REOPEN_BACKGROUND: &str = r#"
browser.tabs.onUpdated.addListener(async (tabId, changeInfo, tab) => {
  if (!changeInfo.url || !changeInfo.url.includes("/page2")) return;
  if (tab.cookieStoreId === "firefox-container-1") return;
  await browser.tabs.create({
    url: changeInfo.url,
    cookieStoreId: "firefox-container-1",
    index: tab.index + 1,
    active: tab.active,
  });
  await browser.tabs.remove(tabId);
});
"#;

/// Write the stand-in extension into a temporary directory.
fn reopen_extension() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temporary directory");
    fs_err::write(dir.path().join("manifest.json"), REOPEN_MANIFEST).expect("write manifest");
    fs_err::write(dir.path().join("background.js"), REOPEN_BACKGROUND)
        .expect("write background script");
    dir
}

/// The tabs of a window showing the test server's second page.
async fn page2_tabs(h: &Harness, window_id: WindowId) -> Vec<TabDetails> {
    h.client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed")
        .into_iter()
        .filter(|tab| tab.url.contains("/page2"))
        .collect()
}

async fn reopened_tab_stays_in_its_window_body(h: &Harness, wait_for_load: bool) {
    let server = test_server::Server::start_plain();
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    let target = windows.first().expect("the initial window").id;
    // Opened last, the other window is the one Firefox puts tabs created
    // without a windowId into.
    let other = h
        .client()
        .open_window(None, false)
        .await
        .expect("OpenWindow should succeed");

    let mut params = OpenTabParamsBuilder::default();
    params.window_id(target).url(server.page2_url());
    if wait_for_load {
        params.wait_for_load_timeout_ms(10_000_u32);
    }
    let opened = h
        .client()
        .open_tab(params.build().expect("build OpenTabParams"))
        .await
        .expect("OpenTab should succeed");
    if wait_for_load {
        pretty_assertions::assert_eq!(
            opened.window_id,
            target,
            "the returned tab should be in the requested window"
        );
    }

    // Wait for the reopen to happen and settle.
    let mut in_target = Vec::new();
    for _ in 0..50 {
        in_target = page2_tabs(h, target).await;
        if in_target
            .iter()
            .any(|tab| tab.cookie_store_id.as_ref().map(|id| id.as_str()) == Some(CONTAINER))
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let containers: Vec<Option<String>> = in_target
        .iter()
        .map(|tab| {
            tab.cookie_store_id
                .as_ref()
                .map(|id| id.as_str().to_owned())
        })
        .collect();
    pretty_assertions::assert_eq!(
        containers,
        vec![Some(CONTAINER.to_owned())],
        "the requested window should hold just the reopened tab"
    );
    pretty_assertions::assert_eq!(
        page2_tabs(h, other).await,
        Vec::new(),
        "the reopened tab should not stay in the other window"
    );
}

#[tokio::test]
async fn reopened_tab_stays_in_its_window_firefox() {
    let extension = reopen_extension();
    harness::run_with_extensions(
        browser::Kind::Firefox,
        &[extension.path().to_path_buf()],
        |h| Box::pin(reopened_tab_stays_in_its_window_body(h, false)),
    )
    .await;
}

#[tokio::test]
async fn reopened_tab_stays_in_its_window_when_waiting_for_load_firefox() {
    let extension = reopen_extension();
    harness::run_with_extensions(
        browser::Kind::Firefox,
        &[extension.path().to_path_buf()],
        |h| Box::pin(reopened_tab_stays_in_its_window_body(h, true)),
    )
    .await;
}
