//! Acceptance: signing in is what opens the credential store, and the only thing that does.
//!
//! The store this replaces was a plaintext JSON map the daemon could read whenever it liked. What
//! replaces it is openable only from a key derived at login, so the login path acquires two
//! obligations it did not have:
//!
//! 1. **A store that will not open fails the login**, distinctly. This is the half-login rule of
//!    `GitHubTokenStore`'s doc comment extended one step: a session minted over a vault nobody can
//!    read is a session whose every credential-backed feature reports itself unavailable, and the
//!    one remedy — re-linking the accounts — is the one action the operator has no reason to
//!    attempt unless they are told. `tddy-github`'s
//!    `github_token_retention_acceptance.rs::fails_the_login_when_the_access_token_cannot_be_retained`
//!    pins the write half of the rule; this is the open half.
//! 2. **A stub login opens nothing.** A stub's access token is synthetic
//!    (`GitHubOAuthProvider::issues_usable_access_token` is `false`), so a demo holds no credential
//!    by construction — and must therefore not leave a vault behind, nor derive a key from a token
//!    that is different on every login.

use tddy_credentials::{CredentialStore, VaultError};
use tddy_daemon_auth::auth::build_auth_entries;
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{MultiRpcService, RequestMetadata, RpcBridge, RpcMessage, ServiceEntry, Status};
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
};

const THE_CALLBACK_CODE: &str = "the-code";
const THE_LOGIN: &str = "operator";
const A_CREDENTIAL_THIS_DAEMON_WILL_NEVER_HOLD: &[u8] =
    b"the-login-credential-of-a-former-operator";

#[tokio::test]
async fn a_credential_store_sealed_under_another_key_refuses_the_login() {
    // Given an auth_storage directory holding a vault this login's key does not open — the
    // operator revoked their authorisation and GitHub issued them a different token
    let dir = tempfile::tempdir().expect("a temporary directory");
    let storage = dir.path().join("auth");
    std::fs::create_dir_all(&storage).expect("the storage directory is created");
    CredentialStore::open_or_create(
        &CredentialStore::path_in(&storage),
        A_CREDENTIAL_THIS_DAEMON_WILL_NEVER_HOLD,
        THE_LOGIN,
    )
    .expect("a vault is sealed under the former credential");

    // When they come back from GitHub with their callback code
    let (config, _config_dir) = a_daemon_retaining_credentials_in(&storage);
    let refusal = sign_in(&config).await;

    // Then they are not signed in — a session over a vault nobody can read is a half-login
    assert_eq!(
        refusal
            .map(|_| "signed in")
            .map_err(|status| status.message),
        Err(VaultError::Locked.to_string()),
        "a locked credential store must fail the login, naming the lock"
    );
}

#[tokio::test]
async fn a_locked_credential_store_is_not_replaced_by_an_empty_one() {
    // Given a locked vault, and the bytes it holds
    let dir = tempfile::tempdir().expect("a temporary directory");
    let storage = dir.path().join("auth");
    std::fs::create_dir_all(&storage).expect("the storage directory is created");
    let vault_path = CredentialStore::path_in(&storage);
    CredentialStore::open_or_create(
        &vault_path,
        A_CREDENTIAL_THIS_DAEMON_WILL_NEVER_HOLD,
        THE_LOGIN,
    )
    .expect("a vault is sealed under the former credential");
    let before = std::fs::read(&vault_path).expect("the vault is on disk");

    // When a login that cannot open it is attempted
    let (config, _config_dir) = a_daemon_retaining_credentials_in(&storage);
    let _refused = sign_in(&config).await;

    // Then the vault is exactly as it was — re-initialising it would discard every account the
    // operator has linked, silently, at the moment they are least able to notice
    assert_eq!(
        std::fs::read(&vault_path).ok(),
        Some(before),
        "a failed open must never re-initialise the store"
    );
}

#[tokio::test]
async fn a_stub_login_leaves_no_credential_store_behind() {
    // Given a demo daemon — a stub provider retains nothing, because its access token is synthetic
    let dir = tempfile::tempdir().expect("a temporary directory");
    let storage = dir.path().join("auth");

    // When the demo operator signs in
    let (config, _config_dir) = a_daemon_retaining_credentials_in(&storage);
    let signed_in = sign_in(&config).await;

    // Then the login succeeds and no vault exists: a demo holds no credential by construction, and
    // a key derived from a token that differs on every login would lock the store on the next one
    assert_eq!(
        (
            signed_in
                .map(|response| response.session_token.is_empty())
                .map_err(|status| status.message),
            CredentialStore::path_in(&storage).exists()
        ),
        (Ok(false), false),
        "a stub login must retain nothing and seal nothing"
    );
}

/// Complete a whole sign-in through the served `auth.AuthService`.
async fn sign_in(config: &DaemonConfig) -> Result<ExchangeCodeResponse, Status> {
    let auth = the_auth_service(config);
    let started: GetAuthUrlResponse = call(
        &auth,
        "auth.AuthService",
        "GetAuthUrl",
        GetAuthUrlRequest {},
    )
    .await
    .expect("a configured daemon hands out an authorize url");
    call(
        &auth,
        "auth.AuthService",
        "ExchangeCode",
        ExchangeCodeRequest {
            code: THE_CALLBACK_CODE.to_string(),
            state: started.state,
        },
    )
    .await
}

/// A daemon that signs its own sessions and keeps its operators' credentials under `storage`.
fn a_daemon_retaining_credentials_in(
    storage: &std::path::Path,
) -> (DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let yaml = format!(
        "auth_storage: \"{}\"\nusers:\n  - github_user: \"{THE_LOGIN}\"\n    os_user: \"{THE_LOGIN}-os\"\n\
         github:\n  stub: true\n  stub_codes: \"{THE_CALLBACK_CODE}:{THE_LOGIN}\"\n",
        storage.display()
    );
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    (DaemonConfig::load(&path).expect("the config loads"), dir)
}

fn the_auth_service(config: &DaemonConfig) -> ServiceEntry {
    build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a daemon with github configured builds its auth entries")
        .entries
        .into_iter()
        .find(|entry| entry.name == "auth.AuthService")
        .expect("auth.AuthService is one of the entries")
}

async fn call<Req: prost::Message, Res: prost::Message + Default>(
    entry: &ServiceEntry,
    service: &str,
    method: &str,
    request: Req,
) -> Result<Res, Status> {
    let bridge = RpcBridge::new(MultiRpcService::new(vec![ServiceEntry {
        name: entry.name,
        service: entry.service.clone(),
    }]));
    let message = RpcMessage {
        payload: request.encode_to_vec(),
        metadata: RequestMetadata::default(),
    };
    match bridge.handle_messages(service, method, &[message]).await? {
        tddy_rpc::ResponseBody::Complete(chunks) => {
            Ok(Res::decode(&chunks[0][..]).expect("a unary response decodes"))
        }
        _ => panic!("{method} is unary"),
    }
}
