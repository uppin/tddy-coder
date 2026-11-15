//! What `ConnectCoderAccount` must answer, and what a later reader of the account is given.
//!
//! A Coder account is connected by entering the deployment's URL and a session token. The pair is
//! checked by asking the deployment who the token belongs to **before** anything is stored, and
//! the vault is consulted before the network, so a closed vault refuses without the token leaving
//! the daemon. The vault is the real one, sealed in a temporary directory; the deployment is a
//! stand-in, because the network is the daemon's concern.
//!
//! Feature: docs/ft/daemon/provider-accounts.md

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_accounts::{
    AccountsError, AccountsServiceImpl, CoderAccount, CoderAccountError, CoderAccounts,
    CoderDeploymentProbe, CoderDeploymentUrl, CoderProbeError, CoderUser, SessionSubjectResolver,
    SessionVaultAccountStore, CODER_PROVIDER,
};
use tddy_credentials::{
    AccountId, CredentialRecord, CredentialStore, ProviderId, SecretString, SessionVaults,
};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::accounts::{
    AccountSummary, AccountsService, ConnectCoderAccountRequest, ListAccountsRequest,
};

const ADA: &str = "ada";
const ADAS_SESSION: &str = "session-token-for-ada";
const ADAS_PASSPHRASE: &str = "correct horse battery staple";

const THE_DEPLOYMENT: &str = "https://coder.example.com";
const ADAS_CODER_ID: &str = "3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10";
const ADAS_CODER_NAME: &str = "ada-lovelace";
const ADAS_ACCOUNT_ID: &str = "coder-3f1c9a52-7d0e-4b7a-9a43-0c2f8f1f6b10@coder.example.com";
const A_CODER_TOKEN: &str = "coder-token-that-must-never-be-echoed";
const A_NEWER_CODER_TOKEN: &str = "a-newer-coder-token-never-echoed-either";

// ---------------------------------------------------------------------------------------------
// The deployment

