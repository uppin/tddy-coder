//! What a conversation's mutating tool call did to its worktree, read off the call's result.
//!
//! The daemon answers a conversation's mutating call with the tool's own fields plus a
//! `worktreeChange` object. It is lifted out of the still-structured result here — not from the
//! model-facing text — so the descriptor can report it beside, not inside, the tool's summary.

use tddy_subagent_worktree::WorktreeChange;

use super::ToolDispatch;

/// The `worktreeChange` of a dispatch that ran, or `None` when the call had none: a read, a call
/// made outside a conversation worktree, or a dispatch that produced no result.
///
/// A `worktreeChange` that is present but unreadable is reported in the log and left off rather
/// than invented; the tool's own result still stands.
pub(super) fn of(dispatch: &ToolDispatch) -> Option<WorktreeChange> {
    let ToolDispatch::Ran(result) = dispatch else {
        return None;
    };
    let change = result.get("worktreeChange")?;
    match serde_json::from_value(change.clone()) {
        Ok(change) => Some(change),
        Err(error) => {
            log::warn!("a tool result carried an unreadable worktreeChange: {error}");
            None
        }
    }
}
