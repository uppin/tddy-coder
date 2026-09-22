//! Acceptance: a daemon restart does not sign anybody out of their credentials.
//!
//! The credential vault opens only from a key a login derives, and the daemon keeps no key of its
//! own — so after a restart it holds nothing that opens anybody's vault. What carries a session's
//! credentials across a restart is the **browser**: each login hands its session lineage an unlock
//! key to a slot of its own in the vault, and the lineage's next session refresh presents it. The
//! daemon reopens the vault through that slot, rotates it, and hands back the replacement.
//!
//! What an operator is entitled to, stated here:
//!
//! - after a restart, the first refresh reopens their vault and PR status reads with their token
//!   again — **with no new login**;
//! - the key rotates at every refresh, so one that has been presented opens nothing afterwards;
//! - signing out removes that lineage's slot;
//! - the key itself is never written into the vault file;
//! - a demo login is handed no key, because it keeps no vault.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_credentials::{CredentialStore, SessionVaults, UnlockKey};
use tddy_daemon_auth::auth::build_auth_entries;
use tddy_daemon_auth::github_pr_credentials::{
    pr_lookup_for_caller, retained_github_token, PrLookup,
};
use tddy_daemon_auth::{DaemonSigningKey, SessionTokens, StandaloneKeyDirectory};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_github::provider::{DeviceLoginPoll, DeviceLoginStart, GitHubOAuthProvider, GitHubUser};
use tddy_github::{AuthServiceImpl, SessionTokenAuthority};
use tddy_rpc::{MultiRpcService, Request, RequestMetadata, RpcBridge, RpcMessage, ServiceEntry};
use tddy_service::proto::auth::{
    AuthService, ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    LogoutRequest, RefreshSessionRequest, RefreshSessionResponse,
};

const THE_LOGIN: &str = "operator";
const THE_GRANTED_TOKEN: &str = "gho_granted_by_the_operator";

#[tokio::test]
async fn after_a_restart_a_refresh_reopens_the_vault_and_pr_status_reads_with_the_stored_token() {
    // Given an operator who signed in, and a daemon that has since restarted
    let daemon = a_daemon();
    let signed_in = daemon.running().sign_in().await;
    let after_restart = daemon.running();

    // When their browser refreshes its session, presenting the key it was handed
    after_restart
        .refresh(&signed_in.refresh_token, &signed_in.vault_unlock_key)
        .await;

    // Then PR status reads with the token they granted — and nobody signed in again
    assert_eq!(
        after_restart.pr_lookup_for(THE_LOGIN),
        PrLookup::Perform(THE_GRANTED_TOKEN.to_string())
    );
}

#[tokio::test]
async fn before_that_refresh_pr_status_says_the_credential_unlocks_at_the_next_refresh() {
    // Given an operator who signed in, and a daemon that has since restarted
    let daemon = a_daemon();
    daemon.running().sign_in().await;
    let after_restart = daemon.running();

    // When their still-valid access token asks for PR status before any refresh
    let lookup = after_restart.pr_lookup_for(THE_LOGIN);

    // Then it is unavailable, and says the remedy is the refresh — not a new login
    assert!(
        matches!(&lookup, PrLookup::Unavailable(reason) if reason.contains("next session refresh")),
        "expected an unavailable lookup naming the next session refresh, got {lookup:?}"
    );
}

#[tokio::test]
async fn a_refresh_rotates_the_unlock_key_so_the_presented_one_opens_nothing() {
    // Given a lineage that has refreshed once since signing in
    let daemon = a_daemon();
    let signed_in = daemon.running().sign_in().await;
    let refreshed = daemon
        .running()
        .refresh(&signed_in.refresh_token, &signed_in.vault_unlock_key)
        .await;

    // When each key is tried against the vault
    let opens = |wire: &str| daemon.unlock_key_opens_the_vault(wire);

    // Then only the rotated one opens it
    assert_eq!(
        (
            opens(&signed_in.vault_unlock_key),
            opens(&refreshed.vault_unlock_key)
        ),
        (false, true)
    );
}

