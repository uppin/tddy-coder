//! Adding a second GitHub account without becoming it.
//!
//! **The load-bearing test here is a negative one.** After a link completes, the caller is still
//! who they were: the account their session was established with has not moved, and no token of
//! any kind came back. The convenient implementation — call `#keyring` 2/9's login path and discard
//! the session it returns — passes every positive test in this file, and its first symptom in
//! production is a person silently acting as someone else.
//!
//! Second in weight is the re-link, whose failure is also silent: a fresh `account_id` leaves
//! `#keyring` 5/9's project assignments pointing at nothing, and the resolver then answers
//! `NotAssigned` — the same answer it gives when nobody ever assigned one.
//!
//! Both fakes below are **storage and transport only**. Deduplication and preservation go through
//! the real [`record_for_link`], and the removal refusal through the real [`removal_allowed`],
//! because those two decisions are what this node claims.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_accounts::{
    AccountLinker, AccountStore, AccountsError, AccountsServiceImpl, LinkChallenge, LinkError,
    LinkProgress, LinkedAccountStore, LinkedIdentity, META_SUBJECT, META_SUBJECT_ID,
};
use tddy_credentials::{AccountId, CredentialRecord, ProviderId, FIRST_VERSION};
use tddy_rpc::{Request, Status};
use tddy_service::proto::accounts::{
    AccountsService, BeginLinkAccountRequest, BeginLinkAccountResponse, LinkState,
    ListAccountsRequest, ListAccountsResponse, PollLinkAccountRequest, PollLinkAccountResponse,
    RemoveAccountRequest, RemoveAccountResponse,
};

const ADAS_SESSION: &str = "session-token-for-ada";
const GITHUB: &str = "github";

// ---------------------------------------------------------------------------------------------
// Builders

fn github() -> ProviderId {
    ProviderId::new(GITHUB)
}

fn ada() -> LinkedIdentity {
    LinkedIdentity {
        subject_id: "1024".to_string(),
        login: "ada".to_string(),
    }
}

fn grace() -> LinkedIdentity {
    LinkedIdentity {
        subject_id: "2048".to_string(),
        login: "grace".to_string(),
    }
}

/// The account Ada's session was established with — already in the vault before anything is linked.
fn adas_own_account() -> CredentialRecord {
    let mut metadata = BTreeMap::new();
    metadata.insert(META_SUBJECT_ID.to_string(), ada().subject_id);
    metadata.insert(META_SUBJECT.to_string(), ada().login);

    CredentialRecord {
        provider: github(),
        account: AccountId::new("account-ada"),
        label: "Ada".to_string(),
        secret: "the-token-ada-signed-in-with".to_string(),
        metadata,
        updated_at: 1_726_700_000,
        version: FIRST_VERSION,
    }
}

/// A second account, already linked — the vault holds it and no session belongs to it.
fn graces_linked_account() -> CredentialRecord {
    let mut metadata = BTreeMap::new();
    metadata.insert(META_SUBJECT_ID.to_string(), grace().subject_id);
    metadata.insert(META_SUBJECT.to_string(), grace().login);

    CredentialRecord {
        provider: github(),
        account: AccountId::new("account-grace"),
        label: "Grace".to_string(),
        secret: "the-token-grace-linked-with".to_string(),
        metadata,
        updated_at: 1_726_700_000,
        version: FIRST_VERSION,
    }
}

// ---------------------------------------------------------------------------------------------
// The provider's dance, faked

/// Transport only: it answers with whatever outcome the test set, and counts nothing else.
///
/// It cannot mint a session even by accident — [`AccountLinker`] has no method that produces one
/// and no access to the store, which is the point of the port being this narrow.
struct AProviderThatAnswers {
    outcome: Mutex<LinkProgress>,
    refusal: Option<LinkError>,
}

fn a_provider_that_approves(identity: LinkedIdentity, access_token: &str) -> AProviderThatAnswers {
    AProviderThatAnswers {
        outcome: Mutex::new(LinkProgress::Approved {
            identity,
            access_token: access_token.to_string(),
        }),
        refusal: None,
    }
}

fn a_provider_that_answers(outcome: LinkProgress) -> AProviderThatAnswers {
    AProviderThatAnswers {
        outcome: Mutex::new(outcome),
        refusal: None,
    }
}

