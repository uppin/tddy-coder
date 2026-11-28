//! `#workqueue` 3/14: the agent tools, crates and packages speak the work-queue vocabulary — a
//! *work item* in a *work queue* — with no alias kept for an old `pr_*` name. This file reads only the
//! catalogue and the packages directory, so it compiles the same before and after the rename.
//!
//! `#workqueue` 5/14 removed `work_queue_create`: a queue is created only at session start.

use std::path::Path;

use tddy_tool_engine::project_home_tools::project_home_offered_catalog;

const EVERY_WORK_QUEUE_TOOL: [&str; 19] = [
    "work_queue_list",
    "work_queue_status",
    "work_pr_read",
    "work_pr_search",
    "work_pr_comments",
    "work_item_add",
    "work_item_update",
    "work_item_delete",
    "work_item_set_status",
    "work_item_adopt",
    "work_item_merge",
    "work_item_close",
    "work_item_repoint",
    "work_item_set_parents",
    "work_item_resolve_conflicts",
    "work_item_spawn_session",
    "work_item_attach_file",
    "work_item_detach_file",
    "work_item_list_files",
];

fn offered_tool_names() -> Vec<String> {
    project_home_offered_catalog()
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

#[test]
fn every_work_queue_tool_is_offered_under_its_work_name() {
    // When
    let offered = offered_tool_names();

    // Then
    let missing: Vec<&str> = EVERY_WORK_QUEUE_TOOL
        .into_iter()
        .filter(|name| !offered.iter().any(|o| o == name))
        .collect();
    assert!(missing.is_empty(), "not offered: {missing:?}");
}

#[test]
fn no_tool_in_the_catalogue_is_named_pr_underscore() {
    // When
    let offered = offered_tool_names();

    // Then
    let old: Vec<&String> = offered
        .iter()
        .filter(|name| name.starts_with("pr_"))
        .collect();
    assert!(old.is_empty(), "still offered under a pr_ name: {old:?}");
}

#[test]
fn the_crates_and_packages_carry_the_work_queue_names() {
    // Given
    let packages = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the packages directory");

    // When
    let present = |name: &str| packages.join(name).is_dir();

    // Then
    assert_eq!(
        [
            present("tddy-work-queue"),
            present("tddy-workqueue-web"),
            present("tddy-pr-stack"),
            present("tddy-prstack-web"),
        ],
        [true, true, false, false],
        "[tddy-work-queue, tddy-workqueue-web, tddy-pr-stack, tddy-prstack-web] present"
    );
}
