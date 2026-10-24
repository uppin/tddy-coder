//! Whether a subagent conversation has a worktree of its own for `subagent_diff`, `subagent_pull`
//! and `subagent_end` to read and take.
//!
//! An in-jail conversation under managed access always has one. A conversation the daemon runs has
//! one exactly when its agent was attached with `clone` placement; a `shared` one wrote the session
//! worktree directly and has nothing to hand back.
//!
//! PRD: docs/ft/coder/1-WIP/PRD-2026-10-24-coagents-clone-placement.md

/// Where the conversation's turns run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationHost {
    /// This `tddy-tools` process runs the loop.
    InProcess { managed_access: bool },
    /// The daemon runs it; `clone` is its agent's placement.
    Daemon { clone: bool },
}

/// Whether the conversation's work lives on a branch of its own.
pub fn has_its_own_worktree(_host: ConversationHost) -> bool {
    // TODO(clone-placement): implement
    false
}
