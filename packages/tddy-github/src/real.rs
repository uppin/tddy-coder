use std::collections::HashSet;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::Deserialize;
use uuid::Uuid;

use crate::provider::{DeviceLoginPoll, DeviceLoginStart, GitHubOAuthProvider, GitHubUser};

/// Real GitHub OAuth provider that calls GitHub's API endpoints.
pub struct RealGitHubProvider {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    /// Where the OAuth endpoints live — `https://github.com` in production.
    oauth_base_url: String,
    /// Where the REST API lives — `https://api.github.com` in production.
    api_base_url: String,
    pending_states: Mutex<HashSet<String>>,
    http_client: reqwest::Client,
}

/// GitHub's own OAuth host. The default for [`RealGitHubProvider::new`].
pub const GITHUB_OAUTH_BASE_URL: &str = "https://github.com";

/// GitHub's own REST host. The default for [`RealGitHubProvider::new`].
pub const GITHUB_API_BASE_URL: &str = "https://api.github.com";

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GitHubApiUser {
    id: u64,
    login: String,
    avatar_url: String,
    name: Option<String>,
}

impl RealGitHubProvider {
    pub fn new(client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self::new_with_base_urls(
            client_id,
            client_secret,
            redirect_uri,
            GITHUB_OAUTH_BASE_URL,
            GITHUB_API_BASE_URL,
        )
    }

    /// The same provider pointed at other hosts.
    ///
    /// This node doubles the crate's network surface — two more endpoints, six more error returns
    /// — and none of it is reachable by a test while the hosts are string literals inside the
    /// request builders. A test serves both halves locally and passes their address here. It is a
    /// constructor rather than a cfg-gated branch precisely because it must be ordinary production
    /// code: GitHub Enterprise is the same substitution.
    pub fn new_with_base_urls(
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
        oauth_base_url: &str,
        api_base_url: &str,
    ) -> Self {
        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
            oauth_base_url: oauth_base_url.trim_end_matches('/').to_string(),
            api_base_url: api_base_url.trim_end_matches('/').to_string(),
            pending_states: Mutex::new(HashSet::new()),
            http_client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl GitHubOAuthProvider for RealGitHubProvider {
    fn authorize_url(&self) -> (String, String) {
        let state = Uuid::new_v4().to_string();
        self.pending_states.lock().unwrap().insert(state.clone());
        // `read:user` identifies the operator; `repo` is what lets the granted token read (and later
        // repoint/merge) pull requests on a private repository — `read:user` alone cannot.
        // Space-separated per OAuth, URL-encoded as `%20`.
        let url = format!(
            "{}/login/oauth/authorize?client_id={}&redirect_uri={}&state={}&scope=read:user%20repo",
            self.oauth_base_url, self.client_id, self.redirect_uri, state
        );
        (url, state)
    }

    async fn exchange_code(&self, code: &str, state: &str) -> Result<(String, GitHubUser), String> {
        let state_valid = self.pending_states.lock().unwrap().remove(state);
        if !state_valid {
            return Err("invalid or expired state parameter".to_string());
        }

        // Exchange code for access token
        let token_resp = self
            .http_client
            .post(format!("{}/login/oauth/access_token", self.oauth_base_url))
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "client_id": self.client_id,
                "client_secret": self.client_secret,
                "code": code,
            }))
            .send()
            .await
            .map_err(|e| format!("token exchange request failed: {}", e))?;

        if !token_resp.status().is_success() {
            return Err(format!(
                "token exchange failed with status: {}",
                token_resp.status()
            ));
        }

        let token_data: AccessTokenResponse = token_resp
            .json()
            .await
            .map_err(|e| format!("failed to parse token response: {}", e))?;

        // Fetch user info
        let user_resp = self
            .http_client
            .get(format!("{}/user", self.api_base_url))
            .header(
                "Authorization",
                format!("Bearer {}", token_data.access_token),
            )
            .header("User-Agent", "tddy-github")
            .send()
            .await
            .map_err(|e| format!("user info request failed: {}", e))?;

        if !user_resp.status().is_success() {
            return Err(format!(
                "user info request failed with status: {}",
                user_resp.status()
            ));
        }

        let api_user: GitHubApiUser = user_resp
            .json()
            .await
            .map_err(|e| format!("failed to parse user response: {}", e))?;

        let user = GitHubUser {
            id: api_user.id,
            login: api_user.login,
            avatar_url: api_user.avatar_url,
            name: api_user.name.unwrap_or_default(),
        };

        Ok((token_data.access_token, user))
    }

    async fn start_device_login(&self) -> Result<DeviceLoginStart, String> {
        // TODO(desktop-login): POST {oauth_base_url}/login/device/code with `client_id` and the
        // `read:user repo` scope, and no client secret.
        todo!("RealGitHubProvider::start_device_login")
    }

    async fn poll_device_login(&self, _device_code: &str) -> Result<DeviceLoginPoll, String> {
        // TODO(desktop-login): POST {oauth_base_url}/login/oauth/access_token with
        // `grant_type=urn:ietf:params:oauth:grant-type:device_code`, then map GitHub's `error`
        // field onto the poll states and fetch the user from {api_base_url}/user on success.
        todo!("RealGitHubProvider::poll_device_login")
    }

    fn issues_usable_access_token(&self) -> bool {
        true
    }
}
