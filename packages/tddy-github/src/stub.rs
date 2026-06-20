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
        // Given a stub provider configured with a known client_id
        let stub = StubGitHubProvider::new("https://github.com", "my-client-id");

        // When generating an authorize URL
        let (url, state) = stub.authorize_url();

        // Then the URL contains the expected OAuth parameters and the state is non-empty
        assert!(url.contains("https://github.com/login/oauth/authorize"));
        assert!(url.contains("client_id=my-client-id"));
        assert!(url.contains(&format!("state={}", state)));
        assert!(!state.is_empty());
    }

    #[test]
    fn authorize_url_generates_unique_states() {
        // Given a stub provider
        let stub = StubGitHubProvider::new("https://github.com", "id");

        // When generating two authorize URLs
        let (_, s1) = stub.authorize_url();
        let (_, s2) = stub.authorize_url();

        // Then each has a distinct state token (CSRF protection)
        assert_ne!(s1, s2);
    }

    #[tokio::test]
    async fn exchange_code_with_registered_code_returns_user() {
        // Given a stub with a pre-registered code→user mapping
        let stub = StubGitHubProvider::new("https://github.com", "id");
        stub.register_code("test-code", test_user());
        let (_, state) = stub.authorize_url();

        // When exchanging the registered code with the valid state
        let result = stub.exchange_code("test-code", &state).await;

        // Then the exchange succeeds and returns the expected user and a stub token
        assert!(result.is_ok());
        let (token, user) = result.unwrap();
        assert!(token.starts_with("stub-access-token-"));
        assert_eq!(user.login, "testuser");
        assert_eq!(user.id, 42);
    }

    #[tokio::test]
    async fn exchange_code_with_unknown_code_returns_error() {
        // Given a stub with no registered codes
        let stub = StubGitHubProvider::new("https://github.com", "id");
        let (_, state) = stub.authorize_url();

        // When exchanging an unregistered code
        let result = stub.exchange_code("unknown-code", &state).await;

        // Then the exchange fails with a descriptive error
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown authorization code"));
    }

    #[tokio::test]
    async fn exchange_code_with_invalid_state_returns_error() {
        // Given a stub with a registered code
        let stub = StubGitHubProvider::new("https://github.com", "id");
        stub.register_code("test-code", test_user());

        // When exchanging with a bogus (unregistered) state
        let result = stub.exchange_code("test-code", "bogus-state").await;

        // Then the exchange is rejected
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid or expired state"));
    }

    #[tokio::test]
    async fn state_is_single_use() {
        // Given a stub and a valid authorize flow
        let stub = StubGitHubProvider::new("https://github.com", "id");
        stub.register_code("test-code", test_user());
        let (_, state) = stub.authorize_url();

        // When exchanging once — succeeds
        let first = stub.exchange_code("test-code", &state).await;
        assert!(first.is_ok());

        // Then re-using the same state fails (state is consumed after first use)
        stub.register_code("test-code", test_user());
        let second = stub.exchange_code("test-code", &state).await;
        assert!(second.is_err(), "state should be consumed after first use");
    }
}
