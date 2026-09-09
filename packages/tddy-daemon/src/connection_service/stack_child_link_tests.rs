use super::*;
use tddy_core::changeset::{Stack, StackNode};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::{read_changeset, write_changeset, Changeset};

/// A planned node: it carries the branch name the planner proposed, but nothing created it yet.
fn a_planned_node(node_id: &str, branch_suggestion: &str, parents: &[&str]) -> StackNode {
    StackNode {
        node_id: node_id.to_string(),
        title: node_id.to_string(),
        description: String::new(),
        branch_suggestion: Some(branch_suggestion.to_string()),
        branch: None,
        session_id: None,
        parents: parents.iter().map(|p| p.to_string()).collect(),
        pr_status: None,
        child_state: None,
        internal_status: None,
        display_order: None,
    }
}

/// A pr-stack orchestrator whose planned stack is `bottom` (`feature/bottom`) → `top`.
fn an_orchestrator_with_a_two_node_stack(sessions_base: &std::path::Path) -> PathBuf {
    let dir = unified_session_dir_path(sessions_base, "orchestrator-1");
    std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
    write_changeset(
        &dir,
        &Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![
                    a_planned_node("bottom", "feature/bottom", &[]),
                    a_planned_node("top", "feature/top", &["bottom"]),
                ],
            }),
            ..Changeset::default()
        },
    )
    .expect("write orchestrator changeset");
    dir
}

/// The same stack, except `bottom` already owns a branch that differs from its suggestion.
fn an_orchestrator_with_a_renamed_bottom_branch(sessions_base: &std::path::Path) -> PathBuf {
    let dir = unified_session_dir_path(sessions_base, "orchestrator-1");
    std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
    write_changeset(
        &dir,
        &Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![
                    StackNode {
                        branch: Some("feature/bottom-renamed".to_string()),
                        ..a_planned_node("bottom", "feature/bottom", &[])
                    },
                    a_planned_node("top", "feature/top", &["bottom"]),
                ],
            }),
            ..Changeset::default()
        },
    )
    .expect("write orchestrator changeset");
    dir
}

/// The same stack, except `bottom` was worked on and then lost its session: it owns
/// `feature/bottom`, but the session recorded against it no longer exists. This is the state
/// `DeleteSession` leaves behind, and the one the operator recovers from.
fn an_orchestrator_whose_bottom_node_lost_its_child_session(
    sessions_base: &std::path::Path,
) -> PathBuf {
    let dir = unified_session_dir_path(sessions_base, "orchestrator-1");
    std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
    write_changeset(
        &dir,
        &Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![
                    StackNode {
                        branch: Some("feature/bottom".to_string()),
                        session_id: Some("deleted-child".to_string()),
                        ..a_planned_node("bottom", "feature/bottom", &[])
                    },
                    a_planned_node("top", "feature/top", &["bottom"]),
                ],
            }),
            ..Changeset::default()
        },
    )
    .expect("write orchestrator changeset");
    dir
}

/// The same stack, except `bottom` never recorded its branch — only the child session that
/// created it knows the name. Models a link written before the branch was known.
fn an_orchestrator_whose_bottom_branch_only_its_session_knows(
    sessions_base: &std::path::Path,
) -> PathBuf {
    let child_dir = unified_session_dir_path(sessions_base, "child-1");
    std::fs::create_dir_all(&child_dir).expect("create child session dir");
    write_changeset(
        &child_dir,
        &Changeset {
            branch: Some("feature/bottom".to_string()),
            ..Changeset::default()
        },
    )
    .expect("write child changeset");

    let dir = unified_session_dir_path(sessions_base, "orchestrator-1");
    std::fs::create_dir_all(&dir).expect("create orchestrator session dir");
    write_changeset(
        &dir,
        &Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(Stack {
                version: 1,
                nodes: vec![
                    StackNode {
                        session_id: Some("child-1".to_string()),
                        ..a_planned_node("bottom", "feature/bottom", &[])
                    },
                    a_planned_node("top", "feature/top", &["bottom"]),
                ],
            }),
            ..Changeset::default()
        },
    )
    .expect("write orchestrator changeset");
    dir
}

