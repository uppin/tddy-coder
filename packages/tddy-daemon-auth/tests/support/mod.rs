//! One daemon's disk and signing identity across as many runs as a test needs, and a GitHub that
//! behaves like GitHub: **every exchange mints a new access token**.
//!
//! That last property is the one the credential vault's first design missed. A GitHub OAuth App
//! issues a fresh token on each code or device exchange, so nothing derived from the token is
//! stable across logins; a fake that returned one constant token encoded the false premise and
//! could not catch it.

#![allow(dead_code)] // each test binary uses its own part of the harness

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tddy_credentials::{CredentialStore, SessionVaults, UnlockKey};
use tddy_daemon_auth::github_pr_credentials::{
    pr_lookup_for_caller, retained_github_token, PrLookup,
};
use tddy_daemon_auth::{DaemonSigningKey, SessionTokens, StandaloneKeyDirectory};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_github::provider::{DeviceLoginPoll, DeviceLoginStart, GitHubOAuthProvider, GitHubUser};
use tddy_github::{AuthServiceImpl, SessionTokenAuthority};
use tddy_rpc::{
    MultiRpcService, Request, RequestMetadata, RpcBridge, RpcMessage, ServiceEntry, Status,
};
use tddy_service::proto::auth::{
    AuthService, ExchangeCodeRequest, ExchangeCodeResponse, GetAuthStatusRequest,
    GetAuthStatusResponse, LogoutRequest, PollDeviceLoginRequest, PollDeviceLoginResponse,
    RefreshSessionRequest, RefreshSessionResponse, ResetVaultRequest, ResetVaultResponse,
    UnlockVaultRequest, UnlockVaultResponse, VaultState,
};

pub const THE_LOGIN: &str = "operator";
pub const ANOTHER_LOGIN: &str = "somebody-else";
pub const THE_PASSPHRASE: &str = "correct horse battery staple";
pub const A_WRONG_PASSPHRASE: &str = "incorrect horse battery staple";
pub const A_NEW_PASSPHRASE: &str = "a passphrase chosen after forgetting";

/// The access token GitHub grants at the `n`th exchange this daemon's disk has seen, from 1.
pub fn the_token_granted_at_exchange(n: usize) -> String {
    format!("gho_granted_at_exchange_{n}")
}

/// One `auth_storage`, one signing identity and one GitHub, across as many daemon runs as a test
/// needs.
pub struct ADaemon {
    storage: tempfile::TempDir,
    home: tempfile::TempDir,
    exchanges: Arc<AtomicUsize>,
}

pub fn a_daemon() -> ADaemon {
    ADaemon {
        storage: tempfile::tempdir().expect("a temporary directory"),
        home: tempfile::tempdir().expect("a temporary directory"),
        exchanges: Arc::new(AtomicUsize::new(0)),
    }
}

impl ADaemon {
    pub fn storage(&self) -> &std::path::Path {
        self.storage.path()
    }

    /// A fresh daemon process over the same disk: its own, empty set of open vaults.
    pub fn running(&self) -> ARunningDaemon {
        self.running_with(SessionVaults::new(self.storage()))
    }

    /// A fresh daemon process whose pending sign-ins wait `lifetime`, reading the time from
    /// `clock` — so a test moves time by hand rather than sleeping.
    pub fn running_on(
        &self,
        clock: tddy_credentials::Clock,
        lifetime: Option<std::time::Duration>,
    ) -> ARunningDaemon {
        self.running_with(
            SessionVaults::new(self.storage())
                .with_clock(clock)
                .with_pending_lifetime(lifetime),
        )
    }

    fn running_with(&self, vaults: SessionVaults) -> ARunningDaemon {
        let key = DaemonSigningKey::load_or_generate(&self.home.path().join("signing.key"))
            .expect("the daemon's signing key loads");
        let tokens = SessionTokens::new(&key, Arc::new(StandaloneKeyDirectory));
        let vaults = Arc::new(vaults);
        let service = AuthServiceImpl::new_signed(
            GitHubMintingATokenPerExchange {
                exchanges: Arc::clone(&self.exchanges),
            },
            tokens.signer().clone(),
            Arc::clone(tokens.verifier()) as Arc<dyn SessionTokenAuthority>,
        )
        .with_credential_vaults(Arc::clone(&vaults));
        ARunningDaemon { service, vaults }
    }

