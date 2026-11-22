//! Conditional GitHub reads — the transport under the daemon's GitHub inbox (`#workqueue` 2/14).
//!
//! Every answer carries what a shared, quota-aware reader needs: whether anything changed (`304`
//! costs no quota), the ETag to send next time, GitHub's `X-Poll-Interval`, and the rate-limit
//! headers. Two transports, matching the two credentials the daemon already reads with: a token over
//! REST, and the host's `gh` login (`gh api -i`, where a `304` exits non-zero and is still a success).

use std::sync::Arc;

use async_trait::async_trait;

use crate::trunk_feed::{GhApi, TokenSource};

/// The REST API root the daemon reads (github.com only — see the trunk watch's recorded gap).
pub const GITHUB_API_BASE: &str = "https://api.github.com";

/// `X-RateLimit-Remaining` and `X-RateLimit-Reset` as one answer reported them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub remaining: u32,
    /// Seconds since the epoch at which the window resets.
    pub reset_at_unix_s: u64,
}

/// What a conditional `GET` answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionalAnswer {
    /// `304`: the ETag sent still names the current value.
    NotModified {
        poll_interval_ms: Option<u64>,
        rate_limit: Option<RateLimit>,
    },
    /// `2xx`: the current body, with the ETag to send next time.
    Modified {
        body: String,
        etag: Option<String>,
        poll_interval_ms: Option<u64>,
        rate_limit: Option<RateLimit>,
    },
}

impl ConditionalAnswer {
    /// The rate-limit headers of this answer, whichever kind it is.
    pub fn rate_limit(&self) -> Option<RateLimit> {
        match self {
            Self::NotModified { rate_limit, .. } | Self::Modified { rate_limit, .. } => *rate_limit,
        }
    }

    /// GitHub's `X-Poll-Interval`, in milliseconds, whichever kind it is.
    pub fn poll_interval_ms(&self) -> Option<u64> {
        match self {
            Self::NotModified {
                poll_interval_ms, ..
            }
            | Self::Modified {
                poll_interval_ms, ..
            } => *poll_interval_ms,
        }
    }
}

/// A read that did not produce an answer: transport failure, or a non-2xx/304 status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubReadError {
    /// The HTTP status, when one was received.
    pub status: Option<u16>,
    pub message: String,
    /// `Retry-After`, in seconds, when GitHub sent one (secondary rate limits).
    pub retry_after_s: Option<u64>,
    pub rate_limit: Option<RateLimit>,
}

/// A conditional `GET` against the GitHub REST API.
#[async_trait]
pub trait ConditionalRead: Send + Sync {
    /// `path` is relative to the API root and carries its query (`repos/o/n/pulls/1`).
    async fn get(
        &self,
        path: &str,
        if_none_match: Option<&str>,
    ) -> Result<ConditionalAnswer, GithubReadError>;
}

/// [`ConditionalRead`] over REST with a token resolved on every read.
pub struct RestConditionalRead {
    api_base: String,
    token: TokenSource,
}

impl RestConditionalRead {
    pub fn new(api_base: impl Into<String>, token: TokenSource) -> Self {
        Self {
            api_base: api_base.into(),
            token,
        }
    }
}

#[async_trait]
impl ConditionalRead for RestConditionalRead {
    async fn get(
        &self,
        path: &str,
        if_none_match: Option<&str>,
    ) -> Result<ConditionalAnswer, GithubReadError> {
        // TODO(n2 github-event-inbox): implement — reqwest GET `{api_base}/{path}` with the token,
        // `If-None-Match`, the API version header; map 304/2xx/other and read every header above.
        let _ = (&self.api_base, &self.token, path, if_none_match);
        todo!("TODO(n2): implement RestConditionalRead::get")
    }
}

/// [`ConditionalRead`] over the host's `gh` login (`gh api -i`).
pub struct GhCliConditionalRead {
    gh: Arc<dyn GhApi>,
}

impl GhCliConditionalRead {
    pub fn new(gh: Arc<dyn GhApi>) -> Self {
        Self { gh }
    }
}

#[async_trait]
impl ConditionalRead for GhCliConditionalRead {
    async fn get(
        &self,
        path: &str,
        if_none_match: Option<&str>,
    ) -> Result<ConditionalAnswer, GithubReadError> {
        // TODO(n2 github-event-inbox): implement — parse the status line and headers `gh api -i`
        // prints; the status line decides, never the exit code (a 304 exits 1).
        let _ = (&self.gh, path, if_none_match);
        todo!("TODO(n2): implement GhCliConditionalRead::get")
    }
}
