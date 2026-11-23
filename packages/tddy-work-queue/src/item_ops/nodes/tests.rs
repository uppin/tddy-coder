use super::*;
use crate::item_ops::test_support::{
    a_node, a_stack_session_dir, a_started_node, assert_rejected, stack_on_disk, write_stack,
};
use std::path::Path;
use tddy_core::changeset::{Changeset, WorkItem, WorkQueue};

// -----------------------------------------------------------------------
// add_work_item
// -----------------------------------------------------------------------

#[test]
fn appending_a_root_planned_pr_to_an_empty_stack_assigns_n1_and_persists_it() {
    // Given — a session with no stack yet
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    tddy_core::changeset::write_changeset(dir, &Changeset::default()).unwrap();
    // The session was started with a work queue, which is empty
    tddy_changeset::work_queue_fixtures::seed_session_stack(dir, WorkQueue::default());

    // When
    let result = add_work_item(
        dir,
        AddWorkItemInput {
            title: "Add token store".to_string(),
            description: "Persists refresh tokens.".to_string(),
            branch_suggestion: Some("feature/token-store".to_string()),
            parents: vec![],
            child_recipe: None,
        },
    );

    // Then
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let node = result.unwrap();
    assert_eq!(node.node_id, "n1");
    assert_eq!(node.title, "Add token store");
    assert_eq!(node.description, "Persists refresh tokens.");
    assert_eq!(
        node.branch_suggestion.as_deref(),
        Some("feature/token-store")
    );
    assert_eq!(node.parents, Vec::<String>::new());

    let loaded = stack_on_disk(dir);
    assert_eq!(loaded.nodes.len(), 1);
    assert_eq!(loaded.node("n1").unwrap().title, "Add token store");
}

#[test]
fn appending_a_node_with_valid_parents_persists_them_and_assigns_the_next_free_id() {
    // Given — a stack with two existing nodes, n1 and n2
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_node("n1", "Add token store", vec![]),
            a_node("n2", "Add auth middleware", vec!["n1"]),
        ],
    );

    // When — the new node depends on both existing nodes
    let result = add_work_item(
        dir,
        AddWorkItemInput {
            title: "Add token refresh endpoint".to_string(),
            description: String::new(),
            branch_suggestion: None,
            parents: vec!["n1".to_string(), "n2".to_string()],
            child_recipe: None,
        },
    );

    // Then
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    let node = result.unwrap();
    assert_eq!(node.node_id, "n3");
    assert_eq!(node.parents, vec!["n1".to_string(), "n2".to_string()]);

    let loaded = stack_on_disk(dir);
    assert_eq!(loaded.nodes.len(), 3);
    assert_eq!(
        loaded.node("n3").unwrap().parents,
        vec!["n1".to_string(), "n2".to_string()]
    );
}

#[test]
fn a_dangling_parent_ref_is_rejected_and_the_stack_on_disk_is_unchanged() {
    // Given — a stack with a single node, n1
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(dir, vec![a_node("n1", "Add token store", vec![])]);

    // When — the requested ancestor "n99" does not exist
    let result = add_work_item(
        dir,
        AddWorkItemInput {
            title: "Add auth middleware".to_string(),
            description: String::new(),
            branch_suggestion: None,
            parents: vec!["n99".to_string()],
            child_recipe: None,
        },
    );

    // Then
    assert!(result.is_err(), "expected Err for a dangling parent ref");
    assert!(result.unwrap_err().contains("n99"));
    let loaded = stack_on_disk(dir);
    assert_eq!(loaded.nodes.len(), 1, "stack on disk must be unchanged");
}

#[test]
fn the_new_node_always_stays_planned_with_no_session_id_or_pr_status() {
    // Given
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    tddy_core::changeset::write_changeset(dir, &Changeset::default()).unwrap();
    // The session was started with a work queue, which is empty
    tddy_changeset::work_queue_fixtures::seed_session_stack(dir, WorkQueue::default());

    // When
    let node = add_work_item(
        dir,
        AddWorkItemInput {
            title: "Add token store".to_string(),
            description: String::new(),
            branch_suggestion: None,
            parents: vec![],
            child_recipe: None,
        },
    )
    .unwrap();

    // Then
    assert_eq!(node.session_id, None);
    assert_eq!(node.pr_status, None);
    assert_eq!(node.branch, None);
    assert_eq!(node.child_state, None);
}

#[test]
fn node_id_assignment_picks_up_after_a_non_contiguous_max() {
    // Given — a stack whose highest existing node id is "n5", not the node count (2)
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![
            a_node("n1", "Add token store", vec![]),
            a_node("n5", "Add auth middleware", vec![]),
        ],
    );

    // When
    let node = add_work_item(
        dir,
        AddWorkItemInput {
            title: "Add token refresh endpoint".to_string(),
            description: String::new(),
            branch_suggestion: None,
            parents: vec![],
            child_recipe: None,
        },
    )
    .unwrap();

    // Then — next id is one past the max ("n6"), not one past the count ("n3")
    assert_eq!(node.node_id, "n6");
}