    /// The vault file's bytes, or `None` when there is none.
    pub fn vault_on_disk(&self, login: &str) -> Option<Vec<u8>> {
        std::fs::read(CredentialStore::path_in(self.storage(), login)).ok()
    }

    /// Every file in `auth_storage`, by name.
    pub fn files_in_storage(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.storage())
            .expect("the storage directory is readable")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    pub fn unlock_key_opens_the_vault(&self, wire: &str) -> bool {
        let unlock = UnlockKey::from_wire(wire).expect("the daemon hands out well-formed keys");
        CredentialStore::open_with_unlock_key(
            &CredentialStore::path_in(self.storage(), unlock.subject()),
            &unlock,
        )
        .is_ok()
    }
}

pub struct ARunningDaemon {
    pub service: AuthServiceImpl<GitHubMintingATokenPerExchange>,
    pub vaults: Arc<SessionVaults>,
}

impl ARunningDaemon {
    /// A redirect-flow sign-in as [`THE_LOGIN`].
    pub async fn sign_in(&self) -> ExchangeCodeResponse {
        self.sign_in_as(THE_LOGIN).await
    }

    /// A redirect-flow sign-in as `login` (the fake reads the login from the code).
    pub async fn sign_in_as(&self, login: &str) -> ExchangeCodeResponse {
        self.service
            .exchange_code(Request::direct(ExchangeCodeRequest {
                code: login.to_string(),
                state: "the-state".to_string(),
            }))
            .await
            .expect("the login succeeds")
            .into_inner()
    }

    /// A device-flow sign-in as [`THE_LOGIN`], approved at the first poll.
    pub async fn sign_in_by_device(&self) -> PollDeviceLoginResponse {
        self.service
            .poll_device_login(Request::direct(PollDeviceLoginRequest {
                device_code: THE_LOGIN.to_string(),
            }))
            .await
            .expect("the device login completes")
            .into_inner()
    }

    /// A signed-in operator who created their vault under [`THE_PASSPHRASE`], and the unlock key
    /// that creation handed their lineage.
    pub async fn sign_in_and_create_the_vault(
        &self,
    ) -> (ExchangeCodeResponse, UnlockVaultResponse) {
        let signed_in = self.sign_in().await;
        let created = self
            .unlock_vault(&signed_in.session_token, THE_PASSPHRASE, true)
            .await
            .expect("a first passphrase creates the vault");
        (signed_in, created)
    }

    pub async fn refresh(
        &self,
        refresh_token: &str,
        vault_unlock_key: &str,
    ) -> RefreshSessionResponse {
        self.service
            .refresh_session(Request::direct(RefreshSessionRequest {
                refresh_token: refresh_token.to_string(),
                vault_unlock_key: vault_unlock_key.to_string(),
            }))
            .await
            .expect("a valid refresh token extends the session")
            .into_inner()
    }

    pub async fn unlock_vault(
        &self,
        session_token: &str,
        passphrase: &str,
        create: bool,
    ) -> Result<UnlockVaultResponse, Status> {
        self.service
            .unlock_vault(Request::direct(UnlockVaultRequest {
                session_token: session_token.to_string(),
                passphrase: passphrase.to_string(),
                create,
            }))
            .await
            .map(tddy_rpc::Response::into_inner)
    }

    pub async fn reset_vault(
        &self,
        session_token: &str,
        new_passphrase: &str,
    ) -> Result<ResetVaultResponse, Status> {
        self.service
            .reset_vault(Request::direct(ResetVaultRequest {
                session_token: session_token.to_string(),
                new_passphrase: new_passphrase.to_string(),
            }))
            .await
            .map(tddy_rpc::Response::into_inner)
    }

    pub async fn status(&self, session_token: &str) -> GetAuthStatusResponse {
        self.service
            .get_auth_status(Request::direct(GetAuthStatusRequest {
                session_token: session_token.to_string(),
            }))
            .await
            .expect("a status check is answered")
            .into_inner()
    }

    pub async fn logout(&self, session_token: &str, vault_unlock_key: &str) {
        self.service
            .logout(Request::direct(LogoutRequest {
                session_token: session_token.to_string(),
                vault_unlock_key: vault_unlock_key.to_string(),
            }))
            .await
            .expect("a logout is answered");
    }