/// How the stand-in deployment answers `whoami`.
enum Answer {
    /// Names Ada for each of these tokens, and rejects every other one.
    AdaFor(Vec<&'static str>),
    /// Nothing answers at the URL.
    Unreachable,
}

/// A Coder deployment that answers out of memory and remembers what it was asked.
struct ACoderDeployment {
    answer: Answer,
    asked: Mutex<Vec<(String, String)>>,
}

fn a_deployment_knowing_ada() -> ACoderDeployment {
    ACoderDeployment {
        answer: Answer::AdaFor(vec![A_CODER_TOKEN, A_NEWER_CODER_TOKEN]),
        asked: Mutex::new(Vec::new()),
    }
}

fn an_unreachable_deployment() -> ACoderDeployment {
    ACoderDeployment {
        answer: Answer::Unreachable,
        asked: Mutex::new(Vec::new()),
    }
}

impl ACoderDeployment {
    fn was_never_asked(&self) -> bool {
        self.asked.lock().unwrap().is_empty()
    }
}

#[async_trait]
impl CoderDeploymentProbe for ACoderDeployment {
    async fn whoami(
        &self,
        url: &CoderDeploymentUrl,
        token: &SecretString,
    ) -> Result<CoderUser, CoderProbeError> {
        self.asked
            .lock()
            .unwrap()
            .push((url.as_str().to_string(), token.expose().to_string()));
        match &self.answer {
            Answer::AdaFor(tokens) if tokens.contains(&token.expose()) => Ok(CoderUser {
                id: ADAS_CODER_ID.to_string(),
                username: ADAS_CODER_NAME.to_string(),
            }),
            Answer::AdaFor(_) => Err(CoderProbeError::Rejected(
                "401 Unauthorized: API key is invalid".to_string(),
            )),
            Answer::Unreachable => Err(CoderProbeError::Unreachable(
                "connection refused".to_string(),
            )),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The daemon

struct World {
    _dir: tempfile::TempDir,
    vaults: Arc<SessionVaults>,
    deployment: Arc<ACoderDeployment>,
    coder: Arc<CoderAccounts>,
    service: AccountsServiceImpl<SessionVaultAccountStore>,
}

fn adas_passphrase() -> SecretString {
    SecretString::new(ADAS_PASSPHRASE)
}

/// Ada chose a passphrase on an earlier run of the daemon, and her vault holds `records`.
fn ada_created_her_vault_holding(dir: &Path, records: Vec<CredentialRecord>) {
    let vault =
        CredentialStore::create(&CredentialStore::path_in(dir, ADA), &adas_passphrase(), ADA)
            .expect("Ada's vault is created");
    for record in records {
        vault.put(record).expect("the record is retained");
    }
}

/// A daemon whose vault registry holds Ada's vault — sealed, holding `records`, not yet unlocked —
/// and that reaches `deployment` when it is asked to connect a Coder account.
fn a_daemon_where_adas_vault_is_locked(
    records: Vec<CredentialRecord>,
    deployment: ACoderDeployment,
) -> World {
    let dir = tempfile::tempdir().expect("a temporary directory");
    ada_created_her_vault_holding(dir.path(), records);
    let vaults = Arc::new(SessionVaults::new(dir.path()));
    let subject_of: SessionSubjectResolver =
        Arc::new(|token: &str| (token == ADAS_SESSION).then(|| ADA.to_string()));
    let deployment = Arc::new(deployment);
    let coder = Arc::new(CoderAccounts::new(
        Arc::clone(&vaults),
        subject_of.clone(),
        Arc::clone(&deployment) as Arc<dyn CoderDeploymentProbe>,
    ));
    let service = AccountsServiceImpl::new(Arc::new(SessionVaultAccountStore::new(
        Arc::clone(&vaults),
        subject_of,
    )))
    .with_coder_accounts(Arc::clone(&coder));
    World {
        _dir: dir,
        vaults,
        deployment,
        coder,
        service,
    }
}

/// The same daemon after Ada unlocked her (empty) vault with her passphrase.
fn a_daemon_where_ada_unlocked_her_vault(deployment: ACoderDeployment) -> World {
    let world = a_daemon_where_adas_vault_is_locked(Vec::new(), deployment);
    world
        .vaults
        .unlock(ADA, &adas_passphrase())
        .expect("Ada's passphrase opens her vault");
    world
}

fn a_connect(deployment_url: &str, token: &str, label: &str) -> ConnectCoderAccountRequest {
    ConnectCoderAccountRequest {
        session_token: ADAS_SESSION.to_string(),
        deployment_url: deployment_url.to_string(),
        token: token.to_string(),
        label: label.to_string(),
    }
}

impl World {
    async fn connect(&self, request: ConnectCoderAccountRequest) -> Result<AccountSummary, Status> {
        self.service
            .connect_coder_account(Request::direct(request))
            .await
            .map(|response| response.into_inner().account.expect("an account"))
    }

    async fn listed(&self) -> Vec<(String, Vec<AccountSummary>)> {
        self.service
            .list_accounts(Request::direct(ListAccountsRequest {
                session_token: ADAS_SESSION.to_string(),
            }))
            .await
            .expect("listing")
            .into_inner()
            .providers
            .into_iter()
            .map(|group| (group.provider, group.accounts))
            .collect()
    }

    /// Every record Ada's open vault holds, read past the service.
    fn sealed(&self) -> Vec<CredentialRecord> {
        self.vaults
            .get(ADA)
            .expect("Ada's vault is open")
            .list(None)
            .expect("the vault lists")
    }
}

/// The Coder account Ada has at the deployment, as the vault would hold it with `token`.
fn adas_coder_record(token: &str) -> CredentialRecord {
    CoderAccount {
        account_id: AccountId::new(ADAS_ACCOUNT_ID),
        label: "Work Coder".to_string(),
        deployment_url: CoderDeploymentUrl::parse(THE_DEPLOYMENT).expect("a deployment URL"),
        username: ADAS_CODER_NAME.to_string(),
        user_id: ADAS_CODER_ID.to_string(),
    }
    .into_record(SecretString::new(token), 1_760_000_000)
}

// ---------------------------------------------------------------------------------------------
// Connecting

#[tokio::test]
async fn connecting_a_deployment_stores_a_coder_account_named_after_its_user() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());

    // When
    let connected = world
        .connect(a_connect(THE_DEPLOYMENT, A_CODER_TOKEN, "Work Coder"))
        .await
        .expect("a token the deployment accepts connects");

    // Then
    assert_eq!(connected.provider, CODER_PROVIDER);
    assert_eq!(connected.account_id, ADAS_ACCOUNT_ID);
    assert_eq!(connected.subject, ADAS_CODER_NAME);
    assert_eq!(connected.label, "Work Coder");
    let sealed = world.sealed();
    assert_eq!(sealed.len(), 1);
    assert_eq!(sealed[0].provider, ProviderId::new(CODER_PROVIDER));
    assert_eq!(sealed[0].account, AccountId::new(ADAS_ACCOUNT_ID));
    assert_eq!(sealed[0].secret, SecretString::new(A_CODER_TOKEN));
    assert_eq!(
        *world.deployment.asked.lock().unwrap(),
        vec![(THE_DEPLOYMENT.to_string(), A_CODER_TOKEN.to_string())]
    );
}

#[tokio::test]
async fn a_connected_coder_account_lists_without_its_token() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());
    world
        .connect(a_connect(THE_DEPLOYMENT, A_CODER_TOKEN, "Work Coder"))
        .await
        .expect("connected");

