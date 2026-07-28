//! Acceptance: what a GitHub login leaves behind on the server.
//!
//! The token the operator already granted is the only credential that can read pull requests on a
//! private repo, so `exchange_code` must retain it — it was being discarded, which is why the
//! PR-Stack screen reported "no PR" for a live open PR. Three constraints shape how:
//!
//! - the authorize request must ask for `repo`; `read:user` alone cannot read a private repo's PRs,
//! - a **stub/demo** login must retain nothing — its token is synthetic, and a demo shows no PRs
//!   rather than an error (D12),
//! - the retained token must never reach the client: the session token is handed to a browser over a
//!   plain-http LAN origin, so it stays an identity assertion only.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C3, D7, D12).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::Engine;

use tddy_github::provider::{GitHubOAuthProvider, GitHubUser};
use tddy_github::token_store::GitHubTokenStore;
use tddy_github::{AuthServiceImpl, RealGitHubProvider, SessionTokenSigner, StubGitHubProvider};
use tddy_rpc::Request;
use tddy_service::proto::auth::{AuthService, ExchangeCodeRequest, ExchangeCodeResponse};

const GRANTED_TOKEN: &str = "gho_granted_by_the_operator";

/// An in-memory `GitHubTokenStore` — the daemon's file-backed store without the filesystem.
#[derive(Default)]
struct InMemoryTokenStore {
    tokens: Mutex<HashMap<String, String>>,
}

impl InMemoryTokenStore {
    fn logins(&self) -> Vec<String> {
        let mut logins: Vec<String> = self.tokens.lock().unwrap().keys().cloned().collect();
        logins.sort();
        logins
    }
}

impl GitHubTokenStore for InMemoryTokenStore {
    fn put(&self, login: &str, access_token: &str) -> Result<(), String> {
        self.tokens
            .lock()
            .unwrap()
            .insert(login.to_string(), access_token.to_string());
        Ok(())
    }

    fn get(&self, login: &str) -> Option<String> {
        self.tokens.lock().unwrap().get(login).cloned()
    }
}

/// The server-side detail the daemon's own store names in a failed `put`: the file it could not
/// write. It is logged, never returned to the browser.
const UNWRITABLE_PATH: &str = "/var/lib/tddy/auth/github-tokens.json";

/// A store whose backing medium cannot be written — an unwritable `auth_storage` path. Its error
/// carries the same server-side path detail `FileGitHubTokenStore` reports.
struct AnUnwritableTokenStore;

impl GitHubTokenStore for AnUnwritableTokenStore {
    fn put(&self, _login: &str, _access_token: &str) -> Result<(), String> {
        Err(format!("opening {UNWRITABLE_PATH}: Permission denied"))
    }

    fn get(&self, _login: &str) -> Option<String> {
        None
    }
}

/// A provider that completes the OAuth exchange offline while declaring — as the real GitHub
/// provider does — that its access token is a usable GitHub credential.
struct ProviderWithARealCredential;

#[async_trait]
impl GitHubOAuthProvider for ProviderWithARealCredential {
    fn authorize_url(&self) -> (String, String) {
        (
            "https://github.com/login/oauth/authorize".to_string(),
            "s".to_string(),
        )
    }

    async fn exchange_code(
        &self,
        _code: &str,
        _state: &str,
    ) -> Result<(String, GitHubUser), String> {
        Ok((
            GRANTED_TOKEN.to_string(),
            GitHubUser {
                id: 7,
                login: "operator".to_string(),
                avatar_url: "https://example.com/a.png".to_string(),
                name: "Operator".to_string(),
            },
        ))
    }

    fn issues_usable_access_token(&self) -> bool {
        true
    }
}