#[test]
fn spawning_bases_a_node_on_a_parent_branch_only_its_child_session_recorded() {
    // Given — `bottom` owns no branch of its own; only its child session names one
    let tmp = tempfile::tempdir().expect("temp dir");
    an_orchestrator_whose_bottom_branch_only_its_session_knows(tmp.path());

    // When — `top` is spawned and the daemon resolves the node it materializes
    let (_dir, stack, node_id) =
        tddy_core::pr_stack_node_for_spawn(tmp.path(), "orchestrator-1", "", "feature/top")
            .expect("the planned node for the spawned branch must resolve");

    // Then — the session fallback supplies the parent's branch, so `top` is not blocked
    assert_eq!(node_id, "top");
    assert_eq!(
        stack
            .base_ref_for_spawn(&node_id, "origin/master")
            .expect("a session-resolved parent branch must unblock its dependent"),
        "origin/feature/bottom"
    );
}

fn stack_of(orchestrator_dir: &std::path::Path) -> Stack {
    read_changeset(orchestrator_dir)
        .expect("read orchestrator changeset")
        .stack
        .expect("orchestrator must carry a stack")
}

#[test]
fn linking_records_the_branch_a_child_created_on_the_planned_node_it_materializes() {
    // Given — a planned stack, nothing spawned yet
    let tmp = tempfile::tempdir().expect("temp dir");
    let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());

    // When — a child creates the branch the bottom node was planned to own
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        "feature/bottom",
        "child-1",
    )
    .expect("linking the spawned branch must succeed");

    // Then — the node owns a real branch now; the sibling stays planned
    let stack = stack_of(&orchestrator_dir);
    assert_eq!(
        stack.node("bottom").and_then(|n| n.branch.as_deref()),
        Some("feature/bottom"),
        "the planned node must record the branch the child actually created"
    );
    assert_eq!(
        stack.node("top").and_then(|n| n.branch.as_deref()),
        None,
        "linking one node must not touch its siblings"
    );
}

#[test]
fn linking_records_the_child_session_as_a_fallback_route_to_the_branch() {
    // Given — a planned stack, nothing spawned yet
    let tmp = tempfile::tempdir().expect("temp dir");
    let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());

    // When — a child creates the bottom node's branch
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        "feature/bottom",
        "child-1",
    )
    .expect("linking the spawned branch must succeed");

    // Then — the session is recorded too, so the branch stays resolvable from the session alone
    assert_eq!(
        stack_of(&orchestrator_dir)
            .node("bottom")
            .and_then(|n| n.session_id.as_deref()),
        Some("child-1")
    );
}

#[test]
fn linking_a_planned_node_unblocks_spawning_its_dependent_node() {
    // Given — a planned stack where `top` depends on `bottom`
    let tmp = tempfile::tempdir().expect("temp dir");
    let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());

    // Before the link, `top` has no ref to base onto: the failed_precondition operators hit
    let err = stack_of(&orchestrator_dir)
        .base_ref_for_spawn("top", "origin/master")
        .expect_err("a parent without a branch must block its dependent");
    assert!(
        err.to_string().contains("no branch"),
        "unexpected error: {err}"
    );

    // When — `bottom`'s branch is created and linked
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        "feature/bottom",
        "child-1",
    )
    .expect("linking the spawned branch must succeed");

    // Then — `top` bases off its parent's branch instead of being refused
    assert_eq!(
        stack_of(&orchestrator_dir)
            .base_ref_for_spawn("top", "origin/master")
            .expect("a branch-owning parent must no longer block its dependent"),
        "origin/feature/bottom"
    );
}

#[test]
fn linking_is_a_no_op_when_the_spawn_materializes_no_planned_node() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());

    // No stack parent at all, and a branch no planned node claims: both are ordinary sessions.
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        None,
        "feature/bottom",
        "child-1",
    )
    .expect("a parentless spawn must not error");
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        "feature/unplanned",
        "child-1",
    )
    .expect("a spawn whose branch matches no node must not error");

    let stack = stack_of(&orchestrator_dir);
    assert!(
        stack.nodes.iter().all(|n| n.branch.is_none()),
        "no planned node was materialized, so none may be linked"
    );
}