    // When
    let listed = world.listed().await;

    // Then
    assert_eq!(listed.len(), 1);
    let (provider, accounts) = &listed[0];
    assert_eq!(provider, CODER_PROVIDER);
    assert_eq!(accounts.len(), 1);
    let account = &accounts[0];
    assert_eq!(account.account_id, ADAS_ACCOUNT_ID);
    assert_eq!(account.label, "Work Coder");
    assert_eq!(account.subject, ADAS_CODER_NAME);
    assert!(account.has_secret);
    assert_eq!(account.preset, "");
    assert_eq!(account.wire_protocols, Vec::<String>::new());
    assert!(!format!("{listed:?}").contains(A_CODER_TOKEN));
}

#[tokio::test]
async fn a_token_the_deployment_rejects_is_refused_and_nothing_is_stored() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());

    // When
    let refusal = world
        .connect(a_connect(
            THE_DEPLOYMENT,
            "a-token-nobody-issued",
            "Work Coder",
        ))
        .await
        .expect_err("a rejected token is refused");

    // Then
    assert_eq!(refusal.code(), Code::Unauthenticated);
    assert!(refusal.message().contains("API key is invalid"));
    assert!(!refusal.message().contains("a-token-nobody-issued"));
    assert_eq!(world.sealed(), Vec::new());
}

#[tokio::test]
async fn an_unreachable_deployment_is_refused_as_unavailable() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(an_unreachable_deployment());

    // When
    let refusal = world
        .connect(a_connect(THE_DEPLOYMENT, A_CODER_TOKEN, "Work Coder"))
        .await
        .expect_err("an unreachable deployment is refused");

    // Then
    assert_eq!(refusal.code(), Code::Unavailable);
    assert!(refusal.message().contains("connection refused"));
    assert_eq!(world.sealed(), Vec::new());
}

#[tokio::test]
async fn a_url_with_userinfo_or_a_query_is_refused() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());

    // When
    let with_userinfo = world
        .connect(a_connect(
            "https://ada:hunter2@coder.example.com",
            A_CODER_TOKEN,
            "Work Coder",
        ))
        .await
        .expect_err("userinfo is refused");
    let with_a_query = world
        .connect(a_connect(
            "https://coder.example.com?next=/workspaces",
            A_CODER_TOKEN,
            "Work Coder",
        ))
        .await
        .expect_err("a query is refused");

    // Then
    assert_eq!(
        (with_userinfo.code(), with_a_query.code()),
        (Code::InvalidArgument, Code::InvalidArgument)
    );
    assert!(world.deployment.was_never_asked());
    assert_eq!(world.sealed(), Vec::new());
}

