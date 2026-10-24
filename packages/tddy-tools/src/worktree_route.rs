//! Whether a subagent conversation has a worktree of its own for `subagent_diff`, `subagent_pull`
//! and `subagent_end` to read and take.
//!
//! An in-jail conversation under managed access always has one. A conversation the daemon runs has
//! one exactly when its agent was attached with `clone` placement; a `shared` one wrote the session
//! worktree directly and has nothing to hand back.
//!
//! Feature doc: docs/ft/coder/managed-codebase-subagents.md § A daemon-run conversation of a `clone`-placed agent

/// Where the conversation's turns run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationHost {
    /// This `tddy-tools` process runs the loop.
    InProcess { managed_access: bool },
    /// The daemon runs it; `clone` is its agent's placement.
    Daemon { clone: bool },
}

/// Whether the conversation's work lives on a branch of its own.
///
/// An in-process loop has one exactly when its tools reach the codebase through the daemon's managed
/// access, which is where the conversation worktree is cut. A daemon-run loop has one exactly when
/// its agent is `clone`-placed.
pub fn has_its_own_worktree(host: ConversationHost) -> bool {
    match host {
        ConversationHost::InProcess { managed_access } => managed_access,
        ConversationHost::Daemon { clone } => clone,
    }
}

/// Whether a roster entry's wire `placement` is `clone`. Only the value `clone` answers yes: an
/// unknown number is not read as a clone, because a shared agent is the one that never has a
/// worktree of its own to take.
pub fn is_clone_placement(placement: i32) -> bool {
    placement == tddy_service::proto::types::AgentPlacement::Clone as i32
}
