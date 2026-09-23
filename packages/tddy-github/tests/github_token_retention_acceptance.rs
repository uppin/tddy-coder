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
//! The token is retained in the operator's own credential vault (`tddy-credentials`), sealed under
//! a key derived from the operator's passphrase. A first login has no vault yet, so its token waits
//! in memory until a passphrase creates one — these tests read it back the way the operator would,
//! by choosing that passphrase and then reading the vault.
//!
//! PRD: docs/ft/coder/pr-stack-live-status.md (C3, D7, D12);
//! docs/ft/daemon/1-WIP/PRD-2026-09-19-keyring-store.md.

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;

use tddy_credentials::{AccountId, ProviderId, SecretString, SessionVaults};
use tddy_github::provider::{DeviceLoginPoll, DeviceLoginStart, GitHubOAuthProvider, GitHubUser};
use tddy_github::{
    AuthServiceImpl, RealGitHubProvider, SessionClaims, SessionTokenAuthority, SessionTokenError,
    SessionTokenSigner, StubGitHubProvider, GITHUB_PROVIDER,
};
use tddy_rpc::Request;
use tddy_service::proto::auth::{AuthService, ExchangeCodeRequest, ExchangeCodeResponse};

const GRANTED_TOKEN: &str = "gho_granted_by_the_operator";

const THE_PASSPHRASE: &str = "correct horse battery staple";

/// The operator's GitHub token as they would find it: their vault, created with a first
/// passphrase (which seals the token their login left waiting), then read.
fn the_token_retained_for(vaults: &SessionVaults, login: &str) -> Option<String> {
    vaults
        .create(login, &SecretString::new(THE_PASSPHRASE))
        .expect("a first passphrase creates the vault");
    vaults
        .get(login)?
        .get(&ProviderId::new(GITHUB_PROVIDER), &AccountId::new(login))
        .expect("the retained record authenticates")
        .map(|record| record.secret.expose().to_string())
}

/// A service whose operator's vault is open, and whose vault file has then been made impossible
/// to read or replace — a directory where the file was, standing in for a disk that fails under a
/// running daemon.
async fn a_service_whose_open_vault_can_no_longer_be_written() -> (
    tempfile::TempDir,
    AuthServiceImpl<ProviderWithARealCredential>,
) {
    let storage = tempfile::tempdir().expect("a temporary directory");
    let vaults = Arc::new(SessionVaults::new(storage.path()));
    let service =
        a_signed_service(ProviderWithARealCredential).with_credential_vaults(Arc::clone(&vaults));
    exchange(&service, "login-code", "s").await;
    vaults
        .create("operator", &SecretString::new(THE_PASSPHRASE))
        .expect("a first passphrase creates the vault");
    let vault_file = vaults.path_for("operator");
    std::fs::remove_file(&vault_file).expect("the vault file is removed");
    std::fs::create_dir(&vault_file).expect("a directory takes its place");
    (storage, service)
}

/// A provider that completes the OAuth exchange offline while declaring — as the real GitHub
/// provider does — that its access token is a usable GitHub credential.
struct ProviderWithARealCredential;

