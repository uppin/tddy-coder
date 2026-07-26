use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Domain model for a GitHub user (not proto — converted in auth_service).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    pub id: u64,
    pub login: String,
    pub avatar_url: String,
    pub name: String,
}

/// Trait abstracting GitHub OAuth operations. Implementations provide either
/// real GitHub API calls or an in-memory stub for testing.
#[async_trait]
pub trait GitHubOAuthProvider: Send + Sync + 'static {
    /// Generate the OAuth authorize URL and a CSRF state token.
    /// Returns (authorize_url, state).
    fn authorize_url(&self) -> (String, String);

    /// Exchange an authorization code for an access token and fetch user info.
    /// The state parameter must match one previously issued by authorize_url.
    /// Returns (access_token, user) on success.
    async fn exchange_code(&self, code: &str, state: &str) -> Result<(String, GitHubUser), String>;

    /// Whether [`Self::exchange_code`]'s access token is a real GitHub credential that can
    /// authenticate API calls.
    ///
    /// `false` for the in-memory stub, whose token is synthetic — GitHub would reject it, so it must
    /// never be retained: a demo login holds no credential *by construction*, and its PR lookups
    /// resolve to a clean empty result rather than to an error (PR-stack UX recovery, D12).
    fn issues_usable_access_token(&self) -> bool;
}
