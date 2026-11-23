//! Unit tests: which stack a PR-stack request names, and the drawer's summary of one stack record
//! (`#orchestrators` 9/11).
//!
//! Changeset: docs/dev/1-WIP/2026-11-10-stack-web-by-id.md

use pretty_assertions::assert_eq;
use tddy_core::changeset::{GithubPrStatus, WorkItem, WorkQueue};
use tddy_core::work_queue_store::WorkQueueRecord;
use tddy_rpc::Code;
use tddy_service::proto::work_queue::{WorkItemCounts, WorkQueueAddress, WorkQueueSummary};

use super::{node_counts_of, summary_of, work_queue_target, WorkQueueTarget};

const PROJECT: &str = "proj-auth";
const STACK: &str = "0190b000-0000-7000-8000-0000000000aa";
const HOST: &str = "host-a";

fn an_address() -> WorkQueueAddress {
    WorkQueueAddress {
        project_id: PROJECT.to_string(),
        queue_id: STACK.to_string(),
        daemon_instance_id: HOST.to_string(),
    }
}

/// A node titled after `node_id`, with whatever `shape` gives it.
fn a_node(node_id: &str, shape: impl FnOnce(&mut WorkItem)) -> WorkItem {
    let mut node = WorkItem {
        node_id: node_id.to_string(),
        title: format!("PR {node_id}"),
        ..WorkItem::default()
    };
    shape(&mut node);
    node
}

fn merged() -> Option<GithubPrStatus> {
    Some(GithubPrStatus {
        phase: "merged".to_string(),
        url: None,
        error: None,
    })
}

/// Four nodes: one only planned, one with a child session, one with a branch, one merged.
fn a_stack_part_way_through() -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: vec![
            a_node("n1", |_| {}),
            a_node("n2", |n| n.session_id = Some("child-2".to_string())),
            a_node("n3", |n| n.branch = Some("feature/n3".to_string())),
            a_node("n4", |n| {
                n.branch = Some("feature/n4".to_string());
                n.session_id = Some("child-4".to_string());
                n.pr_status = merged();
            }),
        ],
    }
}

fn a_record(title: Option<&str>, stack: WorkQueue) -> WorkQueueRecord {
    WorkQueueRecord {
        queue_id: STACK.to_string(),
        project_id: PROJECT.to_string(),
        created_by_session_id: "planner-1".to_string(),
        created_at_unix_ms: 1_762_000_000_000,
        title: title.map(str::to_string),
        stack,
    }
}

// ── work_queue_target ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_request_carrying_an_address_names_that_stack() {
    // When
    let target = work_queue_target(Some(&an_address()), "");

    // Then
    assert_eq!(target.ok(), Some(WorkQueueTarget::Address(an_address())));
}

#[test]
fn an_address_wins_over_a_session_id_carried_beside_it() {
    // When
    let target = work_queue_target(Some(&an_address()), "orchestrator-1");

    // Then
    assert_eq!(target.ok(), Some(WorkQueueTarget::Address(an_address())));
}

#[test]
fn a_request_carrying_only_a_session_id_names_that_session() {
    // When
    let target = work_queue_target(None, " orchestrator-1 ");

    // Then
    assert_eq!(
        target.ok(),
        Some(WorkQueueTarget::Session("orchestrator-1".to_string()))
    );
}

#[test]
fn a_request_naming_neither_an_address_nor_a_session_is_refused() {
    // When
    let refused = work_queue_target(None, "  ");

    // Then
    assert_eq!(refused.err().map(|s| s.code), Some(Code::InvalidArgument));
}

#[test]
fn an_address_missing_its_stack_id_is_refused_rather_than_completed() {
    // Given
    let address = WorkQueueAddress {
        queue_id: " ".to_string(),
        ..an_address()
    };

    // When
    let refused = work_queue_target(Some(&address), "orchestrator-1");

    // Then — not quietly read as the session's stack
    assert_eq!(refused.err().map(|s| s.code), Some(Code::InvalidArgument));
}

#[test]
fn an_address_missing_its_project_is_refused() {
    // Given
    let address = WorkQueueAddress {
        project_id: String::new(),
        ..an_address()
    };

    // When
    let refused = work_queue_target(Some(&address), "");

    // Then
    assert_eq!(refused.err().map(|s| s.code), Some(Code::InvalidArgument));
}

#[test]
fn an_address_naming_no_host_is_this_hosts_and_is_accepted() {
    // Given
    let address = WorkQueueAddress {
        daemon_instance_id: String::new(),
        ..an_address()
    };

    // When
    let target = work_queue_target(Some(&address), "");

    // Then
    assert_eq!(target.ok(), Some(WorkQueueTarget::Address(address)));
}

// ── node_counts_of ───────────────────────────────────────────────────────────────────────────

#[test]
fn node_counts_count_every_node_the_started_ones_and_the_merged_ones() {
    // When
    let counts = node_counts_of(&a_stack_part_way_through());

    // Then — n2 (session), n3 (branch) and n4 are started; n4 is merged
    assert_eq!(
        counts,
        WorkItemCounts {
            total: 4,
            started: 3,
            merged: 1,
        }
    );
}

#[test]
fn an_empty_stack_counts_nothing() {
    // When
    let counts = node_counts_of(&WorkQueue::default());

    // Then
    assert_eq!(counts, WorkItemCounts::default());
}

// ── summary_of ───────────────────────────────────────────────────────────────────────────────

#[test]
fn a_summary_carries_the_records_address_title_counts_creator_and_referencing_sessions() {
    // Given
    let record = a_record(Some("Auth rework"), a_stack_part_way_through());

    // When
    let summary = summary_of(&record, HOST, vec!["orchestrator-1".to_string()]);

    // Then
    assert_eq!(
        summary,
        WorkQueueSummary {
            address: Some(an_address()),
            title: "Auth rework".to_string(),
            node_counts: Some(WorkItemCounts {
                total: 4,
                started: 3,
                merged: 1,
            }),
            referencing_session_ids: vec!["orchestrator-1".to_string()],
            created_by_session_id: "planner-1".to_string(),
            created_at_unix_ms: 1_762_000_000_000,
        }
    );
}

#[test]
fn an_untitled_record_is_summarised_with_an_empty_title() {
    // Given
    let record = a_record(None, WorkQueue::default());

    // When
    let summary = summary_of(&record, HOST, Vec::new());

    // Then — still addressed, so it can be opened
    assert_eq!(summary.title, "");
    assert_eq!(summary.address, Some(an_address()));
}