#[async_trait]
impl GitHubOAuthProvider for ProviderWithARealCredential {
    fn authorize_url(&self) -> Result<(String, String), String> {
        Ok((
            "https://github.com/login/oauth/authorize".to_string(),
            "s".to_string(),
        ))
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

    async fn start_device_login(&self) -> Result<DeviceLoginStart, String> {
        unimplemented!("this fake authenticates by code exchange, never by device code")
    }

    async fn poll_device_login(&self, _device_code: &str) -> Result<DeviceLoginPoll, String> {
        unimplemented!("this fake authenticates by code exchange, never by device code")
    }

    fn issues_usable_access_token(&self) -> bool {
        true
    }
}

/// A service that signs its tokens with a daemon key of its own.
///
/// These tests exercise the exchange, which only *mints*, so the authority it would verify
/// presented tokens through admits none — nothing here presents one.
fn a_signed_service<P: GitHubOAuthProvider>(provider: P) -> AuthServiceImpl<P> {
    let key = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
    AuthServiceImpl::new_signed(
        provider,
        SessionTokenSigner::new(key),
        Arc::new(AdmitsNoToken),
    )
}

struct AdmitsNoToken;

#[async_trait]
impl SessionTokenAuthority for AdmitsNoToken {
    async fn verify(&self, _token: &str) -> Result<SessionClaims, SessionTokenError> {
        Err(SessionTokenError::InvalidSignature)
    }
}

/// Everything a signed token is made of, decoded: `v2.<base64url(claims)>.<base64url(signature)>`.
/// An embedded credential would otherwise hide inside the base64.
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
        .exchange_code(Request::direct(ExchangeCodeRequest {
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
    let storage = tempfile::tempdir().expect("a temporary directory");
    let vaults = Arc::new(SessionVaults::new(storage.path()));
    let service =
        a_signed_service(ProviderWithARealCredential).with_credential_vaults(Arc::clone(&vaults));

    // When
    exchange(&service, "login-code", "s").await;

    // Then — the operator's own credential is available for server-side GitHub reads
    assert_eq!(
        the_token_retained_for(&vaults, "operator").as_deref(),
        Some(GRANTED_TOKEN)
    );
}

#[tokio::test]
async fn fails_the_login_when_the_access_token_cannot_be_retained() {
    // Given — a real login over an open vault whose file cannot be written
    let (_storage, service) = a_service_whose_open_vault_can_no_longer_be_written().await;

    // When
    let err = service
        .exchange_code(Request::direct(ExchangeCodeRequest {
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
    // Given — a real login whose credential vault fails with the path it could not write
    let (storage, service) = a_service_whose_open_vault_can_no_longer_be_written().await;

    // When
    let err = service
        .exchange_code(Request::direct(ExchangeCodeRequest {
            code: "login-code".to_string(),
            state: "s".to_string(),
        }))
        .await
        .expect_err("a login that cannot retain its token must not succeed");

    // Then — the browser learns *that* retention failed; where the server keeps its tokens is
    // operator-side detail that belongs in the daemon log only
    assert!(
        !err.message()
            .contains(&storage.path().display().to_string()),
        "the server's storage path must not reach the client, got: {}",
        err.message()
    );
}

#[tokio::test]
async fn retains_nothing_for_a_stub_login() {
    // Given — the demo/stub provider, whose access token is synthetic
    let storage = tempfile::tempdir().expect("a temporary directory");
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
    let (_, state) = stub
        .authorize_url()
        .expect("the stub issues an authorize URL");
    let service =
        a_signed_service(stub).with_credential_vaults(Arc::new(SessionVaults::new(storage.path())));

    // When
    let resp = exchange(&service, "demo-code", &state).await;

    // Then — a demo login holds no credential at all, so its PR lookups read as "no PRs" rather
    // than as a credential that GitHub would reject: no vault, and no key to one
    let left_behind: Vec<_> = std::fs::read_dir(storage.path())
        .expect("the storage directory is readable")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(
        (left_behind, resp.vault_unlock_key),
        (Vec::new(), String::new())
    );
}

#[tokio::test]
async fn keeps_the_github_token_out_of_everything_the_client_receives() {
    // Given
    let storage = tempfile::tempdir().expect("a temporary directory");
    let service = a_signed_service(ProviderWithARealCredential)
        .with_credential_vaults(Arc::new(SessionVaults::new(storage.path())));

    // When
    let resp = exchange(&service, "login-code", "s").await;

    // Then — the browser is on a plain-http LAN origin; the session and refresh tokens assert an
    // identity and nothing more, and the vault unlock key is a wrap key rather than the credential
    let client_visible = format!(
        "{} {} {:?} {}",
        decoded_parts(&resp.session_token),
        decoded_parts(&resp.refresh_token),
        resp.user,
        resp.vault_unlock_key
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
    let (authorize_url, _state) = provider
        .authorize_url()
        .expect("a confidential client issues an authorize URL");

    // Then — `read:user` alone cannot read pull requests on a private repository
    assert!(
        authorize_url.contains("scope=read:user%20repo"),
        "expected the authorize URL to request `read:user repo`, got: {authorize_url}"
    );
}
