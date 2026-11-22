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

/// Where [`RestTrunkFeed`] reads its GitHub token from, once per read: a token rotated in the vault
/// is used from the next poll on, and one revoked fails that poll.
pub type TokenSource = Arc<dyn Fn() -> Result<String, TrunkReadError> + Send + Sync>;

/// [`TrunkFeed`] over the REST API with a GitHub token.
pub struct RestTrunkFeed {
    token: TokenSource,
}

impl RestTrunkFeed {
    pub fn new(token: TokenSource) -> Self {
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
        let url = format!("https://api.github.com/{}", commits_path(repo, branch));
        let token = (self.token)()?;
        let mut request = reqwest::Client::new()
            .get(url)
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "tddy-trunk-watch")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let response = request
            .send()
            .await
            .map_err(|e| TrunkReadError(e.to_string()))?;
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let poll_interval_ms = header("x-poll-interval").and_then(|v| seconds_to_ms(&v));
        let response_etag = header("etag");
        match status {
            304 => Ok(Conditional::NotModified { poll_interval_ms }),
            200 => {
                let body = response
                    .text()
                    .await
                    .map_err(|e| TrunkReadError(e.to_string()))?;
                Ok(Conditional::Modified {
                    value: parse_commits(&body)?,
                    etag: response_etag,
                    poll_interval_ms,
                })
            }
            other => Err(TrunkReadError(format!("GitHub answered HTTP {other}"))),
        }
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
        let out = self
            .gh
            .api_with_headers(&commits_path(repo, branch), etag)
            .await?;
        let (head, body) = split_head_body(&out.stdout);
        let mut lines = head.lines();
        let status: u16 = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse().ok())
            .ok_or_else(|| TrunkReadError("gh printed no HTTP status line".to_string()))?;
        let header = |name: &str| {
            head.lines().skip(1).find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.trim()
                    .eq_ignore_ascii_case(name)
                    .then(|| v.trim().to_string())
            })
        };
        let poll_interval_ms = header("x-poll-interval").and_then(|v| seconds_to_ms(&v));
        match status {
            // `gh` exits non-zero for any non-2xx, a 304 included: the status line decides.
            304 => Ok(Conditional::NotModified { poll_interval_ms }),
            200 => Ok(Conditional::Modified {
                value: parse_commits(body)?,
                etag: header("etag"),
                poll_interval_ms,
            }),
            other => Err(TrunkReadError(format!(
                "gh answered HTTP {other} (exit {})",
                out.exit_code
            ))),
        }
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
    etag: Option<String>,
    last_tip: Option<String>,
    last_unconditional_at: Option<u64>,
}

impl TrunkPoller {
    pub fn new(feed: Arc<dyn TrunkFeed>, repo: RepoSlug, branch: String) -> Self {
        Self {
            feed,
            repo,
            branch,
            etag: None,
            last_tip: None,
            last_unconditional_at: None,
        }
    }

    /// Fold an answer (newest first) into the last tip seen and say what changed.
    ///
    /// A last tip missing from the answer's window is read as a rewrite; a burst of more commits
    /// than the window holds between two polls would read the same way.
    fn absorb(&mut self, newest_first: Vec<TrunkCommit>) -> Vec<TrunkChange> {
        let Some(tip) = newest_first.first().cloned() else {
            return vec![];
        };
        let previous = self.last_tip.replace(tip.sha.clone());
        let Some(previous) = previous else {
            return vec![];
        };
        if previous == tip.sha {
            return vec![];
        }
        match newest_first.iter().position(|c| c.sha == previous) {
            Some(i) => vec![TrunkChange::Landed(
                newest_first[..i].iter().rev().cloned().collect(),
            )],
            None => vec![TrunkChange::Rewritten {
                from: previous,
                to: tip,
            }],
        }
    }

