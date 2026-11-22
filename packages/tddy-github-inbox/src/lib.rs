//! The daemon's GitHub inbox (`#workqueue` 2/14).
//!
//! **One inbox per repository**: it reads the repository's activity feed conditionally (a `304`
//! costs nothing; `X-Poll-Interval` is honoured), sweeps recently updated pull requests on a slower
//! cadence for what the feed misses, and polls check runs only for heads whose checks are still
//! queued or running. What it reads lands in **rows** — a PR, the PR for a head branch, the checks at
//! a commit, a PR's conversation, a PR's commits, a branch's newest commits — each kept with its own
//! ETag. An event **invalidates** exactly the rows it affects; the next read revalidates with the
//! stored ETag. Concurrent reads of one stale row make **one** request. Subscribers register for a
//! repository and the **event kinds** they want, and hear when a matching row changes.
//!
//! Time is injected (`step(now)`), never read from a clock, so the feed is tested without sleeping.

mod feed;
mod in_flight;
mod pacing;
pub mod paths;
mod repo;
mod rows;
mod subscriptions;
mod trunk;

use std::sync::Arc;

use std::collections::HashMap;
use std::sync::Mutex;

use tddy_github::conditional::{ConditionalRead, RateLimit};
use tddy_github::pr_api::{CheckRun, PrDetail, PrIssueComment, PrReview, PrReviewComment};
pub use tddy_github::trunk_feed::{RepoSlug, TrunkCommit};
use tokio::sync::mpsc;

use repo::RepoInbox;
use rows::{RowValue, Tally};

pub use trunk::InboxTrunkFeed;

/// Below this many remaining requests the feed stops polling until the rate-limit window resets;
/// reads keep answering from their cached rows.
pub const LOW_QUOTA_REMAINING: u32 = 100;

/// What kind of change a subscriber hears about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// A pull request opened, edited, retargeted, synchronized, merged or closed.
    Pr,
    /// A check run at some head changed status or concluded.
    Checks,
    /// A review, review comment or conversation comment was added or changed.
    Comments,
    /// A branch moved.
    Push,
}

/// One cached row of a repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RowKey {
    Pr { number: u64 },
    PrForHead { branch: String },
    ChecksAt { sha: String },
    Conversation { number: u64 },
    PrCommits { number: u64, head_sha: String },
    BranchCommits { branch: String },
}

/// A row's value as last read, with the ETag that revalidates it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cached<T> {
    pub value: T,
    pub fetched_at_unix_ms: u64,
    pub etag: Option<String>,
}

/// A PR's reviews (with their state), review comments and conversation comments.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrConversation {
    pub reviews: Vec<PrReview>,
    pub review_comments: Vec<PrReviewComment>,
    pub issue_comments: Vec<PrIssueComment>,
}

/// One commit of a PR, as its commit list reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrCommit {
    pub sha: String,
    pub author_name: String,
    /// ISO-8601 author date (survives a rebase; the SHA does not).
    pub author_date: String,
    /// The first line of the message.
    pub subject: String,
}

/// What a subscriber is told: a row of `repo` changed, of kind `kind`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboxEvent {
    pub repo: RepoSlug,
    pub kind: EventKind,
    pub row: RowKey,
}

/// Why an inbox read produced no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxError {
    /// No credential reads this repository, or GitHub could not be reached.
    Unavailable(String),
    /// The quota is spent and nothing is cached for this row.
    RateLimited { reset_at_unix_s: u64 },
    /// GitHub answered 404.
    NotFound,
}

/// Which transport reads a repository — the project's vault account over REST, or the host's `gh`.
/// Resolved on every read, so a rotated credential is picked up without a restart.
pub trait RepoReaders: Send + Sync {
    fn reader_for(&self, repo: &RepoSlug) -> Result<Arc<dyn ConditionalRead>, InboxError>;
}

/// A subscription: the events of one repository, filtered to some kinds. Dropping it unsubscribes;
/// a repository with no subscriber left stops its feed.
pub struct InboxSubscription {
    receiver: mpsc::UnboundedReceiver<InboxEvent>,
}

impl InboxSubscription {
    /// The next event already delivered, without waiting.
    pub fn try_next(&mut self) -> Option<InboxEvent> {
        self.receiver.try_recv().ok()
    }

    /// The next event, waiting for it; `None` once the inbox is gone.
    pub async fn next(&mut self) -> Option<InboxEvent> {
        self.receiver.recv().await
    }
}

/// What one `step` did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StepReport {
    /// GitHub requests the step made, across every repository.
    pub requests: u32,
    /// When the next step has something to do.
    pub next_step_at_unix_ms: Option<u64>,
    /// The tightest rate limit any answer reported.
    pub rate_limit: Option<RateLimit>,
}

/// The daemon's GitHub inbox. One per daemon; every repository it reads has its own feed inside.
pub struct GithubInbox {
    readers: Arc<dyn RepoReaders>,
    repos: Mutex<HashMap<RepoSlug, Arc<RepoInbox>>>,
}

/// Narrow a row to the value type its key promises.
fn typed<T>(
    cached: Cached<RowValue>,
    pick: impl FnOnce(RowValue) -> Option<T>,
) -> Result<Cached<T>, InboxError> {
    let Cached {
        value,
        fetched_at_unix_ms,
        etag,
    } = cached;
    let value = pick(value)
        .ok_or_else(|| InboxError::Unavailable("a row held a value of another kind".to_string()))?;
    Ok(Cached {
        value,
        fetched_at_unix_ms,
        etag,
    })
}

