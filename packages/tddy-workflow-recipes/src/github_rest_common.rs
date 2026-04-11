//! Shared GitHub REST API constants and token resolution for **tddy-workflow-recipes** and consumers (e.g. **tddy-tools**).
//!
//! Keep `Accept` and `X-GitHub-Api-Version` in sync across merge-pr curl and tddy-tools GitHub PR helpers.

/// GitHub REST API version header value (REST API v2022-11-28).
pub const GITHUB_API_VERSION: &str = "2022-11-28";

/// Required `Accept` header for GitHub REST JSON responses.
pub const GITHUB_ACCEPT: &str = "application/vnd.github+json";

/// `User-Agent` for merge-pr workflow curl calls (historical identifier).
pub const USER_AGENT_MERGE_PR: &str = "tddy-coder-workflow-recipes";

/// `User-Agent` for **tddy-tools** GitHub PR MCP / REST calls.
pub const USER_AGENT_TDDY_TOOLS: &str = "tddy-tools";

/// Resolve GitHub token from the environment (`GITHUB_TOKEN` preferred, then `GH_TOKEN`).
#[must_use]
pub fn github_token_from_env() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::env::var("GH_TOKEN")
                .ok()
                .filter(|s| !s.trim().is_empty())
        })
}

/// True when a non-empty `GITHUB_TOKEN` or `GH_TOKEN` is set (prompt gating, merge-pr, etc.).
#[must_use]
pub fn github_env_token_present() -> bool {
    github_token_from_env().is_some()
}
