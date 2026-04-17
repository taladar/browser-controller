//! Chrome tab group tests.
//!
//! These tests are Chrome-only since Firefox doesn't support tab groups.

#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are inherently outside #[cfg(test)]"
)]
#![expect(
    clippy::expect_used,
    reason = "panicking on unexpected failure is acceptable in tests"
)]
#![expect(
    clippy::panic,
    reason = "test assertions use panic! for non-assert failures"
)]

use browser_controller_client::OpenTabParamsBuilder;
use browser_controller_integration_tests::Harness;
use browser_controller_integration_tests::browser;
use browser_controller_integration_tests::harness;
use browser_controller_integration_tests::profile;
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

/// Helper: get the first window ID.
async fn first_window_id(h: &Harness) -> WindowId {
    let windows = h
        .client()
        .list_windows()
        .await
        .expect("ListWindows should succeed");
    assert!(!windows.is_empty(), "need at least 1 window");
    windows.first().expect("just asserted non-empty").id
}

// --- ListTabGroups ---

async fn list_tab_groups_body(h: &Harness) {
    // Initially there may be no tab groups; just verify the call succeeds.
    let groups = h
        .client()
        .list_tab_groups(None)
        .await
        .expect("ListTabGroups should succeed");

    // groups may be empty — that's fine, the call itself working is the test.
    // If there happen to be pre-existing groups, verify the fields are populated.
    for g in &groups {
        assert!(g.id.as_u32() > 0, "group id should be positive");
        assert!(g.window_id.as_u32() > 0, "group should belong to a window");
    }
}

#[tokio::test]
async fn list_tab_groups_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(list_tab_groups_body(h))).await;
}

// --- GroupTabs: create a new group ---

async fn group_tabs_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open two tabs to group together.
    let tab1 = h
        .client()
        .open_tab(
            OpenTabParamsBuilder::default()
                .window_id(window_id)
                .url("about:blank")
                .background(true)
                .build()
                .expect("build params"),
        )
        .await
        .expect("OpenTab 1 should succeed");
    let tab2 = h
        .client()
        .open_tab(
            OpenTabParamsBuilder::default()
                .window_id(window_id)
                .url("about:blank")
                .background(true)
                .build()
                .expect("build params"),
        )
        .await
        .expect("OpenTab 2 should succeed");

    // Group both tabs into a new group (no group_id → creates new group).
    let group = h
        .client()
        .group_tabs(vec![tab1.id, tab2.id], None)
        .await
        .expect("GroupTabs should succeed");

    assert!(group.id.as_u32() > 0, "new group should have a valid ID");
    pretty_assertions::assert_eq!(
        group.window_id,
        window_id,
        "group should be in the same window",
    );

    // Verify group appears in list.
    let groups = h
        .client()
        .list_tab_groups(Some(window_id))
        .await
        .expect("ListTabGroups should succeed");
    assert!(
        groups.iter().any(|g| g.id == group.id),
        "newly created group should appear in list",
    );

    // Verify the tabs report the group_id.
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    for tab_id in [tab1.id, tab2.id] {
        let tab = tabs.iter().find(|t| t.id == tab_id).expect("tab exists");
        pretty_assertions::assert_eq!(
            tab.group_id,
            Some(group.id),
            "tab {tab_id} should report the group_id",
        );
    }
}

#[tokio::test]
async fn group_tabs_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(group_tabs_body(h))).await;
}

// --- UpdateTabGroup ---

async fn update_tab_group_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open a tab and group it.
    let tab = h
        .client()
        .open_tab(
            OpenTabParamsBuilder::default()
                .window_id(window_id)
                .url("about:blank")
                .background(true)
                .build()
                .expect("build params"),
        )
        .await
        .expect("OpenTab should succeed");
    let group = h
        .client()
        .group_tabs(vec![tab.id], None)
        .await
        .expect("GroupTabs should succeed");

    // Update the group title and color.
    let updated = h
        .client()
        .update_tab_group(
            group.id,
            Some("Test Group".to_owned()),
            Some(browser_controller_client::TabGroupColor::Blue),
            Some(true),
        )
        .await
        .expect("UpdateTabGroup should succeed");

    pretty_assertions::assert_eq!(updated.title, "Test Group", "title should be updated");
    pretty_assertions::assert_eq!(
        updated.color,
        browser_controller_client::TabGroupColor::Blue,
        "color should be updated",
    );
    assert!(updated.collapsed, "group should be collapsed");

    // Verify via GetTabGroup.
    let fetched = h
        .client()
        .get_tab_group(group.id)
        .await
        .expect("GetTabGroup should succeed");
    pretty_assertions::assert_eq!(
        fetched.title,
        "Test Group",
        "GetTabGroup title should match"
    );
    assert!(fetched.collapsed, "GetTabGroup collapsed should match");
}

#[tokio::test]
async fn update_tab_group_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(update_tab_group_body(h))
    })
    .await;
}

// --- UngroupTabs ---

async fn ungroup_tabs_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open a tab and group it.
    let tab = h
        .client()
        .open_tab(
            OpenTabParamsBuilder::default()
                .window_id(window_id)
                .url("about:blank")
                .background(true)
                .build()
                .expect("build params"),
        )
        .await
        .expect("OpenTab should succeed");
    let group = h
        .client()
        .group_tabs(vec![tab.id], None)
        .await
        .expect("GroupTabs should succeed");

    // Verify tab is in the group.
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let t = tabs.iter().find(|t| t.id == tab.id).expect("tab exists");
    pretty_assertions::assert_eq!(
        t.group_id,
        Some(group.id),
        "tab should be in the group before ungrouping",
    );

    // Ungroup.
    h.client()
        .ungroup_tabs(vec![tab.id])
        .await
        .expect("UngroupTabs should succeed");

    // Verify tab is no longer in a group.
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    let t = tabs.iter().find(|t| t.id == tab.id).expect("tab exists");
    pretty_assertions::assert_eq!(
        t.group_id,
        None,
        "tab should not be in a group after ungrouping",
    );
}

