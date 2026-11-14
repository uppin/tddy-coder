//! Unit tests: which stack a PR-stack request names, and the drawer's summary of one stack record
//! (`#orchestrators` 9/11).
//!
//! Changeset: docs/dev/1-WIP/2026-11-10-stack-web-by-id.md

use pretty_assertions::assert_eq;
use tddy_core::changeset::{GithubPrStatus, Stack, StackNode};
use tddy_core::project_stack_store::StackRecord;
use tddy_rpc::Code;
use tddy_service::proto::pr_stack::{ProjectStackSummary, StackAddress, StackNodeCounts};

use super::{node_counts_of, stack_target, summary_of, StackTarget};

const PROJECT: &str = "proj-auth";
const STACK: &str = "0190b000-0000-7000-8000-0000000000aa";
const HOST: &str = "host-a";

fn an_address() -> StackAddress {
    StackAddress {
        project_id: PROJECT.to_string(),
        stack_id: STACK.to_string(),
        daemon_instance_id: HOST.to_string(),
    }
}

/// A node titled after `node_id`, with whatever `shape` gives it.
fn a_node(node_id: &str, shape: impl FnOnce(&mut StackNode)) -> StackNode {
    let mut node = StackNode {
        node_id: node_id.to_string(),
        title: format!("PR {node_id}"),
        ..StackNode::default()
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
fn a_stack_part_way_through() -> Stack {
    Stack {
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

fn a_record(title: Option<&str>, stack: Stack) -> StackRecord {
    StackRecord {
        stack_id: STACK.to_string(),
        project_id: PROJECT.to_string(),
        created_by_session_id: "planner-1".to_string(),
        created_at_unix_ms: 1_762_000_000_000,
        title: title.map(str::to_string),
        stack,
    }
}

// ── stack_target ─────────────────────────────────────────────────────────────────────────────

#[test]
fn a_request_carrying_an_address_names_that_stack() {
    // When
    let target = stack_target(Some(&an_address()), "");

    // Then
    assert_eq!(target.ok(), Some(StackTarget::Address(an_address())));
}

#[test]
fn an_address_wins_over_a_session_id_carried_beside_it() {
    // When
    let target = stack_target(Some(&an_address()), "orchestrator-1");

    // Then
    assert_eq!(target.ok(), Some(StackTarget::Address(an_address())));
}

#[test]
fn a_request_carrying_only_a_session_id_names_that_session() {
    // When
    let target = stack_target(None, " orchestrator-1 ");

    // Then
    assert_eq!(
        target.ok(),
        Some(StackTarget::Session("orchestrator-1".to_string()))
    );
}

#[test]
fn a_request_naming_neither_an_address_nor_a_session_is_refused() {
    // When
    let refused = stack_target(None, "  ");

    // Then
    assert_eq!(refused.err().map(|s| s.code), Some(Code::InvalidArgument));
}

#[test]
fn an_address_missing_its_stack_id_is_refused_rather_than_completed() {
    // Given
    let address = StackAddress {
        stack_id: " ".to_string(),
        ..an_address()
    };

    // When
    let refused = stack_target(Some(&address), "orchestrator-1");

    // Then — not quietly read as the session's stack
    assert_eq!(refused.err().map(|s| s.code), Some(Code::InvalidArgument));
}

#[test]
fn an_address_missing_its_project_is_refused() {
    // Given
    let address = StackAddress {
        project_id: String::new(),
        ..an_address()
    };

    // When
    let refused = stack_target(Some(&address), "");

    // Then
    assert_eq!(refused.err().map(|s| s.code), Some(Code::InvalidArgument));
}

#[test]
fn an_address_naming_no_host_is_this_hosts_and_is_accepted() {
    // Given
    let address = StackAddress {
        daemon_instance_id: String::new(),
        ..an_address()
    };

    // When
    let target = stack_target(Some(&address), "");

    // Then
    assert_eq!(target.ok(), Some(StackTarget::Address(address)));
}

// ── node_counts_of ───────────────────────────────────────────────────────────────────────────

#[test]
fn node_counts_count_every_node_the_started_ones_and_the_merged_ones() {
    // When
    let counts = node_counts_of(&a_stack_part_way_through());

    // Then — n2 (session), n3 (branch) and n4 are started; n4 is merged
    assert_eq!(
        counts,
        StackNodeCounts {
            total: 4,
            started: 3,
            merged: 1,
        }
    );
}

#[test]
fn an_empty_stack_counts_nothing() {
    // When
    let counts = node_counts_of(&Stack::default());

    // Then
    assert_eq!(counts, StackNodeCounts::default());
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
        ProjectStackSummary {
            address: Some(an_address()),
            title: "Auth rework".to_string(),
            node_counts: Some(StackNodeCounts {
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
    let record = a_record(None, Stack::default());

    // When
    let summary = summary_of(&record, HOST, Vec::new());

    // Then — still addressed, so it can be opened
    assert_eq!(summary.title, "");
    assert_eq!(summary.address, Some(an_address()));
}
