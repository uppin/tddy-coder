//! What `accounts.AccountsService` answers on a daemon that signs in through a stub GitHub.
//!
//! A stub login holds no credential, so `tddy-github` never opens a vault for one (`#keyring` 3/9)
//! and nothing a stub user can do makes `/accounts` list anything. To render a real list, these
//! tests reach the daemon's own [`SessionVaults`] — the handle its accounts service reads, through
//! [`tddy_daemon::runtime::DaemonRuntime::credential_vaults`] — and open the stub user's vault in
//! memory the way a real login followed by a chosen passphrase would: retain a record, then create
//! the vault, which seals it. The call itself goes through the daemon's real RPC roster.

use std::collections::BTreeMap;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SecretString};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tddy_daemon_auth::SessionVaults;
use tddy_rpc::{MultiRpcService, RequestMetadata, RequestTransport, RpcBridge, RpcMessage, Status};
use tddy_service::proto::accounts::{
    AccountSummary, ListAccountsRequest, ListAccountsResponse, ProviderAccounts,
};
use tddy_service::proto::auth::{
    ExchangeCodeRequest, ExchangeCodeResponse, GetAuthUrlRequest, GetAuthUrlResponse,
};
use tempfile::TempDir;
use tokio::net::TcpListener;

const THE_STUB_USER: &str = "testuser";
const THE_STUB_USERS_CODE: &str = "testuser-code";
/// Recognisable on purpose: one test looks for it where it must never appear.
const A_KNOWN_SECRET: &str = "gho_a-known-secret-that-must-never-leave-the-vault";
const THE_VAULT_PASSPHRASE: &str = "correct horse battery staple";
const A_RECORD_WRITTEN_AT: u64 = 1_726_700_000;

#[tokio::test]
async fn a_stub_users_pre_opened_vault_is_listed_without_its_secret() {
    // Given a stub daemon on which testuser's vault is open and holds one GitHub account
    let daemon = a_stub_daemon_for(THE_STUB_USER).await;
    daemon
        .the_vault_of(THE_STUB_USER)
        .holding(a_github_record_for(THE_STUB_USER))
        .opened();
    let token = daemon.a_login_as(THE_STUB_USERS_CODE).await;

    // When testuser lists their accounts
    let listed = daemon
        .list_accounts(&token)
        .await
        .expect("ListAccounts answers an open vault");

    // Then the vault reads open and lists that account, holding a credential
    assert_eq!(
        listed.response,
        ListAccountsResponse {
            providers: vec![ProviderAccounts {
                provider: "github".to_string(),
                accounts: vec![AccountSummary {
                    provider: "github".to_string(),
                    account_id: THE_STUB_USER.to_string(),
                    label: "Work GitHub".to_string(),
                    subject: THE_STUB_USER.to_string(),
                    updated_at: A_RECORD_WRITTEN_AT as i64,
                    has_secret: true,
                }],
            }],
            vault_locked: false,
            vault_uninitialized: false,
        }
    );
    // And the secret is nowhere in what went over the wire
    assert!(
        !String::from_utf8_lossy(&listed.wire_bytes).contains(A_KNOWN_SECRET),
        "the serialised ListAccounts response carried the vault's secret"
    );
}

#[tokio::test]
async fn a_stub_user_with_no_vault_is_told_none_exists_yet() {
    // Given a stub daemon on which nothing has created testuser's vault
    let daemon = a_stub_daemon_for(THE_STUB_USER).await;
    let token = daemon.a_login_as(THE_STUB_USERS_CODE).await;

    // When testuser lists their accounts
    let listed = daemon
        .list_accounts(&token)
        .await
        .expect("ListAccounts answers a missing vault");

    // Then the answer is that no vault exists yet — not an empty one
    assert_eq!(
        listed.response,
        ListAccountsResponse {
            providers: vec![],
            vault_locked: false,
            vault_uninitialized: true,
        }
    );
}

// ---------------------------------------------------------------------------------------------
// The daemon under test

/// A daemon built from a stub-GitHub config with `auth_storage`, and its RPC roster.
struct AStubDaemon {
    rpc: RpcBridge<MultiRpcService>,
    vaults: Arc<SessionVaults>,
    _dir: TempDir,
}

/// A daemon whose stub GitHub signs `user` in, mapping them to the OS user this test runs as, and
/// keeping credential vaults in a temporary directory.
async fn a_stub_daemon_for(user: &str) -> AStubDaemon {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let runtime = runtime::build(
        a_stub_config_for(user, &dir).await,
        RuntimeOptions::for_binary(),
    )
    .await
    .expect("the daemon runtime builds");
    let vaults = Arc::clone(
        runtime
            .credential_vaults()
            .expect("a daemon with auth_storage keeps credential vaults"),
    );
    AStubDaemon {
        rpc: RpcBridge::new(MultiRpcService::new(runtime.entries)),
        vaults,
        _dir: dir,
    }
}

