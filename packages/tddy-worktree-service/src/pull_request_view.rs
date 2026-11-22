//! A session's PR view, read through the daemon's GitHub inbox (`#workqueue` 4/14, the inspector's
//! PR tab). Nothing here calls GitHub itself; every read is an inbox row, and the watch re-sends the
//! view whenever the inbox reports a change to the PR, its checks or its conversation.

use std::sync::Arc;

use tddy_github::pr_api::{CheckRun, CheckStatus, PrState};
use tddy_github_inbox::{EventKind, GithubInbox, InboxError, InboxEvent, RepoSlug, RowKey};
use tddy_rpc::Status;
use tddy_service::proto::session::SessionEntry;
use tddy_service::proto::worktree::{
    PullRequestCheck, PullRequestComment, PullRequestCommit, SessionPullRequestView,
};
use tokio::sync::mpsc;

/// The inbox a session's PRs are read through, and the repository they live in.
#[derive(Clone)]
pub struct PullRequestSite {
    pub inbox: Arc<GithubInbox>,
    pub repo: RepoSlug,
}

/// How the daemon locates a session's PRs: the host owns the inbox, the project rows and the
/// credentials a repository is read with, so this crate asks it through this port.
pub trait SessionPullRequests: Send + Sync {
    /// The inbox and repository of `project_id`, with the repository registered to be read as the
    /// caller. A project that names no GitHub repository, or has no usable account, is refused.
    fn site_for_session(
        &self,
        os_user: &str,
        session_id: &str,
        project_id: &str,
        session_token: &str,
    ) -> Result<PullRequestSite, Status>;

    /// What is cached of `branch`'s PR in `project_id`, without any request. `None` when nothing is
    /// cached, the branch has no PR, or the project names no GitHub repository.
    fn cached_summary(
        &self,
        os_user: &str,
        project_id: &str,
        branch: &str,
    ) -> Option<PullRequestSummary>;
}

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
    let cached = inbox.peek_pr_for_head(repo, branch)?;
    cached.value.map(|pr| PullRequestSummary {
        number: pr.number,
        state: state_name(pr.state).to_string(),
    })
}

/// Fill `entry`'s PR from what the inbox has cached for its branch — number and state, or nothing.
/// A listing calls this per session, so it never waits on GitHub; an entry whose branch has no
/// cached PR (or that has no branch or project) is left without one.
pub fn apply_cached_pull_request(
    pull_requests: &dyn SessionPullRequests,
    os_user: &str,
    entry: &mut SessionEntry,
) {
    if entry.branch.is_empty() || entry.project_id.is_empty() {
        return;
    }
    if let Some(summary) = pull_requests.cached_summary(os_user, &entry.project_id, &entry.branch) {
        entry.pr_number = summary.number;
        entry.pr_state = summary.state;
    }
}

fn state_name(state: PrState) -> &'static str {
    match state {
        PrState::Open => "open",
        PrState::Draft => "draft",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
    }
}

fn check_view(run: CheckRun) -> PullRequestCheck {
    PullRequestCheck {
        name: run.name,
        status: match run.status {
            CheckStatus::Queued => "queued",
            CheckStatus::InProgress => "in_progress",
            CheckStatus::Completed => "completed",
        }
        .to_string(),
        conclusion: run.conclusion,
        started_at: run.started_at.unwrap_or_default(),
        completed_at: run.completed_at.unwrap_or_default(),
        details_url: run.details_url,
    }
}

/// `branch`'s PR view: the PR, its checks at the head, its conversation and its commits.
pub async fn pull_request_view(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    branch: &str,
) -> Result<SessionPullRequestView, InboxError> {
    read_view(inbox, repo, branch).await.map(|(view, _)| view)
}

