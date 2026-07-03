//! Integration tests: the per-session managed-workflow wiring
//! (PRD: docs/ft/coder/managed-codebase-workflow.md, AC11/AC13).
//!
//! `set_up_managed_workflow` builds a per-session `WorkflowController` positioned at the recipe's
//! start goal and a toolcall listener whose handler is that controller. A committed transition
//! persists the new state into the session's `changeset.yaml`; an illegal transition is rejected and
//! leaves the persisted state unchanged.

use tddy_core::backend::GoalId;
use tddy_core::changeset::{read_changeset, write_changeset, Changeset};
use tddy_core::workflow::controller::TransitionOutcome;
use tddy_daemon::session_toolcall::set_up_managed_workflow;
use tddy_workflow_recipes::resolve_workflow_recipe_from_cli_name;

/// The `tdd` recipe: start goal `interview`, with `interview -> plan` a valid edge and
/// `interview -> green` an invalid one.
const START_GOAL: &str = "interview";
const NEXT_GOAL: &str = "plan";
const ILLEGAL_GOAL: &str = "green";

/// A managed-workflow harness scoped to fresh temp dirs, with an initial changeset written so the
/// controller can persist transitions into it.
struct Harness {
    _session_dir: tempfile::TempDir,
    _worktree: tempfile::TempDir,
    _tddy_data: tempfile::TempDir,
    _socket_dir: tempfile::TempDir,
    session_dir_path: std::path::PathBuf,
    workflow: tddy_daemon::session_toolcall::ManagedWorkflow,
}

fn a_managed_workflow(recipe_name: &str) -> Harness {
    let session_dir = tempfile::tempdir().unwrap();
    let worktree = tempfile::tempdir().unwrap();
    let tddy_data = tempfile::tempdir().unwrap();
    let socket_dir = tempfile::tempdir().unwrap();

    // An initial changeset must exist for the controller's persist step to read/update it.
    write_changeset(session_dir.path(), &Changeset::default()).unwrap();

    let recipe = resolve_workflow_recipe_from_cli_name(recipe_name).expect("recipe must resolve");
    let workflow = set_up_managed_workflow(
        "test-session",
        recipe,
        session_dir.path(),
        worktree.path(),
        tddy_data.path(),
        socket_dir.path(),
        None,
    )
    .expect("set_up_managed_workflow must succeed");

    Harness {
        session_dir_path: session_dir.path().to_path_buf(),
        _session_dir: session_dir,
        _worktree: worktree,
        _tddy_data: tddy_data,
        _socket_dir: socket_dir,
        workflow,
    }
}

/// AC11: the managed workflow's controller is positioned at the recipe's start goal.
#[tokio::test]
async fn managed_workflow_starts_at_the_recipe_start_goal() {
    // Given / When
    let harness = a_managed_workflow("tdd");

    // Then
    assert_eq!(
        harness.workflow.controller.current_goal().as_str(),
        START_GOAL,
        "controller must start at the recipe's start goal"
    );
    assert_eq!(harness.workflow.start_goal.as_str(), START_GOAL);
}

/// AC11: a committed transition persists the new state into the session's changeset.yaml.
#[tokio::test]
async fn a_committed_transition_persists_the_new_state_to_changeset() {
    // Given
    let harness = a_managed_workflow("tdd");

    // When — an authoritative (orchestrator) transition along a valid edge
    let outcome = harness
        .workflow
        .controller
        .transition(GoalId::new(NEXT_GOAL), false);

    // Then
    assert!(
        matches!(outcome, TransitionOutcome::Committed { .. }),
        "a valid transition must be committed, got: {outcome:?}"
    );
    let changeset = read_changeset(&harness.session_dir_path).unwrap();
    assert_eq!(
        changeset.state.current.as_str(),
        NEXT_GOAL,
        "the committed goal must be persisted as the current changeset state"
    );
}

/// AC11: an illegal transition is rejected and leaves the persisted state unchanged.
#[tokio::test]
async fn an_illegal_transition_is_rejected_and_leaves_the_state_unchanged() {
    // Given
    let harness = a_managed_workflow("tdd");
    let state_before = read_changeset(&harness.session_dir_path)
        .unwrap()
        .state
        .current;

    // When — a transition along an edge the recipe graph does not allow
    let outcome = harness
        .workflow
        .controller
        .transition(GoalId::new(ILLEGAL_GOAL), false);

    // Then
    assert!(
        matches!(outcome, TransitionOutcome::Rejected { .. }),
        "an illegal transition must be rejected, got: {outcome:?}"
    );
    let state_after = read_changeset(&harness.session_dir_path)
        .unwrap()
        .state
        .current;
    assert_eq!(
        state_after, state_before,
        "a rejected transition must leave the persisted state unchanged"
    );
}