fn parents_of(dir: &Path, node_id: &str) -> Vec<String> {
    stack_on_disk(dir).node(node_id).unwrap().parents.clone()
}

fn an_update_of(node_id: &str) -> UpdateWorkItemInput {
    UpdateWorkItemInput {
        node_id: node_id.to_string(),
        ..UpdateWorkItemInput::default()
    }
}

// -----------------------------------------------------------------------
// update_work_item
// -----------------------------------------------------------------------

#[test]
fn updating_a_nodes_title_and_description_persists_both_and_leaves_every_other_field_untouched() {
    // Given — n1 is fully started: it owns a branch, a child session and an open PR
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![a_started_node("n1", "feature/n1", "open", vec![])],
    );
    let before = stack_on_disk(dir).node("n1").unwrap().clone();

    // When — the operator clarifies what the PR is for
    let node = update_work_item(
        dir,
        UpdateWorkItemInput {
            title: Some("Token store, split out".to_string()),
            description: Some("Extracted from the parent PR.".to_string()),
            ..an_update_of("n1")
        },
    )
    .expect("editing intent on a started node should succeed");

    // Then — only the two fields named changed; the node's identity and reality did not
    assert_eq!(node.title, "Token store, split out");
    assert_eq!(node.description, "Extracted from the parent PR.");
    assert_eq!(node.branch, before.branch);
    assert_eq!(node.session_id, before.session_id);
    assert_eq!(node.pr_status, before.pr_status);
    assert_eq!(node.parents, before.parents);
    assert_eq!(
        stack_on_disk(dir).node("n1").unwrap().title,
        "Token store, split out"
    );
}

#[test]
fn an_update_that_names_no_field_is_rejected_and_the_stack_on_disk_is_unchanged() {
    // Given
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(dir, vec![a_node("n1", "Original title", vec![])]);

    // When — every field left unset
    let result = update_work_item(dir, an_update_of("n1"));

    // Then — a no-op update can only be a caller mistake, so it is reported rather than hidden
    assert_rejected(result).with_reason_containing("names no field to change");
    assert_eq!(
        stack_on_disk(dir).node("n1").unwrap().title,
        "Original title"
    );
}

#[test]
fn a_branch_suggestion_edit_is_accepted_while_the_node_is_still_planned() {
    // Given — n1 was never started, so its suggestion is still the only name it has
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![WorkItem {
            branch_suggestion: Some("feature/old-name".to_string()),
            ..a_node("n1", "Add token store", vec![])
        }],
    );

    // When
    let node = update_work_item(
        dir,
        UpdateWorkItemInput {
            branch_suggestion: Some("feature/token-store".to_string()),
            ..an_update_of("n1")
        },
    )
    .expect("renaming a planned node's suggested branch should succeed");

    // Then — read back from disk as well as returned: an edit applied to a clone and never
    // persisted would be no edit at all
    assert_eq!(
        node.branch_suggestion.as_deref(),
        Some("feature/token-store")
    );
    assert_eq!(
        stack_on_disk(dir)
            .node("n1")
            .unwrap()
            .branch_suggestion
            .as_deref(),
        Some("feature/token-store")
    );
}

#[test]
fn a_branch_suggestion_edit_is_rejected_once_the_node_owns_a_branch() {
    // Given — a worktree already exists, so the suggestion has been superseded by a real ref
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(
        dir,
        vec![WorkItem {
            branch_suggestion: Some("feature/suggested".to_string()),
            ..a_started_node("n1", "feature/actual", "open", vec![])
        }],
    );

    // When
    let result = update_work_item(
        dir,
        UpdateWorkItemInput {
            branch_suggestion: Some("feature/renamed".to_string()),
            ..an_update_of("n1")
        },
    );

    // Then — refused, so the plan never claims a branch name nothing uses
    assert_rejected(result).with_reason_containing("feature/actual");
    assert_eq!(
        stack_on_disk(dir)
            .node("n1")
            .unwrap()
            .branch_suggestion
            .as_deref(),
        Some("feature/suggested")
    );
}

#[test]
fn updating_an_unknown_node_id_is_rejected() {
    // Given — a stack with no node called n9
    let tmp = a_stack_session_dir();
    let dir = tmp.path();
    write_stack(dir, vec![a_node("n1", "Add token store", vec![])]);

    // When
    let result = update_work_item(
        dir,
        UpdateWorkItemInput {
            title: Some("Retitled".to_string()),
            ..an_update_of("n9")
        },
    );

    // Then
    assert_rejected(result).with_reason_containing("n9");
}

// -----------------------------------------------------------------------
// delete_work_item
// -----------------------------------------------------------------------

mod delete;