    /// Poll once at `now_unix_ms` (call it no sooner than the last outcome's
    /// `next_poll_at_unix_ms`).
    pub async fn poll(&mut self, now_unix_ms: u64) -> PollOutcome {
        let sweep = self
            .last_unconditional_at
            .is_none_or(|at| now_unix_ms.saturating_sub(at) >= SWEEP_EVERY_MS);
        let etag = if sweep { None } else { self.etag.clone() };
        if sweep {
            self.last_unconditional_at = Some(now_unix_ms);
        }
        let answer = self
            .feed
            .newest_commits(&self.repo, &self.branch, etag.as_deref())
            .await;
        let (changes, wait_ms) = match answer {
            Err(e) => {
                log::warn!(
                    "trunk poll of {}/{} failed: {e}",
                    self.repo.owner,
                    self.repo.name
                );
                (vec![], ERROR_BACKOFF_MS)
            }
            Ok(Conditional::NotModified { poll_interval_ms }) => {
                (vec![], poll_interval_ms.unwrap_or(DEFAULT_POLL_INTERVAL_MS))
            }
            Ok(Conditional::Modified {
                value,
                etag,
                poll_interval_ms,
            }) => {
                self.etag = etag;
                let changes = self.absorb(value);
                (
                    changes,
                    poll_interval_ms.unwrap_or(DEFAULT_POLL_INTERVAL_MS),
                )
            }
        };
        PollOutcome {
            changes,
            next_poll_at_unix_ms: now_unix_ms + wait_ms,
        }
    }
}

fn commits_path(repo: &RepoSlug, branch: &str) -> String {
    format!(
        "repos/{}/{}/commits?sha={branch}&per_page=100",
        repo.owner, repo.name
    )
}

pub(crate) fn seconds_to_ms(seconds: &str) -> Option<u64> {
    seconds.trim().parse::<u64>().ok().map(|s| s * 1000)
}

pub(crate) fn split_head_body(raw: &str) -> (&str, &str) {
    match (raw.find("\r\n\r\n"), raw.find("\n\n")) {
        (Some(i), _) => (&raw[..i], &raw[i + 4..]),
        (None, Some(i)) => (&raw[..i], &raw[i + 2..]),
        (None, None) => (raw, ""),
    }
}

/// The commits array of `GET /repos/{o}/{r}/commits`, newest first as GitHub sends it.
pub fn parse_commits(body: &str) -> Result<Vec<TrunkCommit>, TrunkReadError> {
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| TrunkReadError(format!("not JSON: {e}")))?;
    let items = json
        .as_array()
        .ok_or_else(|| TrunkReadError("the commits answer is not an array".to_string()))?;
    items.iter().map(parse_commit).collect()
}

fn parse_commit(item: &serde_json::Value) -> Result<TrunkCommit, TrunkReadError> {
    let text = |v: Option<&serde_json::Value>| v.and_then(|v| v.as_str()).map(str::to_string);
    let sha =
        text(item.get("sha")).ok_or_else(|| TrunkReadError("a commit has no sha".to_string()))?;
    let commit = item.get("commit");
    let message = text(commit.and_then(|c| c.get("message"))).unwrap_or_default();
    let author = commit.and_then(|c| c.get("author"));
    Ok(TrunkCommit {
        sha,
        author: text(author.and_then(|a| a.get("name"))).unwrap_or_default(),
        subject: message.lines().next().unwrap_or_default().to_string(),
        committed_at_unix_ms: text(author.and_then(|a| a.get("date")))
            .and_then(|d| rfc3339_to_unix_ms(&d))
            .unwrap_or(0),
        parent: text(
            item.get("parents")
                .and_then(|p| p.get(0))
                .and_then(|p| p.get("sha")),
        )
        .unwrap_or_default(),
    })
}

/// `2026-11-21T10:00:00Z` (UTC, as GitHub prints it) to unix milliseconds.
fn rfc3339_to_unix_ms(date: &str) -> Option<u64> {
    let (d, t) = date.trim_end_matches('Z').split_once('T')?;
    let mut dp = d.split('-').map(|n| n.parse::<i64>().ok());
    let (y, m, day) = (dp.next()??, dp.next()??, dp.next()??);
    let mut tp = t.split(':').map(|n| n.parse::<i64>().ok());
    let (hh, mm, ss) = (tp.next()??, tp.next()??, tp.next()??);
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(((days * 24 + hh) * 60 + mm) * 60 + ss)
        .ok()
        .map(|s| s * 1000)
}
