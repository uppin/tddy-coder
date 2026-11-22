//! A queue's GitHub reads through the daemon's inbox (`#workqueue` 6/14, moved here from n2 by the
//! operator): a branch's PR status (`GetPrStatus`, `QueryBranch`'s PR leg), the queue's status views
//! (`work_queue_status` / `work_queue_status`), and each item's check summary. Every read is an inbox row,
//! so a second reader of the same PR costs no GitHub request.

use std::path::Path;

use tddy_github_inbox::{GithubInbox, RepoSlug};
use tddy_service::proto::work_queue::PrStatusView;

use crate::assess::NodeView;
use tddy_core::changeset::WorkQueue;

/// `branch`'s PR as the wire's `PrStatusView`; `unavailable` (never "no PR") when the inbox could not
/// read it.
pub async fn pr_status_view_through_inbox(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    branch: &str,
) -> PrStatusView {
    // TODO(n6 pr-and-session-link): implement
    let _ = (inbox, repo, branch);
    todo!("TODO(n6): implement pr_status_view_through_inbox")
}

/// `assess::assemble_views`, reading each item's PR through the inbox instead of a synchronous
/// `GithubPrApi` lookup.
pub async fn assemble_views_through_inbox(
    parent_dir: &Path,
    sessions_root: &Path,
    queue: &WorkQueue,
    inbox: &GithubInbox,
    repo: &RepoSlug,
) -> Result<Vec<NodeView>, tddy_core::WorkflowError> {
    // TODO(n6 pr-and-session-link): implement
    let _ = (parent_dir, sessions_root, queue, inbox, repo);
    todo!("TODO(n6): implement assemble_views_through_inbox")
}

/// One PR'ed item's checks at its head, counted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemCheckSummary {
    pub item_id: String,
    pub running: u32,
    pub failed: u32,
    pub succeeded: u32,
}

/// The check summary of every item of `queue` that has a PR.
pub async fn item_check_summaries(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    queue: &WorkQueue,
) -> Vec<ItemCheckSummary> {
    // TODO(n6 pr-and-session-link): implement
    let _ = (inbox, repo, queue);
    todo!("TODO(n6): implement item_check_summaries")
}