#[tokio::test]
async fn ungroup_tabs_chrome() {
    harness::run(browser::Kind::Chrome, |h| Box::pin(ungroup_tabs_body(h))).await;
}

// --- Add tab to existing group ---

async fn add_tab_to_existing_group_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open two tabs.
    let tab1 = h
        .client()
        .open_tab(
            OpenTabParamsBuilder::default()
                .window_id(window_id)
                .url("about:blank")
                .background(true)
                .build()
                .expect("build params"),
        )
        .await
        .expect("OpenTab 1 should succeed");
    let tab2 = h
        .client()
        .open_tab(
            OpenTabParamsBuilder::default()
                .window_id(window_id)
                .url("about:blank")
                .background(true)
                .build()
                .expect("build params"),
        )
        .await
        .expect("OpenTab 2 should succeed");

    // Group tab1 into a new group.
    let group = h
        .client()
        .group_tabs(vec![tab1.id], None)
        .await
        .expect("GroupTabs should succeed");

    // Add tab2 to the existing group.
    let updated_group = h
        .client()
        .group_tabs(vec![tab2.id], Some(group.id))
        .await
        .expect("GroupTabs (add to existing) should succeed");
    pretty_assertions::assert_eq!(updated_group.id, group.id, "should be the same group",);

    // Verify both tabs are in the group.
    let tabs = h
        .client()
        .list_tabs(window_id)
        .await
        .expect("ListTabs should succeed");
    for tab_id in [tab1.id, tab2.id] {
        let t = tabs.iter().find(|t| t.id == tab_id).expect("tab exists");
        pretty_assertions::assert_eq!(
            t.group_id,
            Some(group.id),
            "tab {tab_id} should be in the group",
        );
    }
}

#[tokio::test]
async fn add_tab_to_existing_group_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(add_tab_to_existing_group_body(h))
    })
    .await;
}

// --- CLI: tab-groups list ---

async fn cli_tab_groups_list_body(h: &Harness) {
    let stdout = run_cli(h, &["tab-groups", "list"]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse");
    match result {
        CliResult::TabGroups { .. } => {
            // Success — the command worked. Groups may be empty.
        }
        other => panic!("expected TabGroups, got {other:?}"),
    }
}

#[tokio::test]
async fn cli_tab_groups_list_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(cli_tab_groups_list_body(h))
    })
    .await;
}

// --- CLI: tab-groups group + update + get ---

async fn cli_tab_groups_lifecycle_body(h: &Harness) {
    let window_id = first_window_id(h).await;

    // Open a tab via client API.
    let tab = h
        .client()
        .open_tab(
            OpenTabParamsBuilder::default()
                .window_id(window_id)
                .url("about:blank")
                .background(true)
                .build()
                .expect("build params"),
        )
        .await
        .expect("OpenTab should succeed");

    let tab_id_str = tab.id.to_string();

    // Group it via CLI.
    let stdout = run_cli(h, &["tab-groups", "group", "--tab-id", &tab_id_str]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse group result");
    let group_id = match result {
        CliResult::TabGroup(g) => g.id,
        other => panic!("expected TabGroup, got {other:?}"),
    };

    let group_id_str = group_id.to_string();

    // Update via CLI.
    let stdout = run_cli(
        h,
        &[
            "tab-groups",
            "update",
            "--group-id",
            &group_id_str,
            "--title",
            "CLI Test",
            "--color",
            "red",
        ],
    )
    .await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse update result");
    match result {
        CliResult::TabGroup(g) => {
            pretty_assertions::assert_eq!(g.title, "CLI Test", "title should be updated via CLI");
        }
        other => panic!("expected TabGroup, got {other:?}"),
    }

    // Get via CLI.
    let stdout = run_cli(h, &["tab-groups", "get", "--group-id", &group_id_str]).await;
    let result: CliResult = serde_json::from_str(stdout.trim()).expect("parse get result");
    match result {
        CliResult::TabGroup(g) => {
            pretty_assertions::assert_eq!(g.id, group_id, "get should return the same group");
            pretty_assertions::assert_eq!(g.title, "CLI Test", "title should persist");
        }
        other => panic!("expected TabGroup, got {other:?}"),
    }

    // Cleanup: ungroup via CLI, close tab via client.
    run_cli(h, &["tab-groups", "ungroup", "--tab-id", &tab_id_str]).await;
}

#[tokio::test]
async fn cli_tab_groups_lifecycle_chrome() {
    harness::run(browser::Kind::Chrome, |h| {
        Box::pin(cli_tab_groups_lifecycle_body(h))
    })
    .await;
}

// --- Firefox: tab groups commands should return errors ---

async fn tab_groups_unsupported_on_firefox_body(h: &Harness) {
    let result = h.client().list_tab_groups(None).await;
    assert!(result.is_err(), "ListTabGroups should fail on Firefox",);
}

#[tokio::test]
async fn tab_groups_unsupported_on_firefox() {
    harness::run(browser::Kind::Firefox, |h| {
        Box::pin(tab_groups_unsupported_on_firefox_body(h))
    })
    .await;
}
