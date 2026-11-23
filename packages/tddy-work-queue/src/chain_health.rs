//! Chain health per work item (`#workqueue` 12/14): **leaks** — commits in an item's PR that belong to
//! a line ancestor (the same SHA, or a rebased copy: same author, author date and subject) — and
//! **linearity** — whether the item contains its line parent's tip.
//!
//! Pure. Inputs: PR commit lists from the inbox (n2's `pr_commits`), the behind count from git
//! (`tddy_session_worktree::base_sync::count_divergence` over remote tips) and the PR's `mergeable` from the inbox.

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
    own.iter()
        .filter_map(|commit| {
            ancestors
                .iter()
                .find(|(_, theirs)| theirs.iter().any(|c| is_same_work(c, commit)))
                .map(|(branch, _)| Leak {
                    sha: commit.sha.clone(),
                    ancestor: branch.clone(),
                })
        })
        .collect()
}

/// The same SHA, or a rebased copy: same author, author date and subject.
fn is_same_work(a: &PrCommit, b: &PrCommit) -> bool {
    a.sha == b.sha
        || (a.author_name == b.author_name
            && a.author_date == b.author_date
            && a.subject == b.subject)
}

/// The line state from how far behind its parent the item is (`None`: no reading) and its PR's
/// `mergeable`.
pub fn line_state(behind_by: Option<u32>, mergeable: Option<bool>) -> LineState {
    match (behind_by, mergeable) {
        (None, _) => LineState::Unknown,
        (Some(0), _) => LineState::Linear,
        (Some(by), Some(false)) => LineState::Conflicting { by },
        (Some(by), _) => LineState::Behind { by },
    }
}
