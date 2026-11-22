//! Acceptance: the PR-stack flags the daemon forwards to a spawned `tddy-coder`.
//!
//! The stack parent and the planned node id reach the spawned coder as `--stack-parent` and
//! `--stack-node-id`. Both flags are built by one pure function rather than appended inline to a
//! `Command`, because a flag that is silently not passed is the failure mode here — the child comes
//! up looking successful and unlinked from its stack — and an inline `cmd.arg` sequence gives a test
//! nothing to assert on.
//!
//! Feature: `docs/ft/coder/pr-stacking.md`

use tddy_spawn::spawner::work_queue_spawn_args;

#[test]
fn omits_the_flag_for_a_blank_stack_parent() {
    // Given / When — `work_queue_parent` reaches this from the same proto3 string field, so blank must
    // mean unset for it too
    let args = work_queue_spawn_args(Some("  "), None);

    // Then
    assert_eq!(args, Vec::<String>::new());
}

#[test]
fn trims_a_padded_stack_parent() {
    // Given / When — a padded id names a real session, so it is trimmed rather than refused; the
    // coder receives an id it can resolve to a directory
    let args = work_queue_spawn_args(Some(" orchestrator-1 "), None);

    // Then
    assert_eq!(
        args,
        vec!["--stack-parent".to_string(), "orchestrator-1".to_string()]
    );
}

#[test]
fn passes_the_planned_node_id_to_the_spawned_coder() {
    // Given / When — a child spawn for a planned node, which the coder publishes as its stack
    // association so the PR-Stack view can join a cross-host child back to the row that started it
    let args = work_queue_spawn_args(Some("orchestrator-1"), Some("n2"));

    // Then — the node id is the surface's own, never re-derived from the branch (D34)
    assert_eq!(
        args,
        vec![
            "--stack-parent".to_string(),
            "orchestrator-1".to_string(),
            "--stack-node-id".to_string(),
            "n2".to_string(),
        ]
    );
}

#[test]
fn omits_the_node_flag_for_a_spawn_that_names_no_planned_node() {
    // Given / When — the orchestrator agent's own `spawn-child`, which always runs on the
    // orchestrator's host and is linked by the branch-derived local write
    let args = work_queue_spawn_args(Some("orchestrator-1"), None);

    // Then
    assert_eq!(
        args,
        vec!["--stack-parent".to_string(), "orchestrator-1".to_string()]
    );
}

#[test]
fn omits_the_node_flag_for_a_blank_planned_node_id() {
    // Given / When — proto3 carries "unset" as the empty string, so blank must mean unset here too
    let args = work_queue_spawn_args(Some("orchestrator-1"), Some("   "));

    // Then — a blank `--stack-node-id` would reach the coder as an association it cannot publish
    assert_eq!(
        args,
        vec!["--stack-parent".to_string(), "orchestrator-1".to_string()]
    );
}

#[test]
fn trims_a_padded_planned_node_id() {
    // Given / When
    let args = work_queue_spawn_args(Some("orchestrator-1"), Some(" n2 "));

    // Then
    assert_eq!(
        args,
        vec![
            "--stack-parent".to_string(),
            "orchestrator-1".to_string(),
            "--stack-node-id".to_string(),
            "n2".to_string(),
        ]
    );
}