#[tokio::test]
async fn signing_out_removes_the_lineages_unlock_slot() {
    // Given a signed-in lineage
    let daemon = a_daemon();
    let running = daemon.running();
    let signed_in = running.sign_in().await;

    // When it signs out, handing back its key
    running
        .service
        .logout(Request::direct(LogoutRequest {
            session_token: signed_in.session_token.clone(),
            vault_unlock_key: signed_in.vault_unlock_key.clone(),
        }))
        .await
        .expect("a logout is answered");

    // Then that key opens nothing — the slot is gone, so a stolen copy is worthless too
    assert!(!daemon.unlock_key_opens_the_vault(&signed_in.vault_unlock_key));
}

#[tokio::test]
async fn the_vault_file_never_holds_the_unlock_key_it_handed_out() {
    // Given a signed-in lineage, and the key it holds
    let daemon = a_daemon();
    let signed_in = daemon.running().sign_in().await;
    let key_material = signed_in
        .vault_unlock_key
        .rsplit('.')
        .next()
        .expect("an unlock key carries its key material last")
        .to_string();

    // When the vault is read off the daemon's disk
    let on_disk = std::fs::read_to_string(CredentialStore::path_in(daemon.storage(), THE_LOGIN))
        .expect("the login sealed a vault");

    // Then the key is not in it — the file holds the wrap, the browser holds the key
    assert!(
        !on_disk.contains(&key_material),
        "the vault file must hold only the wrapped slot, never the key that opens it"
    );
}

#[tokio::test]
async fn a_stub_login_is_handed_no_unlock_key() {
    // Given a demo daemon — its logins keep no vault
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (config, _config_dir) = a_demo_daemon_retaining_credentials_in(&dir.path().join("auth"));

    // When the demo operator signs in
    let signed_in = sign_in_to_the_demo(&config).await;

    // Then there is no key, because there is no vault for it to open
    assert_eq!(signed_in.vault_unlock_key, "");
}

/// One `auth_storage` and one signing identity, across as many daemon runs as a test needs.
struct ADaemon {
    storage: tempfile::TempDir,
    home: tempfile::TempDir,
}

fn a_daemon() -> ADaemon {
    ADaemon {
        storage: tempfile::tempdir().expect("a temporary directory"),
        home: tempfile::tempdir().expect("a temporary directory"),
    }
}

impl ADaemon {
    fn storage(&self) -> &std::path::Path {
        self.storage.path()
    }

    /// A fresh daemon process over the same disk: its own, empty set of open vaults.
    fn running(&self) -> ARunningDaemon {
        let key = DaemonSigningKey::load_or_generate(&self.home.path().join("signing.key"))
            .expect("the daemon's signing key loads");
        let tokens = SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory));
        let vaults = Arc::new(SessionVaults::new(self.storage()));
        let service = AuthServiceImpl::new_signed(
            ProviderWithARealCredential,
            tokens.signer().clone(),
            Arc::clone(tokens.verifier()) as Arc<dyn SessionTokenAuthority>,
        )
        .with_credential_vaults(Arc::clone(&vaults));
        ARunningDaemon { service, vaults }
    }

    fn unlock_key_opens_the_vault(&self, wire: &str) -> bool {
        let unlock = UnlockKey::from_wire(wire).expect("the daemon hands out well-formed keys");
        CredentialStore::open_with_unlock_key(
            &CredentialStore::path_in(self.storage(), unlock.subject()),
            &unlock,
        )
        .is_ok()
    }
}

struct ARunningDaemon {
    service: AuthServiceImpl<ProviderWithARealCredential>,
    vaults: Arc<SessionVaults>,
}

impl ARunningDaemon {
    async fn sign_in(&self) -> ExchangeCodeResponse {
        self.service
            .exchange_code(Request::direct(ExchangeCodeRequest {
                code: "the-code".to_string(),
                state: "the-state".to_string(),
            }))
            .await
            .expect("the login succeeds")
            .into_inner()
    }

