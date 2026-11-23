//! Conditional GitHub reads — the transport under the daemon's GitHub inbox (`#workqueue` 2/14).
//!
//! Every answer carries what a shared, quota-aware reader needs: whether anything changed (`304`
//! costs no quota), the ETag to send next time, GitHub's `X-Poll-Interval`, and the rate-limit
//! headers. Two transports, matching the two credentials the daemon already reads with: a token over
//! REST, and the host's `gh` login (`gh api -i`, where a `304` exits non-zero and is still a success).

use std::sync::Arc;

use async_trait::async_trait;

use crate::trunk_feed::{seconds_to_ms, split_head_body, GhApi, TokenSource};

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

/// What every answer's headers carry, whatever its status.
#[derive(Default)]
struct AnswerHeaders {
    etag: Option<String>,
    poll_interval_ms: Option<u64>,
    rate_limit: Option<RateLimit>,
    retry_after_s: Option<u64>,
}

impl AnswerHeaders {
    /// `header` looks a header up by lowercase name.
    fn read(header: impl Fn(&str) -> Option<String>) -> Self {
        let number = |name: &str| header(name).and_then(|v| v.trim().parse::<u64>().ok());
        let rate_limit = match (number("x-ratelimit-remaining"), number("x-ratelimit-reset")) {
            (Some(remaining), Some(reset_at_unix_s)) => Some(RateLimit {
                remaining: u32::try_from(remaining).unwrap_or(u32::MAX),
                reset_at_unix_s,
            }),
            _ => None,
        };
        Self {
            etag: header("etag"),
            poll_interval_ms: header("x-poll-interval").and_then(|v| seconds_to_ms(&v)),
            rate_limit,
            retry_after_s: number("retry-after"),
        }
    }

    /// The answer for `status`: `304`, a `2xx` with `body`, or a [`GithubReadError`].
    fn into_answer(
        self,
        status: u16,
        body: impl FnOnce() -> String,
        failure: impl FnOnce() -> String,
    ) -> Result<ConditionalAnswer, GithubReadError> {
        match status {
            304 => Ok(ConditionalAnswer::NotModified {
                poll_interval_ms: self.poll_interval_ms,
                rate_limit: self.rate_limit,
            }),
            200..=299 => Ok(ConditionalAnswer::Modified {
                body: body(),
                etag: self.etag,
                poll_interval_ms: self.poll_interval_ms,
                rate_limit: self.rate_limit,
            }),
            other => Err(GithubReadError {
                status: Some(other),
                message: failure(),
                retry_after_s: self.retry_after_s,
                rate_limit: self.rate_limit,
            }),
        }
    }
}

fn transport_error(message: impl Into<String>) -> GithubReadError {
    GithubReadError {
        status: None,
        message: message.into(),
        retry_after_s: None,
        rate_limit: None,
    }
}

/// [`ConditionalRead`] over REST with a token resolved on every read.
pub struct RestConditionalRead {
    api_base: String,
    token: TokenSource,
    client: reqwest::Client,
}

impl RestConditionalRead {
    pub fn new(api_base: impl Into<String>, token: TokenSource) -> Self {
        Self {
            api_base: api_base.into(),
            token,
            client: reqwest::Client::new(),
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
        let token = (self.token)().map_err(|e| transport_error(e.to_string()))?;
        let mut request = self
            .client
            .get(format!("{}/{path}", self.api_base.trim_end_matches('/')))
            .bearer_auth(token)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "tddy-github-inbox")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(etag) = if_none_match {
            request = request.header("If-None-Match", etag);
        }
        let response = request
            .send()
            .await
            .map_err(|e| transport_error(e.to_string()))?;
        let status = response.status().as_u16();
        let headers = AnswerHeaders::read(|name| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        });
        let body = if status == 304 {
            String::new()
        } else {
            response
                .text()
                .await
                .map_err(|e| transport_error(e.to_string()))?
        };
        headers.into_answer(
            status,
            || body.clone(),
            || format!("GitHub answered HTTP {status} for {path}: {body}"),
        )
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
        let out = self
            .gh
            .api_with_headers(path, if_none_match)
            .await
            .map_err(|e| transport_error(e.to_string()))?;
        let (head, body) = split_head_body(&out.stdout);
        let mut lines = head.lines();
        // `gh` exits non-zero for any non-2xx, a 304 included: the status line decides.
        let status: u16 = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .ok_or_else(|| {
                transport_error(format!(
                    "gh printed no HTTP status line for {path} (exit {})",
                    out.exit_code
                ))
            })?;
        let headers = AnswerHeaders::read(|name| {
            head.lines().skip(1).find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.trim()
                    .eq_ignore_ascii_case(name)
                    .then(|| value.trim().to_string())
            })
        });
        headers.into_answer(
            status,
            || body.to_string(),
            || {
                format!(
                    "gh answered HTTP {status} for {path} (exit {}): {body}",
                    out.exit_code
                )
            },
        )
    }
}
