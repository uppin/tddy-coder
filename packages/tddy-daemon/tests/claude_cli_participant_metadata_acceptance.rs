//! Acceptance: the `session` block a **claude-cli** session's LiveKit participant publishes.
//!
//! A claude-cli session published no participant metadata at all, and planned-PR children are
//! claude-cli sessions — so a child started on another host arrived in the drawer as a synthesized
//! row carrying no `branch` and no `orchestrator_session_id`, the two keys every PR-stack join uses.
//! Presence is the only cross-host signal the web has (`ListSessions` does not fan out), so those
//! keys are the whole of the association's cross-host half (D37).
//!
//! The block is built by a named function rather than inline at the `spawn_livekit_bridge` call so
//! that what it carries can be asserted at all — the same reason `pr_stack_spawn_args` exists.
//!
//! What is deliberately **not** here: that the block reaches a room. That needs a LiveKit server,
//! and the serialization it travels as is pinned by
//! `tddy-core/tests/session_participant_metadata_acceptance.rs`.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D37).

use std::path::Path;

use tddy_daemon::connection_service::{
    claude_cli_participant_metadata, SpawnStackParent, StackBaseLookup, StackNodeLink,
    StackParentHost, StartingClaudeCliSession,
};
use tddy_rpc::Status;

const SESSION: &str = "019f9fdb-cf83-70d2-aef5-0000000000b2";
const ORCHESTRATOR: &str = "019f9dd5-716d-7071-96ac-464ff7b98c2a";
const NODE: &str = "attach-store";
const BRANCH: &str = "feature/attach-docs/attach-store";

/// A starting claude-cli session that materializes a planned node of a pr-stack orchestrator.
fn a_starting_child_of<'a>(stack_parent: &'a SpawnStackParent<'a>) -> StartingClaudeCliSession<'a> {
    StartingClaudeCliSession {
        session_id: SESSION,
        model: "claude-opus-4-8",
        recipe: "tdd",
        worktree_path: Path::new("/home/dev/worktrees/attach-store"),
        branch: BRANCH,
        stack_parent,
    }
}

/// The orchestrator's own daemon, as a spawn addresses it. Its `host` is never called here — this
/// block is built from what the spawn already knows, and asks nobody.
fn an_orchestrator_that_planned_the_node<'a>(
    host: &'a dyn StackParentHost,
) -> SpawnStackParent<'a> {
    SpawnStackParent::OwnedBy {
        session_id: ORCHESTRATOR,
        daemon_instance_id: "host-a",
        stack_node_id: NODE,
        session_token: "valid-session-token",
        host,
    }
}

/// A [`StackParentHost`] that answers nothing: the block is built from what the spawn already
/// knows, so resolving a base or writing a link here would be a question nobody asked.
struct AnUnaskedOwner;

#[async_trait::async_trait]
impl StackParentHost for AnUnaskedOwner {
    async fn chain_base_ref(
        &self,
        _lookup: &StackBaseLookup<'_>,
    ) -> Result<Option<String>, Status> {
        panic!("building the participant block must ask the orchestrator's daemon nothing")
    }

    async fn link_spawned_branch(&self, _link: &StackNodeLink<'_>) -> Result<(), Status> {
        panic!("building the participant block must ask the orchestrator's daemon nothing")
    }
}

#[test]
fn names_the_session_its_orchestrator_the_planned_node_and_the_branch_it_created() {
    // Given
    let owner = AnUnaskedOwner;
    let stack_parent = an_orchestrator_that_planned_the_node(&owner);

    // When
    let block = claude_cli_participant_metadata(&a_starting_child_of(&stack_parent));

    // Then — these four are what a PR-Stack view one host over joins the synthesized row on
    assert_eq!(
        (
            block.session_id.as_str(),
            block.orchestrator_session_id.as_str(),
            block.stack_node_id.as_str(),
            block.branch.as_str()
        ),
        (SESSION, ORCHESTRATOR, NODE, BRANCH)
    );
}

#[test]
fn names_the_agent_the_model_and_the_checkout_the_session_runs_in() {
    // Given
    let owner = AnUnaskedOwner;
    let stack_parent = an_orchestrator_that_planned_the_node(&owner);

    // When
    let block = claude_cli_participant_metadata(&a_starting_child_of(&stack_parent));

    // Then — a cross-host row previously showed only a short session id
    assert_eq!(
        (
            block.agent.as_str(),
            block.model.as_str(),
            block.recipe.as_str(),
            block.repo_path.as_str()
        ),
        (
            "claude",
            "claude-opus-4-8",
            "tdd",
            "/home/dev/worktrees/attach-store"
        )
    );
}

#[test]
fn publishes_the_association_of_a_session_that_belongs_to_no_stack_as_empty_rather_than_absent() {
    // Given — an ordinary claude-cli session, started under no orchestrator
    let stack_parent = SpawnStackParent::NoParent;

    // When
    let block = claude_cli_participant_metadata(&StartingClaudeCliSession {
        branch: "feature/standalone",
        ..a_starting_child_of(&stack_parent)
    });

    // Then — empty is a fact ("this session is nobody's stack child"), and the merge into
    // participant metadata is shallow, so an omitted key would erase a sibling publisher's
    assert_eq!(
        (
            block.orchestrator_session_id.as_str(),
            block.stack_node_id.as_str()
        ),
        ("", "")
    );
}

#[test]
fn leaves_the_live_workflow_fields_empty_because_the_daemon_taps_no_workflow() {
    // Given
    let owner = AnUnaskedOwner;
    let stack_parent = an_orchestrator_that_planned_the_node(&owner);

    // When
    let block = claude_cli_participant_metadata(&a_starting_child_of(&stack_parent));

    // Then — exactly as they were when a claude-cli session published nothing at all; filling them
    // needs a workflow tap the way `tddy-coder` has one
    assert_eq!(
        (
            block.goal.as_str(),
            block.state.as_str(),
            block.activity_status.as_str(),
            block.elapsed_display.as_str(),
            block.pending_elicitation
        ),
        ("", "", "", "", false)
    );
}