impl AStubDaemon {
    /// The test's hand on `subject`'s vault in this daemon's own memory.
    fn the_vault_of<'a>(&'a self, subject: &'a str) -> AVaultBeingPrepared<'a> {
        AVaultBeingPrepared {
            vaults: &self.vaults,
            subject,
        }
    }

    /// Sign in through the stub's redirect flow as whoever `code` names; returns the session token.
    async fn a_login_as(&self, code: &str) -> String {
        let url: GetAuthUrlResponse = self
            .call("auth.AuthService", "GetAuthUrl", GetAuthUrlRequest {})
            .await
            .expect("an authorize url is handed out")
            .response;
        let exchanged: ExchangeCodeResponse = self
            .call(
                "auth.AuthService",
                "ExchangeCode",
                ExchangeCodeRequest {
                    code: code.to_string(),
                    state: url.state,
                },
            )
            .await
            .expect("the stub completes the login")
            .response;
        exchanged.session_token
    }

    async fn list_accounts(
        &self,
        session_token: &str,
    ) -> Result<Answered<ListAccountsResponse>, Status> {
        self.call(
            "accounts.AccountsService",
            "ListAccounts",
            ListAccountsRequest {
                session_token: session_token.to_string(),
            },
        )
        .await
    }

    async fn call<Req: Message, Res: Message + Default>(
        &self,
        service: &str,
        method: &str,
        request: Req,
    ) -> Result<Answered<Res>, Status> {
        let message = RpcMessage::new(
            request.encode_to_vec(),
            RequestMetadata::over(RequestTransport::InProcess),
        );
        let wire_bytes = match self
            .rpc
            .handle_messages(service, method, &[message])
            .await?
        {
            tddy_rpc::ResponseBody::Complete(mut chunks) => chunks.remove(0),
            _ => panic!("{service}/{method} is unary"),
        };
        Ok(Answered {
            response: Res::decode(&wire_bytes[..]).expect("a unary response decodes"),
            wire_bytes,
        })
    }
}

/// A decoded response, and the bytes it was decoded from.
struct Answered<Res> {
    response: Res,
    wire_bytes: Vec<u8>,
}

/// One subject's vault in the daemon's memory, filled before it is opened.
struct AVaultBeingPrepared<'a> {
    vaults: &'a SessionVaults,
    subject: &'a str,
}

impl AVaultBeingPrepared<'_> {
    /// Hand the daemon `record` as a login would: held in memory until the vault opens.
    fn holding(self, record: CredentialRecord) -> Self {
        self.vaults
            .retain(self.subject, record)
            .expect("the daemon holds the record for the vault");
        self
    }

    /// Create the vault under a passphrase, which seals what it held and leaves it open.
    fn opened(self) {
        self.vaults
            .create(self.subject, &SecretString::new(THE_VAULT_PASSPHRASE))
            .expect("the vault is created and opened");
    }
}

/// A GitHub account for `login`, holding [`A_KNOWN_SECRET`].
fn a_github_record_for(login: &str) -> CredentialRecord {
    CredentialRecord {
        provider: ProviderId::new("github"),
        account: AccountId::new(login),
        label: "Work GitHub".to_string(),
        secret: SecretString::new(A_KNOWN_SECRET),
        metadata: BTreeMap::from([("subject".to_string(), login.to_string())]),
        updated_at: A_RECORD_WRITTEN_AT,
    }
}

/// Stub GitHub knowing `user` by [`THE_STUB_USERS_CODE`], a `users:` row for them, and
/// `auth_storage` and the data directory under `dir`.
async fn a_stub_config_for(user: &str, dir: &TempDir) -> DaemonConfig {
    let yaml = format!(
        "listen:\n  web_port: {port}\n  web_host: 127.0.0.1\n\
         tddy_data_dir: \"{data_dir}\"\n\
         auth_storage: \"{auth_storage}\"\n\
         github:\n  stub: true\n  stub_codes: \"{THE_STUB_USERS_CODE}:{user}\"\n\
         users:\n  - github_user: {user}\n    os_user: {os_user}\n",
        port = a_free_tcp_port().await,
        data_dir = dir.path().join("tddy").display(),
        auth_storage = dir.path().join("auth").display(),
        os_user = the_os_user_this_process_runs_as(),
    );
    serde_yaml::from_str(&yaml).expect("the config fixture parses")
}

fn the_os_user_this_process_runs_as() -> String {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    tddy_session_lifecycle::user_sessions_path::username_for_uid(unsafe { libc::getuid() })
        .expect("the test process runs as a named user")
}

/// A port that is free at this moment, so nothing in the fixture competes for a fixed one.
async fn a_free_tcp_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available")
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}
