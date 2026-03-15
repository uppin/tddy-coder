use async_trait::async_trait;

/// Domain model for a GitHub user (not proto — converted in auth_service).
#[derive(Debug, Clone)]
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
}
