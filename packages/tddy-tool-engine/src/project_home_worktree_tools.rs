//! `ListWorktrees` and `GetWorktree`: the project's worktrees as the web's Worktrees screen shows
//! them, for an orchestrator agent (`#start-parity` 3/9).
//!
//! A row is `worktree.WorktreeRow` under its own field names — the snapshot the screen's
//! `StreamWorktreeStats` sends first — and the list is in the screen's order
//! (`tddy-web/src/lib/worktreeList.ts` `sortWorktreesBySize`): biggest first, never-sized last, ties
//! by name. `tests/worktree_tools_parity.rs` holds the field list to the proto descriptor, and the
//! web's `worktreeToolParity.cy.ts` holds the order to the screen's own sort over a shared fixture.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde::{Deserialize, Serialize};

use crate::catalog::ToolDef;

pub const LIST_WORKTREES: &str = "ListWorktrees";
pub const GET_WORKTREE: &str = "GetWorktree";

/// The page size a `ListWorktrees` call without a `limit` gets.
pub const WORKTREE_PAGE_DEFAULT: usize = 50;
/// The most rows one `ListWorktrees` page holds; a larger `limit` is clamped to it.
pub const WORKTREE_PAGE_MAX: usize = 200;

/// One worktree, as the Worktrees screen's row has it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeEntry {
    pub path: String,
    pub branch_label: String,
    /// The last size calculated; `None` when the worktree has never been sized.
    pub disk_bytes: Option<u64>,
    pub changed_files: u32,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub updated_at_unix_ms: i64,
    pub stale: bool,
    /// `none`, `calculating` or `cached`.
    pub size_status: String,
    pub size_calculated_at_unix_ms: i64,
}

/// How far a round of size calculations has got: rows not calculating, of all rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizingProgress {
    pub done: usize,
    pub total: usize,
}

/// One `ListWorktrees` page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeListing {
    pub worktrees: Vec<WorktreeEntry>,
    pub total: usize,
    /// Pass back as `cursor` for the next page; empty when there is none.
    pub next_cursor: String,
    pub has_more: bool,
    /// Set when the `limit` asked for was above [`WORKTREE_PAGE_MAX`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clamped_to: Option<usize>,
    pub sizing: SizingProgress,
}

/// `entries` in the Worktrees screen's order.
pub fn sort_worktrees_by_size(entries: &mut [WorktreeEntry]) {
    // TODO(worktree-list): implement
    let _ = entries;
}

/// The page of the sorted `entries` that `cursor` and `limit` ask for, with the sizing progress of
/// all of them.
pub fn page_worktrees(
    entries: Vec<WorktreeEntry>,
    cursor: &str,
    limit: Option<usize>,
) -> Result<WorktreeListing, String> {
    // TODO(worktree-list): implement
    let _ = (entries, cursor, limit);
    Err("TODO(worktree-list): page_worktrees".to_string())
}

/// The two tools' definitions.
pub fn worktree_tool_defs() -> Vec<ToolDef> {
    // TODO(worktree-list): implement
    Vec::new()
}
