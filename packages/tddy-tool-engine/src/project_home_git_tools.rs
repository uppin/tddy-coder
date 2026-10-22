//! The project-home session's git tools (`#project-home` 7/7), on a target session's worktree in
//! its own project. Executed through `tddy_worktree_service::session_git_ops`, which applies the
//! activity gate; role-gated like the tools in [`crate::project_home_tools`].
//!
//! | Tool | Arguments | Gate |
//! |---|---|---|
//! | `CommitSessionWorktree` | `session_id`, `message`, `paths?` | always |
//! | `PushSessionBranch` | `session_id`, `force_with_lease?` | always; force-with-lease only on an inactive target |
//! | `RebaseSessionBranch` | `session_id`, `onto` | inactive target |
//! | `StashSessionWorktree` | `session_id` | inactive target |
//! | `ResetSessionWorktree` | `session_id`, `mode` (`soft`/`mixed`/`hard`), `to` | inactive target |
//! | `CheckoutSessionWorktree` | `session_id`, `to` | inactive target |
//! | `CleanSessionWorktree` | `session_id` | inactive target |
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-git-tools.md.

use crate::catalog::ToolDef;

pub const COMMIT_SESSION_WORKTREE: &str = "CommitSessionWorktree";
pub const PUSH_SESSION_BRANCH: &str = "PushSessionBranch";
pub const REBASE_SESSION_BRANCH: &str = "RebaseSessionBranch";
pub const STASH_SESSION_WORKTREE: &str = "StashSessionWorktree";
pub const RESET_SESSION_WORKTREE: &str = "ResetSessionWorktree";
pub const CHECKOUT_SESSION_WORKTREE: &str = "CheckoutSessionWorktree";
pub const CLEAN_SESSION_WORKTREE: &str = "CleanSessionWorktree";

/// The seven git tools' definitions, as advertised to a project-home session.
pub fn project_home_git_tool_defs() -> Vec<ToolDef> {
    // TODO(home-git-tools): implement
    Vec::new()
}
