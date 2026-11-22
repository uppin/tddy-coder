//! The rows the inbox caches, and how each one is read from GitHub.
//!
//! A row is fetched conditionally: the stored ETag goes out, a `304` keeps the value, a `200`
//! replaces it. A row made of several requests (a PR's conversation) keeps one ETag per request.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use tddy_github::check_runs::parse_check_runs;
use tddy_github::conditional::{ConditionalAnswer, ConditionalRead, GithubReadError, RateLimit};
use tddy_github::pr_api::{CheckRun, CheckStatus, PrDetail};
use tddy_github::pr_json::{
    parse_first_pr, parse_issue_comments, parse_pr_body, parse_review_comments, parse_reviews,
};
use tddy_github::trunk_feed::{parse_commits, RepoSlug, TrunkCommit};

use crate::{paths, Cached, InboxError, PrCommit, PrConversation, RowKey};

/// What a row holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowValue {
    Pr(PrDetail),
    PrForHead(Option<PrDetail>),
    Checks(Vec<CheckRun>),
    Conversation(PrConversation),
    PrCommits(Vec<PrCommit>),
    BranchCommits(Vec<TrunkCommit>),
}

/// A cached row.
#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub value: RowValue,
    pub fetched_at_unix_ms: u64,
    /// One per request the row is made of.
    pub etags: Vec<Option<String>>,
    /// An event said this row changed: the next read revalidates it.
    pub stale: bool,
}

impl Row {
    pub(crate) fn cached(&self) -> Cached<RowValue> {
        Cached {
            value: self.value.clone(),
            fetched_at_unix_ms: self.fetched_at_unix_ms,
            // A multi-request row has no single ETag to hand out.
            etag: match self.etags.as_slice() {
                [only] => only.clone(),
                _ => None,
            },
        }
    }

    /// Whether this row holds check runs that have not finished.
    pub(crate) fn has_pending_checks(&self) -> bool {
        matches!(&self.value, RowValue::Checks(runs)
            if runs.iter().any(|run| run.status != CheckStatus::Completed))
    }
}

/// What reading a row produced.
pub(crate) struct Fetched {
    pub value: RowValue,
    pub etags: Vec<Option<String>>,
    /// Whether the value differs from the previous one (`true` for a first read).
    pub changed: bool,
}

/// What a run of reads cost.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    pub requests: u32,
    /// The tightest rate limit any answer reported.
    pub rate_limit: Option<RateLimit>,
}

impl Tally {
    pub(crate) fn observe(&mut self, rate_limit: Option<RateLimit>) {
        if let Some(seen) = rate_limit {
            if self
                .rate_limit
                .is_none_or(|tightest| seen.remaining < tightest.remaining)
            {
                self.rate_limit = Some(seen);
            }
        }
    }
}

pub(crate) fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn inbox_error(error: GithubReadError) -> InboxError {
    let limited = matches!(error.status, Some(403 | 429))
        && (error.retry_after_s.is_some()
            || error.rate_limit.is_some_and(|limit| limit.remaining == 0));
    match error.status {
        Some(404) => InboxError::NotFound,
        _ if limited => InboxError::RateLimited {
            reset_at_unix_s: error.rate_limit.map_or_else(
                || system_now_ms() / 1000 + error.retry_after_s.unwrap_or(0),
                |limit| limit.reset_at_unix_s,
            ),
        },
        _ => InboxError::Unavailable(error.message),
    }
}

/// The requests a row is made of, in order.
fn paths_of(repo: &RepoSlug, key: &RowKey) -> Vec<String> {
    match key {
        RowKey::Pr { number } => vec![paths::pull(repo, *number)],
        RowKey::PrForHead { branch } => vec![paths::pulls_for_head(repo, branch)],
        RowKey::ChecksAt { sha } => vec![paths::check_runs(repo, sha)],
        RowKey::Conversation { number } => vec![
            paths::reviews(repo, *number),
            paths::review_comments(repo, *number),
            paths::issue_comments(repo, *number),
        ],
        RowKey::PrCommits { number, .. } => vec![paths::pr_commits(repo, *number)],
        RowKey::BranchCommits { branch } => vec![paths::branch_commits(repo, branch)],
    }
}