impl AccountLinker for AProviderThatAnswers {
    fn begin(&self, provider: &ProviderId) -> Result<LinkChallenge, LinkError> {
        self.refusal.clone().map_or_else(
            || {
                (provider == &github())
                    .then(|| LinkChallenge {
                        link_id: "link-1".to_string(),
                        user_code: "WXYZ-1234".to_string(),
                        verification_uri: "https://github.com/login/device".to_string(),
                        expires_in_seconds: 900,
                        interval_seconds: 5,
                    })
                    .ok_or_else(|| LinkError::UnsupportedProvider(provider.as_str().to_string()))
            },
            Err,
        )
    }

    fn poll(&self, _link_id: &str) -> Result<LinkProgress, LinkError> {
        self.refusal
            .clone()
            .map_or_else(|| Ok(self.outcome.lock().unwrap().clone()), Err)
    }
}

// ---------------------------------------------------------------------------------------------
// The vault, faked

/// Storage only. Whether a record deduplicates against what is held is decided by the real
/// `record_for_link`, not here.
struct AVaultAdaCanOpen {
    records: Mutex<Vec<CredentialRecord>>,
    session_account: Option<(ProviderId, AccountId)>,
    locked: bool,
}

fn a_vault_holding(record: CredentialRecord) -> AVaultAdaCanOpen {
    AVaultAdaCanOpen {
        session_account: Some((record.provider.clone(), record.account.clone())),
        records: Mutex::new(vec![record]),
        locked: false,
    }
}

impl AVaultAdaCanOpen {
    fn also_holding(self, record: CredentialRecord) -> Self {
        self.records.lock().unwrap().push(record);
        self
    }

    fn sealed_under_another_login(mut self) -> Self {
        self.locked = true;
        self
    }

    fn accounts_it_holds(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .records
            .lock()
            .unwrap()
            .iter()
            .map(|record| record.account.as_str().to_string())
            .collect();
        ids.sort();
        ids
    }

    fn admits(&self, session_token: &str) -> Result<(), LinkError> {
        self.locked
            .then_some(LinkError::Locked)
            .or_else(|| (session_token != ADAS_SESSION).then_some(LinkError::NoSuchSession))
            .map_or(Ok(()), Err)
    }
}

impl LinkedAccountStore for AVaultAdaCanOpen {
    fn held(
        &self,
        session_token: &str,
        provider: &ProviderId,
    ) -> Result<Vec<CredentialRecord>, LinkError> {
        self.admits(session_token)?;
        Ok(self
            .records
            .lock()
            .unwrap()
            .iter()
            .filter(|record| &record.provider == provider)
            .cloned()
            .collect())
    }

    fn put(&self, session_token: &str, record: CredentialRecord) -> Result<(), LinkError> {
        self.admits(session_token)?;
        let mut records = self.records.lock().unwrap();
        records.retain(|held| held.provider != record.provider || held.account != record.account);
        records.push(record);
        Ok(())
    }

    fn session_account(
        &self,
        session_token: &str,
    ) -> Result<Option<(ProviderId, AccountId)>, LinkError> {
        self.admits(session_token)?;
        Ok(self.session_account.clone())
    }
}

/// The read-and-curate port `#keyring` 4/9 owns, over the same records, so one test can link an
/// account and then list what the vault holds.
impl AccountStore for AVaultAdaCanOpen {
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError> {
        self.admits(session_token).map_err(as_accounts_error)?;
        let mut records = self.records.lock().unwrap().clone();
        records.sort_by(|left, right| {
            (left.provider.as_str(), left.account.as_str())
                .cmp(&(right.provider.as_str(), right.account.as_str()))
        });
        Ok(records)
    }

    fn set_label(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
        label: &str,
    ) -> Result<CredentialRecord, AccountsError> {
        self.admits(session_token).map_err(as_accounts_error)?;
        let mut records = self.records.lock().unwrap();
        let found = records
            .iter_mut()
            .find(|record| &record.provider == provider && &record.account == account)
            .ok_or_else(|| AccountsError::Unavailable("no such account".to_string()))?;
        found.label = label.to_string();
        Ok(found.clone())
    }

