//! A session's git worktree: creating and reusing it, the integration base it is cut from, the
//! chain base a stacked child inherits from its parent, and the base sync that keeps it current.
//!
//! Extracted from `tddy-core`, which re-exports every module and root item at its old path.

pub mod base_sync;
pub mod git_head;
pub mod session_chain;
pub mod worktree;

// The changeset and session layout a worktree is recorded in, named at the `crate::` paths these
// modules used inside `tddy-core`.
use tddy_changeset::{branch_worktree_intent, changeset, session_lifecycle};
use tddy_session_store::error::WorkflowError;

pub use session_chain::{
    classify_stack_parent_route, integrate_chain_base_into_session_worktree_bootstrap,
    parent_is_pr_stack_orchestrator, pr_stack_node_for_spawn, resolve_chain_base_for_session_spawn,
    resolve_chain_base_ref, resolve_chain_integration_base_ref_from_parent_session,
    select_worktree_base_ref, spawn_chain_child_worktree, StackParentRoute,
};
#[allow(deprecated)]
pub use worktree::DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF;
pub use worktree::{
    create_worktree, detect_default_remote_name, fetch_integration_base, fetch_origin_master,
    list_recent_remote_branches, list_recent_remote_branches_skip, list_worktrees,
    local_branch_name, local_branch_name_for_remote, push_new_branch_to_origin,
    push_new_branch_to_remote, remove_worktree, resolve_default_integration_base_ref,
    resolve_default_integration_base_ref_with_remote,
    resolve_persisted_worktree_integration_base_for_session, set_git_ssh_command,
    setup_worktree_for_session, setup_worktree_for_session_over_ssh,
    setup_worktree_for_session_with_integration_base,
    setup_worktree_for_session_with_optional_chain_base, validate_chain_pr_integration_base_ref,
    validate_integration_base_ref, worktree_dir, worktree_path_for_branch, WorktreeInfo,
    FALLBACK_DEFAULT_INTEGRATION_BASE_REF,
};