/// The view and the head commit its checks were read at (`None` without a PR).
async fn read_view(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    branch: &str,
) -> Result<(SessionPullRequestView, Option<String>), InboxError> {
    let Some(pr) = inbox.pr_for_head(repo, branch).await?.value else {
        let view = SessionPullRequestView {
            head_branch: branch.to_string(),
            ..Default::default()
        };
        return Ok((view, None));
    };
    let checks = inbox.checks_at(repo, &pr.head_sha).await?.value;
    let conversation = inbox.conversation(repo, pr.number).await?.value;
    let commits = inbox.pr_commits(repo, pr.number, &pr.head_sha).await?.value;

    // Reviews first, then review comments (threaded by `in_reply_to_id`), then conversation comments.
    let reviews = conversation
        .reviews
        .into_iter()
        .map(|review| PullRequestComment {
            id: review.id,
            author: review.author,
            body: review.body,
            created_at: review.submitted_at,
            review_state: review.state,
            ..Default::default()
        });
    let review_comments =
        conversation
            .review_comments
            .into_iter()
            .map(|comment| PullRequestComment {
                id: comment.id,
                in_reply_to_id: comment.in_reply_to_id.unwrap_or(0),
                author: comment.author,
                body: comment.body,
                created_at: comment.created_at,
                path: comment.path,
                ..Default::default()
            });
    let issue_comments =
        conversation
            .issue_comments
            .into_iter()
            .map(|comment| PullRequestComment {
                id: comment.id,
                author: comment.author,
                body: comment.body,
                created_at: comment.created_at,
                ..Default::default()
            });

    let head_sha = pr.head_sha;
    let view = SessionPullRequestView {
        exists: true,
        number: pr.number,
        url: pr.url,
        title: pr.title,
        state: state_name(pr.state).to_string(),
        base_branch: pr.base_branch,
        head_branch: pr.head_branch,
        checks: checks.into_iter().map(check_view).collect(),
        comments: reviews
            .chain(review_comments)
            .chain(issue_comments)
            .collect(),
        commits: commits
            .into_iter()
            .map(|commit| PullRequestCommit {
                sha: commit.sha,
                subject: commit.subject,
                author: commit.author_name,
                authored_at: commit.author_date,
            })
            .collect(),
        unavailable_reason: String::new(),
    };
    Ok((view, Some(head_sha)))
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

/// Whether `event` changes a row the view of `branch` (currently showing `pr`) is made of.
fn concerns(event: &InboxEvent, branch: &str, pr: Option<&(u64, String)>) -> bool {
    match (&event.row, pr) {
        (RowKey::PrForHead { branch: changed }, _) => changed == branch,
        (RowKey::Pr { number } | RowKey::Conversation { number }, Some((shown, _)))
        | (RowKey::PrCommits { number, .. }, Some((shown, _))) => number == shown,
        (RowKey::ChecksAt { sha }, Some((_, head))) => sha == head,
        _ => false,
    }
}

/// The view as the stream sends it: a failed read is reported on the view, and the watch goes on.
async fn current_view(
    inbox: &GithubInbox,
    repo: &RepoSlug,
    branch: &str,
) -> (SessionPullRequestView, Option<String>) {
    match read_view(inbox, repo, branch).await {
        Ok(read) => read,
        Err(error) => {
            let view = SessionPullRequestView {
                head_branch: branch.to_string(),
                unavailable_reason: format!("{error:?}"),
                ..Default::default()
            };
            (view, None)
        }
    }
}

/// Watch `branch`'s PR view through `inbox`.
pub fn watch_pull_request(
    inbox: Arc<GithubInbox>,
    repo: RepoSlug,
    branch: String,
) -> PullRequestWatch {
    let (sender, receiver) = mpsc::unbounded_channel();
    // Subscribed before the first read, so a change landing during it is not missed.
    let mut events = inbox.subscribe(
        &repo,
        &[EventKind::Pr, EventKind::Checks, EventKind::Comments],
    );
    tokio::spawn(async move {
        let mut shown: Option<(u64, String)> = None;
        let mut changed = true;
        loop {
            if changed {
                let (view, head_sha) = current_view(&inbox, &repo, &branch).await;
                shown = head_sha.map(|head| (view.number, head));
                if sender.send(view).is_err() {
                    return;
                }
            }
            let event = tokio::select! {
                () = sender.closed() => return,
                event = events.next() => event,
            };
            let Some(event) = event else { return };
            changed = concerns(&event, &branch, shown.as_ref());
        }
    });
    PullRequestWatch { receiver }
}
