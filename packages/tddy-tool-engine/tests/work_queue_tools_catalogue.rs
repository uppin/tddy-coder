//! Which work tools each access is offered (`#workqueue` 5/14): the owner of a queue every one of the
//! ten, an addressed session the two read tools, and a session with no access none. The queue decides;
//! no role does. The daemon offers and serves the same set: `tddy-daemon-rpc`'s
//! `stack_plan_tools_acceptance.rs` drives it end to end.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-session-work-queue.md.

use pretty_assertions::assert_eq;
use tddy_tool_engine::work_queue_access::{work_queue_tools_offered_to, WorkQueueAccess};

/// The plan and read tool names offered for `access`, in the order they are advertised.
fn offered_to(access: Option<WorkQueueAccess>) -> Vec<String> {
    work_queue_tools_offered_to(access)
        .into_iter()
        .map(|tool| tool.name)
        .filter(|name| !name.starts_with("work_item_m") && !is_a_git_tool(name))
        .collect()
}

fn is_a_git_tool(name: &str) -> bool {
    matches!(
        name,
        "work_item_merge"
            | "work_item_close"
            | "work_item_repoint"
            | "work_item_set_parents"
            | "work_item_resolve_conflicts"
            | "work_item_spawn_session"
    )
}

const EVERY_PLAN_TOOL: [&str; 10] = [
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
];

#[test]
fn each_access_is_offered_its_work_tool_set() {
    // When each access is asked what it is offered
    let offered = [
        offered_to(Some(WorkQueueAccess::Owner)),
        offered_to(Some(WorkQueueAccess::Addressed)),
        offered_to(None),
    ];

    // Then the owner every tool, an addressed session the two reads, and no access nothing
    assert_eq!(
        offered,
        [
            EVERY_PLAN_TOOL.map(String::from).to_vec(),
            vec!["work_queue_status".to_string(), "work_pr_read".to_string()],
            Vec::<String>::new(),
        ]
    );
}