/// Read one row, conditionally on `previous`'s ETags.
pub(crate) async fn fetch(
    reader: &dyn ConditionalRead,
    repo: &RepoSlug,
    key: &RowKey,
    previous: Option<&Row>,
    tally: &mut Tally,
) -> Result<Fetched, InboxError> {
    let paths = paths_of(repo, key);
    let mut bodies: Vec<Option<String>> = Vec::with_capacity(paths.len());
    let mut etags = Vec::with_capacity(paths.len());
    for (i, path) in paths.iter().enumerate() {
        let prior = previous.and_then(|row| row.etags.get(i).cloned().flatten());
        tally.requests += 1;
        match reader.get(path, prior.as_deref()).await {
            Err(error) => {
                tally.observe(error.rate_limit);
                return Err(inbox_error(error));
            }
            Ok(answer) => {
                tally.observe(answer.rate_limit());
                match answer {
                    ConditionalAnswer::NotModified { .. } => {
                        bodies.push(None);
                        etags.push(prior);
                    }
                    ConditionalAnswer::Modified { body, etag, .. } => {
                        bodies.push(Some(body));
                        etags.push(etag);
                    }
                }
            }
        }
    }
    let unavailable = |what: String| InboxError::Unavailable(format!("{key:?}: {what}"));
    let value = if bodies.iter().all(Option::is_none) {
        // Every request came back `304`: only possible with a previous value to keep.
        previous.map(|row| row.value.clone()).ok_or_else(|| {
            unavailable("GitHub answered 304 to an unconditional read".to_string())
        })?
    } else {
        parse(key, &bodies, previous.map(|row| &row.value)).map_err(unavailable)?
    };
    let changed = previous.is_none_or(|row| row.value != value);
    Ok(Fetched {
        value,
        etags,
        changed,
    })
}

/// Build a row's value from its bodies (`None` where that request was a `304`).
fn parse(
    key: &RowKey,
    bodies: &[Option<String>],
    previous: Option<&RowValue>,
) -> Result<RowValue, String> {
    let body = |i: usize| bodies[i].as_deref();
    Ok(match key {
        RowKey::Pr { .. } => RowValue::Pr(parse_pr_body(required(body(0))?)?),
        RowKey::PrForHead { .. } => RowValue::PrForHead(parse_first_pr(required(body(0))?)?),
        RowKey::ChecksAt { .. } => RowValue::Checks(parse_check_runs(required(body(0))?)?),
        RowKey::PrCommits { .. } => RowValue::PrCommits(parse_pr_commits(required(body(0))?)?),
        RowKey::BranchCommits { .. } => {
            RowValue::BranchCommits(parse_commits(required(body(0))?).map_err(|e| e.to_string())?)
        }
        RowKey::Conversation { .. } => {
            let kept = match previous {
                Some(RowValue::Conversation(conversation)) => conversation.clone(),
                _ => PrConversation::default(),
            };
            RowValue::Conversation(PrConversation {
                reviews: body(0).map_or(Ok(kept.reviews), parse_reviews)?,
                review_comments: body(1).map_or(Ok(kept.review_comments), parse_review_comments)?,
                issue_comments: body(2).map_or(Ok(kept.issue_comments), parse_issue_comments)?,
            })
        }
    })
}

fn required(body: Option<&str>) -> Result<&str, String> {
    body.ok_or_else(|| "GitHub answered 304 with nothing to keep".to_string())
}

/// A `GET pulls/{n}/commits` body.
fn parse_pr_commits(body: &str) -> Result<Vec<PrCommit>, String> {
    let json: Value = serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))?;
    let items = json
        .as_array()
        .ok_or_else(|| "the commits answer is not an array".to_string())?;
    items
        .iter()
        .map(|item| {
            let text = |path: &str| {
                item.pointer(path)
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            let sha = text("/sha");
            if sha.is_empty() {
                return Err("a commit has no sha".to_string());
            }
            Ok(PrCommit {
                sha,
                author_name: text("/commit/author/name"),
                author_date: text("/commit/author/date"),
                subject: text("/commit/message")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}
