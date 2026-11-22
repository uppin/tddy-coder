//! Chain health per work item (`#workqueue` 12/14): **leaks** — commits in an item's PR that belong to
//! a line ancestor (the same SHA, or a rebased copy: same author, author date and subject) — and
//! **linearity** — whether the item contains its line parent's tip.
//!
//! Pure. Inputs: PR commit lists from the inbox (n2's `pr_commits`), the behind count from git
//! (`tddy_core::base_sync::count_divergence` over remote tips) and the PR's `mergeable` from the inbox.

use tddy_github_inbox::PrCommit;

/// One leaked commit and the ancestor it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leak {
    pub sha: String,
    /// The nearest line ancestor's branch whose PR carries it.
    pub ancestor: String,
}

/// The line edge into an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineState {
    /// The item contains its parent's tip.
    Linear,
    /// The parent has `by` commits the item lacks.
    Behind { by: u32 },
    /// Behind, and GitHub reports the item's PR as not mergeable.
    Conflicting { by: u32 },
    /// No reading: the parent merged or planned, or the count unreadable.
    Unknown,
}

/// The commits of `own` that belong to an ancestor — `ancestors` are each line ancestor's branch and
/// PR commits, **nearest first**.
pub fn leaks(own: &[PrCommit], ancestors: &[(String, Vec<PrCommit>)]) -> Vec<Leak> {
    // TODO(n12 leak-linearity): implement
    let _ = (own, ancestors);
    todo!("TODO(n12): implement leaks")
}

/// The line state from how far behind its parent the item is (`None`: no reading) and its PR's
/// `mergeable`.
pub fn line_state(behind_by: Option<u32>, mergeable: Option<bool>) -> LineState {
    // TODO(n12 leak-linearity): implement
    let _ = (behind_by, mergeable);
    todo!("TODO(n12): implement line_state")
}