#[tokio::test]
async fn an_empty_token_is_refused_before_the_deployment_is_asked() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());

    // When
    let refusal = world
        .connect(a_connect(THE_DEPLOYMENT, "", "Work Coder"))
        .await
        .expect_err("an empty token is refused");

    // Then
    assert_eq!(refusal.code(), Code::InvalidArgument);
    assert!(world.deployment.was_never_asked());
    assert_eq!(world.sealed(), Vec::new());
}

#[tokio::test]
async fn reconnecting_the_same_user_replaces_the_token_and_keeps_the_label() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());
    let first = world
        .connect(a_connect(THE_DEPLOYMENT, A_CODER_TOKEN, "Work Coder"))
        .await
        .expect("the first connect succeeds");

    // When
    let second = world
        .connect(a_connect(
            THE_DEPLOYMENT,
            A_NEWER_CODER_TOKEN,
            "Typed again differently",
        ))
        .await
        .expect("the reconnect succeeds");

    // Then
    assert_eq!(second.account_id, first.account_id);
    assert_eq!(second.label, "Work Coder");
    let sealed = world.sealed();
    assert_eq!(sealed.len(), 1);
    assert_eq!(sealed[0].label, "Work Coder");
    assert_eq!(sealed[0].secret, SecretString::new(A_NEWER_CODER_TOKEN));
}

#[tokio::test]
async fn a_locked_vault_refuses_the_connect_as_a_precondition() {
    // Given
    let world = a_daemon_where_adas_vault_is_locked(Vec::new(), a_deployment_knowing_ada());

    // When
    let refusal = world
        .connect(a_connect(THE_DEPLOYMENT, A_CODER_TOKEN, "Work Coder"))
        .await
        .expect_err("a locked vault refuses");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
    assert!(refusal.message().contains("locked"));
    assert!(world.deployment.was_never_asked());
}

#[tokio::test]
async fn a_daemon_that_wired_no_coder_accounts_refuses_to_connect_one() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());
    let unwired = AccountsServiceImpl::new(Arc::new(SessionVaultAccountStore::new(
        Arc::clone(&world.vaults),
        Arc::new(|token: &str| (token == ADAS_SESSION).then(|| ADA.to_string())),
    )));

    // When
    let refusal = unwired
        .connect_coder_account(Request::direct(a_connect(
            THE_DEPLOYMENT,
            A_CODER_TOKEN,
            "Work Coder",
        )))
        .await
        .expect_err("unwired");

    // Then
    assert_eq!(refusal.code(), Code::FailedPrecondition);
}

// ---------------------------------------------------------------------------------------------
// Reading the credentials

#[tokio::test]
async fn the_stored_url_and_token_are_readable_while_the_vault_is_open() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());
    let connected = world
        .connect(a_connect(THE_DEPLOYMENT, A_CODER_TOKEN, "Work Coder"))
        .await
        .expect("connected");

    // When
    let credentials = world
        .coder
        .credentials_for(ADA, &AccountId::new(connected.account_id))
        .expect("an open vault gives the credentials");

    // Then
    assert_eq!(credentials.url.as_str(), THE_DEPLOYMENT);
    assert_eq!(credentials.token, SecretString::new(A_CODER_TOKEN));
}

#[tokio::test]
async fn the_stored_token_is_not_readable_from_a_locked_vault() {
    // Given
    let world = a_daemon_where_adas_vault_is_locked(
        vec![adas_coder_record(A_CODER_TOKEN)],
        a_deployment_knowing_ada(),
    );

    // When
    let refusal = world
        .coder
        .credentials_for(ADA, &AccountId::new(ADAS_ACCOUNT_ID));

    // Then
    assert_eq!(
        refusal,
        Err(CoderAccountError::Vault(AccountsError::Locked))
    );
}

#[tokio::test]
async fn an_account_the_open_vault_does_not_hold_has_no_credentials() {
    // Given
    let world = a_daemon_where_ada_unlocked_her_vault(a_deployment_knowing_ada());

    // When
    let refusal = world
        .coder
        .credentials_for(ADA, &AccountId::new(ADAS_ACCOUNT_ID));

    // Then
    assert_eq!(
        refusal,
        Err(CoderAccountError::NotFound(AccountId::new(ADAS_ACCOUNT_ID)))
    );
}