    fn remove(
        &self,
        session_token: &str,
        provider: &ProviderId,
        account: &AccountId,
    ) -> Result<(), AccountsError> {
        self.admits(session_token).map_err(as_accounts_error)?;
        self.records
            .lock()
            .unwrap()
            .retain(|record| &record.provider != provider || &record.account != account);
        Ok(())
    }
}

fn as_accounts_error(error: LinkError) -> AccountsError {
    match error {
        LinkError::NoSuchSession => AccountsError::NoSuchSession,
        LinkError::Locked => AccountsError::Locked,
        other => AccountsError::Unavailable(format!("{other:?}")),
    }
}

// ---------------------------------------------------------------------------------------------
// Asking the service

fn a_service(
    vault: &Arc<AVaultAdaCanOpen>,
    provider: AProviderThatAnswers,
) -> AccountsServiceImpl<AVaultAdaCanOpen> {
    AccountsServiceImpl::new(Arc::clone(vault)).with_linking(
        Arc::new(provider),
        Arc::clone(vault) as Arc<dyn LinkedAccountStore>,
    )
}

async fn a_link_begun_by(
    service: &AccountsServiceImpl<AVaultAdaCanOpen>,
    session_token: &str,
) -> Result<BeginLinkAccountResponse, Status> {
    service
        .begin_link_account(Request::new(BeginLinkAccountRequest {
            session_token: session_token.to_string(),
            provider: GITHUB.to_string(),
        }))
        .await
        .map(tddy_rpc::Response::into_inner)
}

async fn the_poll_after(
    service: &AccountsServiceImpl<AVaultAdaCanOpen>,
    link_id: &str,
) -> Result<PollLinkAccountResponse, Status> {
    service
        .poll_link_account(Request::new(PollLinkAccountRequest {
            session_token: ADAS_SESSION.to_string(),
            link_id: link_id.to_string(),
        }))
        .await
        .map(tddy_rpc::Response::into_inner)
}

/// Begin and poll once — the whole flow a person sees, with no waiting in it.
async fn a_completed_link(
    service: &AccountsServiceImpl<AVaultAdaCanOpen>,
) -> Result<PollLinkAccountResponse, Status> {
    let begun = a_link_begun_by(service, ADAS_SESSION).await?;
    the_poll_after(service, &begun.link_id).await
}

async fn the_listing_from(
    service: &AccountsServiceImpl<AVaultAdaCanOpen>,
) -> Result<ListAccountsResponse, Status> {
    service
        .list_accounts(Request::new(ListAccountsRequest {
            session_token: ADAS_SESSION.to_string(),
        }))
        .await
        .map(tddy_rpc::Response::into_inner)
}

async fn removing(
    service: &AccountsServiceImpl<AVaultAdaCanOpen>,
    account_id: &str,
) -> Result<RemoveAccountResponse, Status> {
    service
        .remove_account(Request::new(RemoveAccountRequest {
            session_token: ADAS_SESSION.to_string(),
            provider: GITHUB.to_string(),
            account_id: account_id.to_string(),
        }))
        .await
        .map(tddy_rpc::Response::into_inner)
}

// ---------------------------------------------------------------------------------------------
// The reason this node exists

#[tokio::test]
async fn linking_leaves_the_caller_signed_in_as_who_they_already_were() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let linked = a_completed_link(&service).await;

    // Then
    assert_eq!(
        linked
            .map_err(|status| status.message)
            .map(|response| response.state)
            .map(|state| (
                state,
                vault
                    .session_account(ADAS_SESSION)
                    .expect("the session's account")
            )),
        Ok((
            LinkState::LinkLinked as i32,
            Some((github(), AccountId::new("account-ada")))
        ))
    );
}

#[tokio::test]
async fn a_completed_link_hands_back_no_token_of_any_kind() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let linked = a_completed_link(&service).await.expect("a completed link");

    // Then
    let on_the_wire = String::from_utf8_lossy(&linked.encode_to_vec()).to_string();
    assert_eq!(
        (
            on_the_wire.contains("grace-s-token"),
            on_the_wire.contains("the-token-ada-signed-in-with")
        ),
        (false, false)
    );
}

// ---------------------------------------------------------------------------------------------
// The flow a person walks through

