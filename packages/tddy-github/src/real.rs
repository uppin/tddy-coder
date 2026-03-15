use std::collections::HashSet;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::Deserialize;
use uuid::Uuid;

use crate::provider::{GitHubOAuthProvider, GitHubUser};

/// Real GitHub OAuth provider that calls GitHub's API endpoints.
pub struct RealGitHubProvider {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    pending_states: Mutex<HashSet<String>>,
    http_client: reqwest::Client,
}

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
        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: redirect_uri.to_string(),
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
        let url = format!(
            "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&state={}&scope=read:user",
            self.client_id, self.redirect_uri, state
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
            .post("https://github.com/login/oauth/access_token")
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
            .get("https://api.github.com/user")
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
}
