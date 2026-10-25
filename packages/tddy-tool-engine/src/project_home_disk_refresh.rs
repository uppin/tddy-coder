//! `RefreshWorktreeDiskUsage`: the Worktrees screen's Refresh, for an orchestrator agent
//! (`#start-parity` 8/9).
//!
//! It queues the same background size walk the screen's Refresh queues and returns at once. The
//! size calculator keeps each worktree's state (`none`, `calculating`, `cached`) and persists the
//! sizes, so progress is read where the screen reads it: `ListWorktrees`' `sizing {done, total}`
//! (the screen's "{done} of {total}") and each row's `size_status`. No ledger of its own.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde::{Deserialize, Serialize};

use crate::catalog::ToolDef;

pub const REFRESH_WORKTREE_DISK_USAGE: &str = "RefreshWorktreeDiskUsage";

/// What a refresh answers at once: how many worktrees it queued a size walk for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshQueued {
    pub queued: usize,
}

/// The tool's definition.
pub fn refresh_worktree_disk_usage_tool_def() -> Option<ToolDef> {
    // TODO(disk-refresh): implement
    None
}