#[tokio::test]
async fn beginning_a_link_shows_the_operator_a_code_and_where_to_type_it() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let begun = a_link_begun_by(&service, ADAS_SESSION).await;

    // Then
    assert_eq!(
        begun
            .map_err(|status| status.message)
            .map(|response| (response.user_code, response.verification_uri)),
        Ok((
            "WXYZ-1234".to_string(),
            "https://github.com/login/device".to_string()
        ))
    );
}

#[tokio::test]
async fn a_link_nobody_has_approved_yet_is_reported_as_pending() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(
        &vault,
        a_provider_that_answers(LinkProgress::Pending {
            interval_seconds: 10,
        }),
    );

    // When
    let polled = a_completed_link(&service).await;

    // Then
    assert_eq!(
        polled
            .map_err(|status| status.message)
            .map(|response| (response.state, response.interval_seconds)),
        Ok((LinkState::LinkPending as i32, 10))
    );
}

#[tokio::test]
async fn a_refused_link_is_reported_as_refused_rather_than_as_a_failure() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_answers(LinkProgress::Denied));

    // When
    let polled = a_completed_link(&service).await;

    // Then
    assert_eq!(
        polled
            .map_err(|status| status.message)
            .map(|response| response.state),
        Ok(LinkState::LinkDenied as i32)
    );
}

#[tokio::test]
async fn a_code_that_outlived_its_window_is_reported_as_expired() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_answers(LinkProgress::Expired));

    // When
    let polled = a_completed_link(&service).await;

    // Then
    assert_eq!(
        polled
            .map_err(|status| status.message)
            .map(|response| response.state),
        Ok(LinkState::LinkExpired as i32)
    );
}

#[tokio::test]
async fn an_approval_with_nowhere_to_put_it_is_reported_as_locked_and_not_as_refused() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()).sealed_under_another_login());
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let polled = a_completed_link(&service).await;

    // Then
    assert_eq!(
        polled
            .map_err(|status| status.message)
            .map(|response| response.state),
        Ok(LinkState::LinkVaultLocked as i32)
    );
}

#[tokio::test]
async fn a_token_naming_no_session_cannot_begin_a_link() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let begun = a_link_begun_by(&service, "a-token-for-nobody").await;

    // Then
    assert_eq!(
        begun.map(|_| ()).map_err(|status| status.code),
        Err(tddy_rpc::Code::Unauthenticated)
    );
}

// ---------------------------------------------------------------------------------------------
// Two accounts, one session

#[tokio::test]
async fn the_linked_account_joins_the_one_the_session_was_established_with() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let _ = a_completed_link(&service).await;

    // Then
    assert_eq!(vault.accounts_it_holds().len(), 2);
}

#[tokio::test]
async fn the_listing_says_which_account_the_session_belongs_to() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let listing = the_listing_from(&service).await;

    // Then
    assert_eq!(
        listing
            .map_err(|status| status.message)
            .map(|response| response.session_account),
        Ok(Some(tddy_service::proto::accounts::SessionAccount {
            provider: GITHUB.to_string(),
            account_id: "account-ada".to_string(),
        }))
    );
}

#[tokio::test]
async fn re_linking_an_account_keeps_the_account_id_a_project_was_assigned_to() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_approves(ada(), "ada-s-fresh-token"));

    // When
    let _ = a_completed_link(&service).await;

    // Then
    assert_eq!(vault.accounts_it_holds(), vec!["account-ada".to_string()]);
}

// ---------------------------------------------------------------------------------------------
// What may be forgotten

#[tokio::test]
async fn removing_the_account_the_session_was_established_with_is_refused_with_the_reason() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()));
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let removal = removing(&service, "account-ada").await;

    // Then
    assert_eq!(
        removal
            .map(|_| ())
            .map_err(|status| (status.code, status.message.contains("session"))),
        Err((tddy_rpc::Code::FailedPrecondition, true))
    );
}

#[tokio::test]
async fn removing_any_other_linked_account_succeeds() {
    // Given
    let vault = Arc::new(a_vault_holding(adas_own_account()).also_holding(graces_linked_account()));
    let service = a_service(&vault, a_provider_that_approves(grace(), "grace-s-token"));

    // When
    let removal = removing(&service, "account-grace").await;

    // Then
    assert_eq!(removal.map(|_| ()).map_err(|status| status.message), Ok(()));
}
