use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use uuid::Uuid;

use crate::provider::{GitHubOAuthProvider, GitHubUser};

/// In-memory stub that mimics GitHub OAuth without HTTP calls.
/// Pre-register code→user mappings via `register_code` before tests.
pub struct StubGitHubProvider {
    pending_states: Mutex<HashSet<String>>,
    code_to_user: Mutex<HashMap<String, GitHubUser>>,
    authorize_base_url: String,
    client_id: String,
    /// When set, authorize_url returns a direct callback URL with the first registered code.
    /// This allows e2e tests to skip the GitHub redirect entirely.
    callback_redirect_url: Option<String>,
}

impl StubGitHubProvider {
    pub fn new(authorize_base_url: &str, client_id: &str) -> Self {
        Self {
            pending_states: Mutex::new(HashSet::new()),
            code_to_user: Mutex::new(HashMap::new()),
            authorize_base_url: authorize_base_url.trim_end_matches('/').to_string(),
            client_id: client_id.to_string(),
            callback_redirect_url: None,
        }
    }

    /// Create a stub configured for e2e testing: authorize_url returns a direct
    /// callback URL instead of a GitHub URL.
    pub fn new_with_callback(callback_redirect_url: &str, client_id: &str) -> Self {
        Self {
            pending_states: Mutex::new(HashSet::new()),
            code_to_user: Mutex::new(HashMap::new()),
            authorize_base_url: "https://github.com".to_string(),
            client_id: client_id.to_string(),
            callback_redirect_url: Some(callback_redirect_url.to_string()),
        }
    }

    /// Register a test code→user mapping. When `exchange_code` is called with
    /// this code, it returns this user.
    pub fn register_code(&self, code: &str, user: GitHubUser) {
        self.code_to_user
            .lock()
            .unwrap()
            .insert(code.to_string(), user);
    }
}

#[async_trait]
impl GitHubOAuthProvider for StubGitHubProvider {
    fn authorize_url(&self) -> (String, String) {
        let state = Uuid::new_v4().to_string();
        self.pending_states.lock().unwrap().insert(state.clone());

        let url = if let Some(ref callback_url) = self.callback_redirect_url {
            // In e2e mode: return a URL that goes directly to the app's callback
            // with the first registered code
            let code = self
                .code_to_user
                .lock()
                .unwrap()
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "test-code".to_string());
            format!("{}?code={}&state={}", callback_url, code, state)
        } else {
            format!(
                "{}/login/oauth/authorize?client_id={}&state={}",
                self.authorize_base_url, self.client_id, state
            )
        };
        (url, state)
    }

    async fn exchange_code(&self, code: &str, state: &str) -> Result<(String, GitHubUser), String> {
        let state_valid = self.pending_states.lock().unwrap().remove(state);
        if !state_valid {
            return Err("invalid or expired state parameter".to_string());
        }
        let user = self
            .code_to_user
            .lock()
            .unwrap()
            .get(code)
            .cloned()
            .ok_or_else(|| format!("unknown authorization code: {}", code))?;
        let access_token = format!("stub-access-token-{}", Uuid::new_v4());
        Ok((access_token, user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user() -> GitHubUser {
        GitHubUser {
            id: 42,
            login: "testuser".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
            name: "Test User".to_string(),
        }
    }

    #[test]
    fn authorize_url_returns_valid_url_and_state() {
        let stub = StubGitHubProvider::new("https://github.com", "my-client-id");
        let (url, state) = stub.authorize_url();
        assert!(url.contains("https://github.com/login/oauth/authorize"));
        assert!(url.contains("client_id=my-client-id"));
        assert!(url.contains(&format!("state={}", state)));
        assert!(!state.is_empty());
    }

    #[test]
    fn authorize_url_generates_unique_states() {
        let stub = StubGitHubProvider::new("https://github.com", "id");
        let (_, s1) = stub.authorize_url();
        let (_, s2) = stub.authorize_url();
        assert_ne!(s1, s2);
    }

    #[tokio::test]
    async fn exchange_code_with_registered_code_returns_user() {
        let stub = StubGitHubProvider::new("https://github.com", "id");
        stub.register_code("test-code", test_user());
        let (_, state) = stub.authorize_url();

        let result = stub.exchange_code("test-code", &state).await;
        assert!(result.is_ok());
        let (token, user) = result.unwrap();
        assert!(token.starts_with("stub-access-token-"));
        assert_eq!(user.login, "testuser");
        assert_eq!(user.id, 42);
    }

    #[tokio::test]
    async fn exchange_code_with_unknown_code_returns_error() {
        let stub = StubGitHubProvider::new("https://github.com", "id");
        let (_, state) = stub.authorize_url();

        let result = stub.exchange_code("unknown-code", &state).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown authorization code"));
    }

    #[tokio::test]
    async fn exchange_code_with_invalid_state_returns_error() {
        let stub = StubGitHubProvider::new("https://github.com", "id");
        stub.register_code("test-code", test_user());

        let result = stub.exchange_code("test-code", "bogus-state").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid or expired state"));
    }

    #[tokio::test]
    async fn state_is_single_use() {
        let stub = StubGitHubProvider::new("https://github.com", "id");
        stub.register_code("test-code", test_user());
        let (_, state) = stub.authorize_url();

        let first = stub.exchange_code("test-code", &state).await;
        assert!(first.is_ok());

        // Re-register code since user was consumed
        stub.register_code("test-code", test_user());
        let second = stub.exchange_code("test-code", &state).await;
        assert!(second.is_err(), "state should be consumed after first use");
    }
}
