//! Which PR-stack tools each kind of caller is offered (`#orchestrators` 7/11): an orchestrator-role
//! session every one of the eleven, whether or not it owns a stack, a node of a stack only the two
//! read tools for its own stack, and every other session none. Roles alone decide; no recipe does
//! (developer, 2026-11-14). The daemon offers and serves the same
//! set: `tddy-daemon-rpc`'s `stack_plan_tools_acceptance.rs` drives it end to end.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-11-10-stack-plan-tools.md.

use pretty_assertions::assert_eq;
use tddy_tool_engine::stack_tools::{stack_tools_offered_to, StackCallerRole};

/// The tool names `role` is offered, in the order they are advertised.
fn offered_to(role: Option<&StackCallerRole>) -> Vec<String> {
    stack_tools_offered_to(role)
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

const EVERY_STACK_TOOL: [&str; 11] = [
    "pr_stack_list",
    "pr_stack_create",
    "pr_stack_status",
    "pr_read",
    "pr_search",
    "pr_comments",
    "pr_add_planned",
    "pr_update_planned",
    "pr_delete_planned",
    "pr_set_status",
    "pr_adopt",
];

#[test]
fn each_caller_kind_is_offered_its_stack_tool_set() {
    // Given one caller of each kind
    let orchestrator = StackCallerRole::Orchestrator { own_stack_id: None };
    let owning_orchestrator = StackCallerRole::Orchestrator {
        own_stack_id: Some("stack-1".to_string()),
    };
    let stack_node = StackCallerRole::StackNode {
        stack_id: "stack-1".to_string(),
    };

    // When each is asked what it is offered
    let offered = [
        offered_to(Some(&orchestrator)),
        offered_to(Some(&owning_orchestrator)),
        offered_to(Some(&stack_node)),
        offered_to(None),
    ];

    // Then both orchestrators every tool, a node the two reads, and an ordinary session nothing
    assert_eq!(
        offered,
        [
            EVERY_STACK_TOOL.map(String::from).to_vec(),
            EVERY_STACK_TOOL.map(String::from).to_vec(),
            vec!["pr_stack_status".to_string(), "pr_read".to_string()],
            Vec::<String>::new(),
        ]
    );
}