impl GithubInbox {
    pub fn new(readers: Arc<dyn RepoReaders>) -> Self {
        Self {
            readers,
            repos: Mutex::new(HashMap::new()),
        }
    }

    fn repo(&self, repo: &RepoSlug) -> Arc<RepoInbox> {
        self.repos
            .lock()
            .expect("repository table poisoned")
            .entry(repo.clone())
            .or_insert_with(|| Arc::new(RepoInbox::new(repo.clone(), self.readers.clone())))
            .clone()
    }

    /// Hear about `kinds` of change in `repo`. The repository's feed starts with its first subscriber.
    pub fn subscribe(&self, repo: &RepoSlug, kinds: &[EventKind]) -> InboxSubscription {
        self.repo(repo).state().subscribers.add(kinds)
    }

    /// Advance every repository's feed to `now_unix_ms`: events (conditional), the sweep, the
    /// pending-check poll, pacing by `X-Poll-Interval` and the rate limit. A repository nobody
    /// subscribes to is not read.
    pub async fn step(&self, now_unix_ms: u64) -> StepReport {
        let repos: Vec<Arc<RepoInbox>> = self
            .repos
            .lock()
            .expect("repository table poisoned")
            .values()
            .cloned()
            .collect();
        let mut tally = Tally::default();
        let mut next_step_at: Option<u64> = None;
        for inbox in repos {
            if !inbox.state().subscribers.any_listening() {
                continue;
            }
            feed::step_repo(&inbox, now_unix_ms, &mut tally).await;
            let state = inbox.state();
            let due = state
                .pacing
                .next_due(!state.pending_check_rows().is_empty());
            next_step_at = Some(next_step_at.map_or(due, |earliest| earliest.min(due)));
        }
        StepReport {
            requests: tally.requests,
            next_step_at_unix_ms: next_step_at,
            rate_limit: tally.rate_limit,
        }
    }

    /// A pull request by number.
    pub async fn pr(&self, repo: &RepoSlug, number: u64) -> Result<Cached<PrDetail>, InboxError> {
        let row = self.repo(repo).read(&RowKey::Pr { number }).await?;
        typed(row, |value| match value {
            RowValue::Pr(pr) => Some(pr),
            _ => None,
        })
    }

    /// The pull request whose head is `branch` (open or not), `None` when there is none.
    pub async fn pr_for_head(
        &self,
        repo: &RepoSlug,
        branch: &str,
    ) -> Result<Cached<Option<PrDetail>>, InboxError> {
        let key = RowKey::PrForHead {
            branch: branch.to_string(),
        };
        typed(self.repo(repo).read(&key).await?, |value| match value {
            RowValue::PrForHead(pr) => Some(pr),
            _ => None,
        })
    }

    /// What is cached for `branch`'s PR, without any request — for callers that must never wait on
    /// GitHub (session listing).
    pub fn peek_pr_for_head(
        &self,
        repo: &RepoSlug,
        branch: &str,
    ) -> Option<Cached<Option<PrDetail>>> {
        let key = RowKey::PrForHead {
            branch: branch.to_string(),
        };
        let row = self.repo(repo).peek(&key)?;
        typed(row, |value| match value {
            RowValue::PrForHead(pr) => Some(pr),
            _ => None,
        })
        .ok()
    }

    /// The check runs at `sha`. A head with a queued or running run joins the pending-check poll.
    pub async fn checks_at(
        &self,
        repo: &RepoSlug,
        sha: &str,
    ) -> Result<Cached<Vec<CheckRun>>, InboxError> {
        let key = RowKey::ChecksAt {
            sha: sha.to_string(),
        };
        typed(self.repo(repo).read(&key).await?, |value| match value {
            RowValue::Checks(runs) => Some(runs),
            _ => None,
        })
    }

    /// A PR's reviews (with state), review comments and conversation comments.
    pub async fn conversation(
        &self,
        repo: &RepoSlug,
        number: u64,
    ) -> Result<Cached<PrConversation>, InboxError> {
        let key = RowKey::Conversation { number };
        typed(self.repo(repo).read(&key).await?, |value| match value {
            RowValue::Conversation(conversation) => Some(conversation),
            _ => None,
        })
    }

    /// A PR's commits at `head_sha` — read once per head.
    pub async fn pr_commits(
        &self,
        repo: &RepoSlug,
        number: u64,
        head_sha: &str,
    ) -> Result<Cached<Vec<PrCommit>>, InboxError> {
        let key = RowKey::PrCommits {
            number,
            head_sha: head_sha.to_string(),
        };
        typed(self.repo(repo).read(&key).await?, |value| match value {
            RowValue::PrCommits(commits) => Some(commits),
            _ => None,
        })
    }

    /// `branch`'s newest commits, newest first (the trunk watch's source).
    pub async fn branch_commits(
        &self,
        repo: &RepoSlug,
        branch: &str,
    ) -> Result<Cached<Vec<TrunkCommit>>, InboxError> {
        let key = RowKey::BranchCommits {
            branch: branch.to_string(),
        };
        typed(self.repo(repo).read(&key).await?, |value| match value {
            RowValue::BranchCommits(commits) => Some(commits),
            _ => None,
        })
    }
}
