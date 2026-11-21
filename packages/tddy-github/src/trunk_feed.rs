//! Watching a repository's trunk branch for new commits, cheaply (`#orchestra` 5/9).
//!
//! [`TrunkFeed`] reads the branch's newest commits **conditionally**: it sends the ETag of its last
//! answer, and a `304 Not Modified` costs nothing against the rate limit and changes nothing. Two
//! transports, chosen by the project's GitHub setting: [`RestTrunkFeed`] over the REST API with the
//! project's GitHub account token, and [`GhCliTrunkFeed`] over `gh api -i` as the session's OS
//! user, for a project that uses the host's own `gh` login. Through `gh`, a 304 exits non-zero and
//! is read as not-modified, never as a failure.
//!
//! [`TrunkPoller`] is one repository's polling rule as a step with no loop and no clock of its own,
//! so every rule is testable with a hand clock:
//!
//! - the first answer is the baseline: nothing that was already on the trunk is reported;
//! - after that, each new commit is reported once, oldest first; a tip the last answer does not
//!   descend from is reported as a rewrite;
//! - the next poll waits out the answer's `X-Poll-Interval` (default [`DEFAULT_POLL_INTERVAL_MS`]);
//!   after a failed read, at least [`ERROR_BACKOFF_MS`];
//! - every [`SWEEP_EVERY_MS`] the read is unconditional, for anything a cached answer hid.

use std::sync::Arc;

use async_trait::async_trait;

/// The wait between polls when GitHub names none.
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 60_000;
/// The least wait after a failed read.
pub const ERROR_BACKOFF_MS: u64 = 60_000;
/// How often a poll ignores its ETag.
pub const SWEEP_EVERY_MS: u64 = 15 * 60_000;

/// `owner/name` of a GitHub repository.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoSlug {
    pub owner: String,
    pub name: String,
}

/// One commit on the trunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrunkCommit {
    pub sha: String,
    pub author: String,
    /// The first line of its message.
    pub subject: String,
    pub committed_at_unix_ms: u64,
    /// Its first parent's sha; empty for a root commit.
    pub parent: String,
}

/// A conditional read's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conditional<T> {
    /// Nothing changed since the ETag sent.
    NotModified { poll_interval_ms: Option<u64> },
    /// The current value, with the ETag to send next time.
    Modified {
        value: T,
        etag: Option<String>,
        poll_interval_ms: Option<u64>,
    },
}

/// Why a trunk read failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrunkReadError(pub String);

impl std::fmt::Display for TrunkReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the trunk could not be read: {}", self.0)
    }
}

impl std::error::Error for TrunkReadError {}

/// The trunk's newest commits, newest first, read conditionally on `etag`.
#[async_trait]
pub trait TrunkFeed: Send + Sync {
    async fn newest_commits(
        &self,
        repo: &RepoSlug,
        branch: &str,
        etag: Option<&str>,
    ) -> Result<Conditional<Vec<TrunkCommit>>, TrunkReadError>;
}

/// [`TrunkFeed`] over the REST API with a GitHub token.
pub struct RestTrunkFeed {
    token: String,
}

impl RestTrunkFeed {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}

#[async_trait]
impl TrunkFeed for RestTrunkFeed {
    async fn newest_commits(
        &self,
        repo: &RepoSlug,
        branch: &str,
        etag: Option<&str>,
    ) -> Result<Conditional<Vec<TrunkCommit>>, TrunkReadError> {
        // TODO(trunk-watch): implement
        let _ = (&self.token, repo, branch, etag);
        unimplemented!("RestTrunkFeed::newest_commits")
    }
}

/// What `gh api -i` printed and how it exited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhApiOutput {
    /// The status line, headers, a blank line, then the body.
    pub stdout: String,
    pub exit_code: i32,
}

/// Runs `gh api -i <path>` as the session's OS user, with `If-None-Match: <etag>` when given.
#[async_trait]
pub trait GhApi: Send + Sync {
    async fn api_with_headers(
        &self,
        path: &str,
        if_none_match: Option<&str>,
    ) -> Result<GhApiOutput, TrunkReadError>;
}

/// [`TrunkFeed`] over the host's `gh` login.
pub struct GhCliTrunkFeed {
    gh: Arc<dyn GhApi>,
}

impl GhCliTrunkFeed {
    pub fn new(gh: Arc<dyn GhApi>) -> Self {
        Self { gh }
    }
}

#[async_trait]
impl TrunkFeed for GhCliTrunkFeed {
    async fn newest_commits(
        &self,
        repo: &RepoSlug,
        branch: &str,
        etag: Option<&str>,
    ) -> Result<Conditional<Vec<TrunkCommit>>, TrunkReadError> {
        // TODO(trunk-watch): implement
        let _ = (&self.gh, repo, branch, etag);
        unimplemented!("GhCliTrunkFeed::newest_commits")
    }
}

/// What one poll found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrunkChange {
    /// New commits, oldest first.
    Landed(Vec<TrunkCommit>),
    /// The tip no longer descends from the one last seen: a force-push.
    Rewritten { from: String, to: TrunkCommit },
}

/// One poll's outcome: what changed, and when the next poll is due.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollOutcome {
    pub changes: Vec<TrunkChange>,
    pub next_poll_at_unix_ms: u64,
}

/// One repository branch's polling state.
pub struct TrunkPoller {
    feed: Arc<dyn TrunkFeed>,
    repo: RepoSlug,
    branch: String,
}

impl TrunkPoller {
    pub fn new(feed: Arc<dyn TrunkFeed>, repo: RepoSlug, branch: String) -> Self {
        Self { feed, repo, branch }
    }

    /// Poll once at `now_unix_ms` (call it no sooner than the last outcome's
    /// `next_poll_at_unix_ms`).
    pub async fn poll(&mut self, now_unix_ms: u64) -> PollOutcome {
        // TODO(trunk-watch): implement
        let _ = (&self.feed, &self.repo, &self.branch, now_unix_ms);
        unimplemented!("TrunkPoller::poll")
    }
}
