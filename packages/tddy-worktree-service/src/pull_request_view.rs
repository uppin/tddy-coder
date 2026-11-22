//! A session's PR view, read through the daemon's GitHub inbox (`#workqueue` 4/14, the inspector's
//! PR tab). Nothing here calls GitHub itself; every read is an inbox row, and the watch re-sends the
//! view whenever the inbox reports a change to the PR, its checks or its conversation.

use std::sync::Arc;

use tddy_github_inbox::{GithubInbox, InboxError, RepoSlug};
use tddy_service::proto::worktree::SessionPullRequestView;
use tokio::sync::mpsc;

/// What a session listing shows of its branch's PR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestSummary {
    pub number: u64,
    /// `open` / `draft` / `merged` / `closed`.
    pub state: String,
}

/// The cached summary of `branch`'s PR, without any request — `None` when nothing is cached yet or
/// the branch has no PR. Session listing calls this; it must never wait on GitHub.
pub fn pull_request_summary(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    branch: &str,
) -> Option<PullRequestSummary> {
    // TODO(n4 session-pr-tabs): implement
    let _ = (inbox, repo, branch);
    todo!("TODO(n4): implement pull_request_summary")
}

/// `branch`'s PR view: the PR, its checks at the head, its conversation and its commits.
pub async fn pull_request_view(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    branch: &str,
) -> Result<SessionPullRequestView, InboxError> {
    // TODO(n4 session-pr-tabs): implement
    let _ = (inbox, repo, branch);
    todo!("TODO(n4): implement pull_request_view")
}

/// A live PR view: the current view first, then a new one on every relevant inbox event.
pub struct PullRequestWatch {
    receiver: mpsc::UnboundedReceiver<SessionPullRequestView>,
}

impl PullRequestWatch {
    /// The next view; `None` once the watch ended.
    pub async fn next(&mut self) -> Option<SessionPullRequestView> {
        self.receiver.recv().await
    }
}

/// Watch `branch`'s PR view through `inbox`.
pub fn watch_pull_request(
    inbox: Arc<GithubInbox>,
    repo: RepoSlug,
    branch: String,
) -> PullRequestWatch {
    // TODO(n4 session-pr-tabs): implement
    let _ = (inbox, repo, branch);
    todo!("TODO(n4): implement watch_pull_request")
}