#[test]
fn linking_repoints_a_node_to_the_child_session_that_now_owns_its_branch() {
    // Given — the bottom node's branch was first created by child-1
    let tmp = tempfile::tempdir().expect("temp dir");
    let orchestrator_dir = an_orchestrator_with_a_two_node_stack(tmp.path());
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        "feature/bottom",
        "child-1",
    )
    .expect("first link must succeed");

    // When — that session is replaced by a new one on the same branch (restart, re-attach)
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        "feature/bottom",
        "child-2",
    )
    .expect("a new session on the same branch must be accepted");

    // Then — the fallback points at the live session; the branch is untouched
    let stack = stack_of(&orchestrator_dir);
    assert_eq!(
        stack.node("bottom").and_then(|n| n.session_id.as_deref()),
        Some("child-2")
    );
    assert_eq!(
        stack.node("bottom").and_then(|n| n.branch.as_deref()),
        Some("feature/bottom")
    );
}

#[test]
fn a_spawn_resuming_an_existing_branch_relinks_the_node_that_owns_it() {
    // Given — `bottom` owns a pushed branch whose session was deleted; the operator restarts it
    // with `work_on_selected_branch`, so no new branch is created and `new_branch_name` is empty
    let tmp = tempfile::tempdir().expect("temp dir");
    let orchestrator_dir = an_orchestrator_whose_bottom_node_lost_its_child_session(tmp.path());

    // When — the spawn links its node on the branch it actually operates on
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        effective_spawn_branch("work_on_selected_branch", "", "feature/bottom", "origin"),
        "child-2",
    )
    .expect("a resumed branch must link its planned node");

    // Then — the recovery sticks: the node points at the live session instead of the deleted one,
    // so the row leaves its recovered state and a second click cannot spawn another orphan
    let stack = stack_of(&orchestrator_dir);
    assert_eq!(
        (
            stack.node("bottom").and_then(|n| n.session_id.as_deref()),
            stack.node("bottom").and_then(|n| n.branch.as_deref())
        ),
        (Some("child-2"), Some("feature/bottom"))
    );
}

#[test]
fn a_spawn_resuming_a_remote_tracking_branch_relinks_the_node_that_owns_it() {
    // Given — the same recovery, driven from the web: the dialog's branch picker is fed by
    // `ListProjectBranches`, which offers `origin/<branch>` names, so that is what the spawn carries
    let tmp = tempfile::tempdir().expect("temp dir");
    let orchestrator_dir = an_orchestrator_whose_bottom_node_lost_its_child_session(tmp.path());

    // When
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        effective_spawn_branch(
            "work_on_selected_branch",
            "",
            "origin/feature/bottom",
            "origin",
        ),
        "child-2",
    )
    .expect("a resumed remote-tracking branch must link its planned node");

    // Then — nodes record local branch names, so a prefixed key would match nothing and leave the
    // node orphaned forever
    let stack = stack_of(&orchestrator_dir);
    assert_eq!(
        (
            stack.node("bottom").and_then(|n| n.session_id.as_deref()),
            stack.node("bottom").and_then(|n| n.branch.as_deref())
        ),
        (Some("child-2"), Some("feature/bottom"))
    );
}

#[test]
fn linking_matches_a_node_by_the_branch_it_recorded_rather_than_its_suggestion() {
    // Given — `bottom` was materialized on a branch other than the one the planner suggested
    let tmp = tempfile::tempdir().expect("temp dir");
    let orchestrator_dir = an_orchestrator_with_a_renamed_bottom_branch(tmp.path());

    // When — a session attaches to the branch the node actually owns
    ConnectionServiceImpl::link_stack_node_to_spawned_branch(
        tmp.path(),
        Some("orchestrator-1"),
        "feature/bottom-renamed",
        "child-1",
    )
    .expect("the recorded branch must identify the node");

    // Then — the node is found by its real branch, not missed because of its stale suggestion
    assert_eq!(
        stack_of(&orchestrator_dir)
            .node("bottom")
            .and_then(|n| n.session_id.as_deref()),
        Some("child-1")
    );
}