    /// How a PR-status read resolves `login`'s credential on this daemon — the same two steps
    /// `DaemonSessionHost::pr_status_for_caller` takes, on a daemon that is not in stub mode.
    pub fn pr_lookup_for(&self, login: &str) -> PrLookup {
        match retained_github_token(Some(&self.vaults), login) {
            Ok(stored) => pr_lookup_for_caller(false, stored.as_deref()),
            Err(reason) => PrLookup::Unavailable(reason),
        }
    }
}

/// The `vault_state` a response carries, as the enum it encodes.
pub fn state(raw: i32) -> VaultState {
    VaultState::try_from(raw).expect("a vault state this build knows")
}

/// A GitHub that completes both flows offline, for the login named by the code or device code, and
/// — as the real one does — mints a **new** access token at every exchange.
pub struct GitHubMintingATokenPerExchange {
    exchanges: Arc<AtomicUsize>,
}

impl GitHubMintingATokenPerExchange {
    fn grant(&self, login: &str) -> (String, GitHubUser) {
        let n = self.exchanges.fetch_add(1, Ordering::SeqCst) + 1;
        (
            the_token_granted_at_exchange(n),
            GitHubUser {
                id: 7,
                login: login.to_string(),
                avatar_url: String::new(),
                name: "The Operator".to_string(),
            },
        )
    }
}

#[async_trait]
impl GitHubOAuthProvider for GitHubMintingATokenPerExchange {
    fn authorize_url(&self) -> Result<(String, String), String> {
        Ok((
            "https://github.com/login/oauth/authorize".to_string(),
            "the-state".to_string(),
        ))
    }

    async fn exchange_code(
        &self,
        code: &str,
        _state: &str,
    ) -> Result<(String, GitHubUser), String> {
        Ok(self.grant(code))
    }

    async fn start_device_login(&self) -> Result<DeviceLoginStart, String> {
        Ok(DeviceLoginStart {
            device_code: THE_LOGIN.to_string(),
            user_code: "WDJB-MJHT".to_string(),
            verification_uri: "https://github.com/login/device".to_string(),
            expires_in_seconds: 900,
            interval_seconds: 5,
        })
    }

    async fn poll_device_login(&self, device_code: &str) -> Result<DeviceLoginPoll, String> {
        let (access_token, user) = self.grant(device_code);
        Ok(DeviceLoginPoll::Complete { access_token, user })
    }

    fn issues_usable_access_token(&self) -> bool {
        true
    }
}

/// A demo daemon, wired exactly as the daemon wires itself, keeping its credentials under
/// `storage`.
pub fn a_demo_daemon_retaining_credentials_in(
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

/// The `auth.AuthService` a configured daemon serves.
pub fn the_auth_service(config: &DaemonConfig) -> ServiceEntry {
    tddy_daemon_auth::auth::build_auth_entries(config, "127.0.0.1", 8080)
        .expect("a daemon with github configured builds its auth entries")
        .entries
        .into_iter()
        .find(|entry| entry.name == "auth.AuthService")
        .expect("auth.AuthService is one of the entries")
}

/// One unary call through the served entry, as a client would make it.
pub async fn call<Req: prost::Message, Res: prost::Message + Default>(
    entry: &ServiceEntry,
    method: &str,
    request: Req,
) -> Result<Res, Status> {
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
        .await?
    {
        tddy_rpc::ResponseBody::Complete(chunks) => {
            Ok(Res::decode(&chunks[0][..]).expect("a unary response decodes"))
        }
        _ => panic!("{method} is unary"),
    }
}

/// A clock that moves only when the test moves it.
pub struct AHandDrivenClock(std::sync::Mutex<std::time::Instant>);

impl AHandDrivenClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(std::sync::Mutex::new(std::time::Instant::now())))
    }

    pub fn advance(&self, by: std::time::Duration) {
        *self.0.lock().unwrap() += by;
    }

    pub fn as_clock(self: &Arc<Self>) -> tddy_credentials::Clock {
        let clock = Arc::clone(self);
        Arc::new(move || *clock.0.lock().unwrap())
    }
}
