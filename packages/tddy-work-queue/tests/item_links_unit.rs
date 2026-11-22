//! A work item's links (`#workqueue` 6/14): an assigned PR (whose branch becomes the item's) and linked
//! tddy sessions.

use pretty_assertions::assert_eq;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_work_queue::item_ops::item_links::{
    assign_pr, assigned_pr_of, link_session, linked_sessions_of, unassign_pr, unlink_session,
    AssignedPr, ItemLinkError, LinkedSession,
};

fn a_queue_with(items: &[(&str, Option<&str>)]) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: items
            .iter()
            .map(|(id, branch)| WorkItem {
                node_id: id.to_string(),
                branch: branch.map(str::to_string),
                ..WorkItem::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn pr_7_on(branch: &str) -> AssignedPr {
    AssignedPr {
        number: 7,
        url: "https://github.com/acme/app/pull/7".to_string(),
        head_branch: branch.to_string(),
    }
}

fn a_session(id: &str) -> LinkedSession {
    LinkedSession {
        session_id: id.to_string(),
        daemon_instance_id: String::new(),
    }
}

fn branch_of(queue: &WorkQueue, item: &str) -> Option<String> {
    queue
        .nodes
        .iter()
        .find(|n| n.node_id == item)
        .and_then(|n| n.branch.clone())
}

#[test]
fn assigning_an_existing_pr_records_it_and_its_branch_on_the_item() {
    // Given
    let mut queue = a_queue_with(&[("n1", None)]);

    // When
    assign_pr(&mut queue, "n1", pr_7_on("feat/one")).expect("assigned");

    // Then
    assert_eq!(
        (assigned_pr_of(&queue, "n1"), branch_of(&queue, "n1")),
        (Some(pr_7_on("feat/one")), Some("feat/one".to_string()))
    );
}

#[test]
fn assigning_a_pr_on_another_branch_than_the_items_is_refused_naming_both() {
    // Given
    let mut queue = a_queue_with(&[("n1", Some("feat/mine"))]);

    // When
    let refused = assign_pr(&mut queue, "n1", pr_7_on("feat/theirs"));

    // Then
    assert_eq!(
        refused,
        Err(ItemLinkError::BranchMismatch {
            item_branch: "feat/mine".to_string(),
            pr_branch: "feat/theirs".to_string(),
        })
    );
    assert_eq!(assigned_pr_of(&queue, "n1"), None);
}

#[test]
fn unassigning_removes_the_pr_and_keeps_the_branch() {
    // Given
    let mut queue = a_queue_with(&[("n1", None)]);
    assign_pr(&mut queue, "n1", pr_7_on("feat/one")).expect("assigned");

    // When
    unassign_pr(&mut queue, "n1").expect("unassigned");

    // Then
    assert_eq!(
        (assigned_pr_of(&queue, "n1"), branch_of(&queue, "n1")),
        (None, Some("feat/one".to_string()))
    );
}

#[test]
fn linking_a_session_twice_links_it_once() {
    // Given
    let mut queue = a_queue_with(&[("n1", None)]);

    // When
    link_session(&mut queue, "n1", a_session("sess-a")).expect("linked");
    link_session(&mut queue, "n1", a_session("sess-a")).expect("linked again");
    link_session(&mut queue, "n1", a_session("sess-b")).expect("linked b");

    // Then
    assert_eq!(
        linked_sessions_of(&queue, "n1"),
        vec![a_session("sess-a"), a_session("sess-b")]
    );
}

#[test]
fn unlinking_removes_only_the_named_session() {
    // Given
    let mut queue = a_queue_with(&[("n1", None)]);
    link_session(&mut queue, "n1", a_session("sess-a")).expect("linked a");
    link_session(&mut queue, "n1", a_session("sess-b")).expect("linked b");

    // When
    unlink_session(&mut queue, "n1", "sess-a").expect("unlinked");
    let not_linked = unlink_session(&mut queue, "n1", "sess-z");

    // Then
    assert_eq!(linked_sessions_of(&queue, "n1"), vec![a_session("sess-b")]);
    assert_eq!(
        not_linked,
        Err(ItemLinkError::NotLinked {
            session_id: "sess-z".to_string()
        })
    );
}

#[test]
fn a_link_on_an_unknown_item_is_refused() {
    // Given
    let mut queue = a_queue_with(&[("n1", None)]);

    // When
    let refused = link_session(&mut queue, "n9", a_session("sess-a"));

    // Then
    assert_eq!(refused, Err(ItemLinkError::NoSuchItem("n9".to_string())));
}

#[test]
fn an_item_with_an_assigned_pr_is_addressed_by_that_pr_number() {
    // Given — an item whose recorded status URL names another PR than the one assigned to it
    let mut queue = a_queue_with(&[("n1", None)]);
    queue.nodes[0].pr_status = Some(tddy_core::changeset::GithubPrStatus {
        url: Some("https://github.com/acme/app/pull/3".to_string()),
        ..Default::default()
    });
    assign_pr(&mut queue, "n1", pr_7_on("feat/one")).expect("assigned");

    // When
    let number = tddy_work_queue::pr_insight::pull_number_for_node(&queue, "n1");

    // Then — the assigned PR wins
    assert_eq!(number, Ok(7));
}
