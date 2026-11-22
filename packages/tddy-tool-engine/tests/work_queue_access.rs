//! Who may use a session's work queue (`#workqueue` 5/14): the owner has every work tool, a session an
//! item links to may read the queue, and nobody else may do anything — whatever their role. The access
//! functions take no role at all.

use pretty_assertions::assert_eq;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_tool_engine::work_item_annotation_tools::WORK_ITEM_ANNOTATION_TOOL_NAMES;
use tddy_tool_engine::work_item_git_tools::WORK_ITEM_GIT_TOOL_NAMES;
use tddy_tool_engine::work_item_link_tools::WORK_ITEM_LINK_TOOL_NAMES;
use tddy_tool_engine::work_queue_access::{
    work_queue_access_of, work_queue_tools_offered_to, WorkQueueAccess,
};
use tddy_tool_engine::work_queue_tools::{PR_READ, PR_STACK_STATUS, WORK_QUEUE_TOOL_NAMES};

fn a_queue_linking(sessions: &[&str]) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: sessions
            .iter()
            .enumerate()
            .map(|(i, session)| WorkItem {
                node_id: format!("n{}", i + 1),
                session_id: Some(session.to_string()),
                ..WorkItem::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn names(access: Option<WorkQueueAccess>) -> Vec<String> {
    work_queue_tools_offered_to(access)
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

#[test]
fn the_owning_session_has_full_access() {
    // When
    let access = work_queue_access_of("sess-owner", "sess-owner", &a_queue_linking(&[]));

    // Then
    assert_eq!(access, Some(WorkQueueAccess::Owner));
}

#[test]
fn a_session_an_item_links_to_is_addressed() {
    // When
    let access = work_queue_access_of(
        "sess-worker",
        "sess-owner",
        &a_queue_linking(&["sess-worker"]),
    );

    // Then
    assert_eq!(access, Some(WorkQueueAccess::Addressed));
}

#[test]
fn any_other_session_has_no_access() {
    // When — e.g. the project's main orchestrator looking at another session's queue
    let access = work_queue_access_of(
        "sess-main",
        "sess-owner",
        &a_queue_linking(&["sess-worker"]),
    );

    // Then
    assert_eq!(access, None);
}

#[test]
fn the_owner_is_offered_every_work_tool_except_creation() {
    // When
    let offered = names(Some(WorkQueueAccess::Owner));

    // Then
    let expected: Vec<String> = WORK_QUEUE_TOOL_NAMES
        .iter()
        .chain(WORK_ITEM_LINK_TOOL_NAMES.iter())
        .chain(WORK_ITEM_ANNOTATION_TOOL_NAMES.iter())
        .chain(tddy_tool_engine::work_queue_lineage_tools::WORK_QUEUE_LINEAGE_TOOL_NAMES.iter())
        .chain(tddy_tool_engine::work_queue_waves_tools::WORK_QUEUE_WAVES_TOOL_NAMES.iter())
        .chain(
            tddy_tool_engine::work_item_commit_flow_tools::WORK_ITEM_COMMIT_FLOW_TOOL_NAMES.iter(),
        )
        .chain(tddy_tool_engine::work_queue_schedule_tools::WORK_QUEUE_SCHEDULE_TOOL_NAMES.iter())
        .chain(WORK_ITEM_GIT_TOOL_NAMES.iter())
        .map(|name| name.to_string())
        .collect();
    assert_eq!(offered, expected);
}

#[test]
fn an_addressed_session_is_offered_the_reads_and_nobody_else_anything() {
    assert_eq!(
        (names(Some(WorkQueueAccess::Addressed)), names(None)),
        (
            vec![PR_STACK_STATUS.to_string(), PR_READ.to_string()],
            Vec::<String>::new()
        )
    );
}
