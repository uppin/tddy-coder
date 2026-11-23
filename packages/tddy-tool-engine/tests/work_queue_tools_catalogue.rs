//! Which PR-stack tools each kind of caller is offered (`#orchestrators` 7/11): an orchestrator-role
//! session every one of the eleven, whether or not it owns a stack, a node of a stack only the two
//! read tools for its own stack, and every other session none. Roles alone decide; no recipe does
//! (developer, 2026-11-14). The daemon offers and serves the same
//! set: `tddy-daemon-rpc`'s `stack_plan_tools_acceptance.rs` drives it end to end.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md.

use pretty_assertions::assert_eq;
use tddy_tool_engine::work_queue_tools::{work_queue_tools_offered_to, WorkQueueCallerRole};

/// The tool names `role` is offered, in the order they are advertised.
fn offered_to(role: Option<&WorkQueueCallerRole>) -> Vec<String> {
    work_queue_tools_offered_to(role)
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

const EVERY_STACK_TOOL: [&str; 11] = [
    "work_queue_list",
    "work_queue_create",
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
fn each_caller_kind_is_offered_its_stack_tool_set() {
    // Given one caller of each kind
    let orchestrator = WorkQueueCallerRole::Orchestrator { own_stack_id: None };
    let owning_orchestrator = WorkQueueCallerRole::Orchestrator {
        own_stack_id: Some("stack-1".to_string()),
    };
    let work_item = WorkQueueCallerRole::WorkItem {
        queue_id: "stack-1".to_string(),
    };

    // When each is asked what it is offered
    let offered = [
        offered_to(Some(&orchestrator)),
        offered_to(Some(&owning_orchestrator)),
        offered_to(Some(&work_item)),
        offered_to(None),
    ];

    // Then both orchestrators every tool, a node the two reads, and an ordinary session nothing
    assert_eq!(
        offered,
        [
            EVERY_STACK_TOOL.map(String::from).to_vec(),
            EVERY_STACK_TOOL.map(String::from).to_vec(),
            vec!["work_queue_status".to_string(), "work_pr_read".to_string()],
            Vec::<String>::new(),
        ]
    );
}