    async fn refresh(&self, refresh_token: &str, vault_unlock_key: &str) -> RefreshSessionResponse {
        self.service
            .refresh_session(Request::direct(RefreshSessionRequest {
                refresh_token: refresh_token.to_string(),
                vault_unlock_key: vault_unlock_key.to_string(),
            }))
            .await
            .expect("a valid refresh token extends the session")
            .into_inner()
    }

    /// How a PR-status read resolves `login`'s credential on this daemon — the same two steps
    /// `DaemonSessionHost::pr_status_for_caller` takes, on a daemon that is not in stub mode.
    fn pr_lookup_for(&self, login: &str) -> PrLookup {
        match retained_github_token(Some(&self.vaults), login) {
            Ok(stored) => pr_lookup_for_caller(false, stored.as_deref()),
            Err(reason) => PrLookup::Unavailable(reason),
        }
    }
}

/// A provider that completes the OAuth exchange offline while declaring — as the real GitHub
/// provider does — that its access token is a usable GitHub credential.
struct ProviderWithARealCredential;

#[async_trait]
impl GitHubOAuthProvider for ProviderWithARealCredential {
    fn authorize_url(&self) -> Result<(String, String), String> {
        Ok((
            "https://github.com/login/oauth/authorize".to_string(),
            "the-state".to_string(),
        ))
    }

    async fn exchange_code(
        &self,
        _code: &str,
        _state: &str,
    ) -> Result<(String, GitHubUser), String> {
        Ok((
            THE_GRANTED_TOKEN.to_string(),
            GitHubUser {
                id: 7,
                login: THE_LOGIN.to_string(),
                avatar_url: String::new(),
                name: "The Operator".to_string(),
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

/// A demo daemon, wired exactly as the daemon wires itself, keeping its credentials under
/// `storage`.
fn a_demo_daemon_retaining_credentials_in(
    storage: &std::path::Path,
) -> (DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let yaml = format!(
        "auth_storage: \"{}\"\nusers:\n  - github_user: \"{THE_LOGIN}\"\n    os_user: \"{THE_LOGIN}-os\"\n\
         github:\n  stub: true\n  stub_codes: \"the-code:{THE_LOGIN}\"\n",
        storage.display()
    );
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).expect("the config is written");
    (DaemonConfig::load(&path).expect("the config loads"), dir)
}

/// Complete a whole demo sign-in through the served `auth.AuthService`.
async fn sign_in_to_the_demo(config: &DaemonConfig) -> ExchangeCodeResponse {
    let auth = build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a daemon with github configured builds its auth entries")
        .entries
        .into_iter()
        .find(|entry| entry.name == "auth.AuthService")
        .expect("auth.AuthService is one of the entries");
    let started: GetAuthUrlResponse = call(&auth, "GetAuthUrl", GetAuthUrlRequest {}).await;
    call(
        &auth,
        "ExchangeCode",
        ExchangeCodeRequest {
            code: "the-code".to_string(),
            state: started.state,
        },
    )
    .await
}

async fn call<Req: prost::Message, Res: prost::Message + Default>(
    entry: &ServiceEntry,
    method: &str,
    request: Req,
) -> Res {
    let bridge = RpcBridge::new(MultiRpcService::new(vec![ServiceEntry {
        name: entry.name,
        service: entry.service.clone(),
    }]));
    let message = RpcMessage {
        payload: request.encode_to_vec(),
        metadata: RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    };
    match bridge
        .handle_messages("auth.AuthService", method, &[message])
        .await
        .unwrap_or_else(|status| panic!("{method} is answered, got {status:?}"))
    {
        tddy_rpc::ResponseBody::Complete(chunks) => {
            Res::decode(&chunks[0][..]).expect("a unary response decodes")
        }
        _ => panic!("{method} is unary"),
    }
}