/// Everything a signed token is made of, decoded: `v1.<base64url(claims)>.<base64url(tag)>`. An
/// embedded credential would otherwise hide inside the base64.
fn decoded_parts(token: &str) -> String {
    token
        .split('.')
        .map(|part| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(part)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_else(|_| part.to_string())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn exchange(
    service: &AuthServiceImpl<impl GitHubOAuthProvider>,
    code: &str,
    state: &str,
) -> ExchangeCodeResponse {
    service
        .exchange_code(Request::new(ExchangeCodeRequest {
            code: code.to_string(),
            state: state.to_string(),
        }))
        .await
        .expect("the exchange should succeed")
        .into_inner()
}

#[tokio::test]
async fn retains_a_real_logins_access_token_under_its_github_login() {
    // Given — a login through a provider whose token is a usable GitHub credential
    let store = Arc::new(InMemoryTokenStore::default());
    let service = AuthServiceImpl::new_signed(
        ProviderWithARealCredential,
        SessionTokenSigner::new(b"secret"),
    )
    .with_token_store(store.clone());

    // When
    exchange(&service, "login-code", "s").await;

    // Then — the operator's own credential is available for server-side GitHub reads
    assert_eq!(store.get("operator").as_deref(), Some(GRANTED_TOKEN));
}

#[tokio::test]
async fn fails_the_login_when_the_access_token_cannot_be_retained() {
    // Given — a real login whose token store cannot be written
    let service = AuthServiceImpl::new_signed(
        ProviderWithARealCredential,
        SessionTokenSigner::new(b"secret"),
    )
    .with_token_store(Arc::new(AnUnwritableTokenStore));

    // When
    let err = service
        .exchange_code(Request::new(ExchangeCodeRequest {
            code: "login-code".to_string(),
            state: "s".to_string(),
        }))
        .await
        .expect_err("a login that cannot retain its token must not succeed");

    // Then — minting a session without its token would leave the operator apparently signed in while
    // every GitHub-backed read reported itself unavailable, with re-authenticating the one remedy
    // they would have no reason to try
    assert!(
        err.message().contains("operator"),
        "the failure must name the login whose token could not be retained, got: {}",
        err.message()
    );
}

#[tokio::test]
async fn keeps_the_servers_storage_path_out_of_the_failure_the_client_is_shown() {
    // Given — a real login whose token store fails with the path it could not write
    let service = AuthServiceImpl::new_signed(
        ProviderWithARealCredential,
        SessionTokenSigner::new(b"secret"),
    )
    .with_token_store(Arc::new(AnUnwritableTokenStore));

    // When
    let err = service
        .exchange_code(Request::new(ExchangeCodeRequest {
            code: "login-code".to_string(),
            state: "s".to_string(),
        }))
        .await
        .expect_err("a login that cannot retain its token must not succeed");

    // Then — the browser learns *that* retention failed; where the server keeps its tokens is
    // operator-side detail that belongs in the daemon log only
    assert!(
        !err.message().contains(UNWRITABLE_PATH),
        "the server's storage path must not reach the client, got: {}",
        err.message()
    );
}

#[tokio::test]
async fn retains_nothing_for_a_stub_login() {
    // Given — the demo/stub provider, whose access token is synthetic
    let store = Arc::new(InMemoryTokenStore::default());
    let stub = StubGitHubProvider::new("https://github.com", "client-id");
    stub.register_code(
        "demo-code",
        GitHubUser {
            id: 1,
            login: "demo".to_string(),
            avatar_url: String::new(),
            name: "Demo".to_string(),
        },
    );
    let state = stub.authorize_url().1;
    let service = AuthServiceImpl::new_signed(stub, SessionTokenSigner::new(b"secret"))
        .with_token_store(store.clone());

    // When
    exchange(&service, "demo-code", &state).await;

    // Then — a demo login holds no credential at all, so its PR lookups read as "no PRs" rather
    // than as a credential that GitHub would reject
    assert_eq!(store.logins(), Vec::<String>::new());
}

#[tokio::test]
async fn keeps_the_github_token_out_of_everything_the_client_receives() {
    // Given
    let service = AuthServiceImpl::new_signed(
        ProviderWithARealCredential,
        SessionTokenSigner::new(b"secret"),
    )
    .with_token_store(Arc::new(InMemoryTokenStore::default()));

    // When
    let resp = exchange(&service, "login-code", "s").await;

    // Then — the browser is on a plain-http LAN origin; the session and refresh tokens assert an
    // identity and nothing more
    let client_visible = format!(
        "{} {} {:?}",
        decoded_parts(&resp.session_token),
        decoded_parts(&resp.refresh_token),
        resp.user
    );
    assert!(
        !client_visible.contains(GRANTED_TOKEN),
        "the GitHub access token must never be returned to the client, found it in: {client_visible}"
    );
}

#[test]
fn asks_github_for_the_repo_scope_as_well_as_the_users_identity() {
    // Given — the real OAuth provider
    let provider =
        RealGitHubProvider::new("client-id", "client-secret", "http://host/auth/callback");

    // When
    let (authorize_url, _state) = provider.authorize_url();

    // Then — `read:user` alone cannot read pull requests on a private repository
    assert!(
        authorize_url.contains("scope=read:user%20repo"),
        "expected the authorize URL to request `read:user repo`, got: {authorize_url}"
    );
}
