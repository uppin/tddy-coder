//! The work-item link tools (`#workqueue` 6/14) and what a link means for access: a session linked to
//! an item is addressed by the queue (`#workqueue` 5/14's rule, extended here).

use pretty_assertions::assert_eq;
use serde_json::Value;
use tddy_core::changeset::{WorkQueue, WorkItem};
use tddy_tool_engine::work_item_link_tools::{
    work_item_link_tool_defs, WORK_ITEM_ASSIGN_PR, WORK_ITEM_LINK_SESSION,
    WORK_ITEM_LINK_TOOL_NAMES, WORK_ITEM_UNASSIGN_PR, WORK_ITEM_UNLINK_SESSION,
};
use tddy_tool_engine::work_queue_access::{work_queue_access_of, WorkQueueAccess};

fn required_of(name: &str) -> Vec<String> {
    let def = work_item_link_tool_defs()
        .into_iter()
        .find(|def| def.name == name)
        .unwrap_or_else(|| panic!("{name} is defined"));
    let schema: Value = serde_json::from_str(&def.input_schema_json).expect("a JSON schema");
    let mut required: Vec<String> =
        serde_json::from_value(schema["required"].clone()).unwrap_or_default();
    required.sort();
    required
}

#[test]
fn the_link_tools_are_defined_in_order() {
    // When
    let names: Vec<String> = work_item_link_tool_defs()
        .into_iter()
        .map(|def| def.name)
        .collect();

    // Then
    assert_eq!(names, WORK_ITEM_LINK_TOOL_NAMES.map(String::from).to_vec());
}

#[test]
fn the_link_tools_take_an_item_and_a_pr_or_a_session() {
    assert_eq!(
        [
            required_of(WORK_ITEM_ASSIGN_PR),
            required_of(WORK_ITEM_UNASSIGN_PR),
            required_of(WORK_ITEM_LINK_SESSION),
            required_of(WORK_ITEM_UNLINK_SESSION),
        ],
        [
            vec!["item_id".to_string(), "pull_number".to_string()],
            vec!["item_id".to_string()],
            vec!["item_id".to_string(), "session_id".to_string()],
            vec!["item_id".to_string(), "session_id".to_string()],
        ]
    );
}

#[test]
fn a_session_linked_to_an_item_is_addressed_by_its_queue() {
    // Given — n1's item was linked to sess-reviewer (the link lives on the item from /green on; the
    // queue here is built the way `link_session` leaves it)
    let mut queue = WorkQueue {
        version: 1,
        nodes: vec![WorkItem {
            node_id: "n1".to_string(),
            ..WorkItem::default()
        }],
    };
    link_through_the_queue_crate(&mut queue, "n1", "sess-reviewer");

    // When
    let access = work_queue_access_of("sess-reviewer", "sess-owner", &queue);

    // Then
    assert_eq!(access, Some(WorkQueueAccess::Addressed));
}

/// Link through the queue crate's own operation, so this test needs no knowledge of where the link is
/// stored.
fn link_through_the_queue_crate(queue: &mut WorkQueue, item_id: &str, session_id: &str) {
    tddy_work_queue::item_ops::item_links::link_session(
        queue,
        item_id,
        tddy_work_queue::item_ops::item_links::LinkedSession {
            session_id: session_id.to_string(),
            daemon_instance_id: String::new(),
        },
    )
    .expect("linked");
}
