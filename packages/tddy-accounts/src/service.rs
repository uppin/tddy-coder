//! `accounts.AccountsService` over an [`AccountStore`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use tddy_credential_sync::AccountSyncSummary;
use tddy_credentials::{AccountId, CredentialRecord, ProviderId};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::accounts::{
    AccountSummary, AccountsService, BeginLinkAccountRequest, BeginLinkAccountResponse, LinkState,
    ListAccountsRequest, ListAccountsResponse, PollLinkAccountRequest, PollLinkAccountResponse,
    ProviderAccounts, RemoveAccountRequest, RemoveAccountResponse, SessionAccount,
    SetAccountLabelRequest, SetAccountLabelResponse, SyncStatus,
};

use crate::linking::{
    record_for_link, removal_allowed, AccountLinker, LinkError, LinkProgress, LinkedAccountStore,
};
use crate::store::{AccountStore, AccountsError};
use crate::sync_status::SyncStatusSource;

/// The metadata key a record's provider-side identifier is read from — a GitHub login, a
/// Cloudflare account id. Shown beside the label; never a credential.
const SUBJECT_METADATA_KEY: &str = "subject";

/// The two ports adding an account needs, wired together.
///
/// Held as one optional field rather than two, because they are only ever useful as a pair: the
/// provider's dance produces a credential, and the store is where it goes. A daemon that wires
/// neither serves the three read-and-curate methods and refuses the two link ones.
struct Linking {
    linker: Arc<dyn AccountLinker>,
    store: Arc<dyn LinkedAccountStore>,
    /// Attempts begun and not yet finished: `link_id` to the session that began it and the
    /// provider it targets. A poll presents only the `link_id`, and a link belongs to the session
    /// that began it.
    attempts: Mutex<HashMap<String, (String, ProviderId)>>,
}

/// Serves `accounts.AccountsService` by reading and curating one [`AccountStore`], and — when the
/// linking half is wired — by adding accounts to it.
pub struct AccountsServiceImpl<S> {
    store: Arc<S>,
    /// `#keyring` 6/9's aggregate sync standing per account. `None` when no sync engine is wired
    /// on this daemon — the common case — in which case every account reports
    /// `SYNC_STATUS_UNSPECIFIED`, exactly as it did before this port existed.
    sync_status: Option<Arc<dyn SyncStatusSource>>,
    linking: Option<Linking>,
}

impl<S> AccountsServiceImpl<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            sync_status: None,
            linking: None,
        }
    }

    /// Wire `#keyring` 6/9's sync standing into `AccountSummary.sync_status`. Optional and
    /// additive — every existing call site that skips this keeps reporting
    /// `SYNC_STATUS_UNSPECIFIED`, unchanged.
    #[must_use]
    pub fn with_sync_status(mut self, source: Arc<dyn SyncStatusSource>) -> Self {
        self.sync_status = Some(source);
        self
    }

    /// Wire the half that adds an account: a provider's authorization dance, and somewhere to put
    /// what it yields.
    ///
    /// Additive rather than a second parameter on [`new`](Self::new), so a daemon that only shows
    /// and curates is unchanged. **Neither port can mint a session**: [`AccountLinker`] has no
    /// method that produces one, and [`LinkedAccountStore`] writes records. That is the boundary
    /// `#keyring` 8/9 is built around, expressed as what the types make unreachable.
    #[must_use]
    pub fn with_linking(
        mut self,
        linker: Arc<dyn AccountLinker>,
        store: Arc<dyn LinkedAccountStore>,
    ) -> Self {
        self.linking = Some(Linking {
            linker,
            store,
            attempts: Mutex::new(HashMap::new()),
        });
        self
    }

    /// The linking half, or the refusal a daemon that never wired one owes the caller.
    ///
    /// A refusal rather than a pretend-empty answer: a person who asked to add an account and was
    /// told nothing happened would try again. **Not a fallback** — nothing is substituted for the
    /// missing ports; the request simply does not happen, and says so.
    fn require_linking(&self) -> Result<&Linking, Status> {
        self.linking.as_ref().ok_or_else(|| {
            Status::failed_precondition("this daemon cannot link accounts: no store is wired")
        })
    }
}

#[async_trait]
impl<S: AccountStore + 'static> AccountsService for AccountsServiceImpl<S> {
    async fn list_accounts(
        &self,
        request: Request<ListAccountsRequest>,
    ) -> Result<Response<ListAccountsResponse>, Status> {
        let request = request.into_inner();
        // `Locked` and `Uninitialized` are the refusals that are answers rather than errors: the
        // vault is closed or does not exist yet, which the screen explains instead of showing an
        // empty list.
        let response = match self.store.list(&request.session_token) {
            Ok(records) => ListAccountsResponse {
                providers: grouped_by_provider(&records, self.sync_status.as_deref()),
                session_account: self.session_account_of(&request.session_token)?.map(
                    |(provider, account)| SessionAccount {
                        provider: provider.as_str().to_string(),
                        account_id: account.as_str().to_string(),
                    },
                ),
                ..ListAccountsResponse::default()
            },
            Err(AccountsError::Locked) => ListAccountsResponse {
                vault_locked: true,
                ..ListAccountsResponse::default()
            },
            Err(AccountsError::Uninitialized) => ListAccountsResponse {
                vault_uninitialized: true,
                ..ListAccountsResponse::default()
            },
            Err(refusal) => return Err(status_for(refusal)),
        };
        Ok(Response::new(response))
    }

    async fn set_account_label(
        &self,
        request: Request<SetAccountLabelRequest>,
    ) -> Result<Response<SetAccountLabelResponse>, Status> {
        let request = request.into_inner();
        let renamed = self
            .store
            .set_label(
                &request.session_token,
                &ProviderId::new(request.provider),
                &AccountId::new(request.account_id),
                &request.label,
            )
            .map_err(status_for)?;
        Ok(Response::new(SetAccountLabelResponse {
            account: Some(summary_of(&renamed, self.sync_status.as_deref())),
        }))
    }

    async fn remove_account(
        &self,
        request: Request<RemoveAccountRequest>,
    ) -> Result<Response<RemoveAccountResponse>, Status> {
        let request = request.into_inner();
        let provider = ProviderId::new(request.provider);
        let account = AccountId::new(request.account_id);
        let session_account = self.session_account_of(&request.session_token)?;
        removal_allowed(
            (&provider, &account),
            session_account.as_ref().map(|(p, a)| (p, a)),
        )
        .map_err(|_| {
            Status::failed_precondition(
                "this is the account your session was established with, and your vault's key \
                 derives from it; sign in with another account before removing it",
            )
        })?;
        self.store
            .remove(&request.session_token, &provider, &account)
            .map_err(status_for)?;
        let remaining = self
            .store
            .list(&request.session_token)
            .map_err(status_for)?;
        Ok(Response::new(RemoveAccountResponse {
            providers: grouped_by_provider(&remaining, self.sync_status.as_deref()),
        }))
    }

    async fn begin_link_account(
        &self,
        request: Request<BeginLinkAccountRequest>,
    ) -> Result<Response<BeginLinkAccountResponse>, Status> {
        let linking = self.require_linking()?;
        let request = request.into_inner();
        let provider = ProviderId::new(request.provider);
        // Only an unknown session stops a link from beginning. A closed vault is reported when the
        // approval arrives, as `LINK_VAULT_LOCKED`, which is where the operator can be told.
        match linking.store.held(&request.session_token, &provider) {
            Ok(_) | Err(LinkError::Locked) => {}
            Err(refusal) => return Err(link_status_for(refusal)),
        }
        let challenge = linking.linker.begin(&provider).map_err(link_status_for)?;
        linking
            .attempts
            .lock()
            .map_err(|_| Status::internal("link attempts are unavailable"))?
            .insert(challenge.link_id.clone(), (request.session_token, provider));
        Ok(Response::new(BeginLinkAccountResponse {
            link_id: challenge.link_id,
            user_code: challenge.user_code,
            verification_uri: challenge.verification_uri,
            expires_in_seconds: saturating_i64(challenge.expires_in_seconds),
            interval_seconds: saturating_i64(challenge.interval_seconds),
        }))
    }

    async fn poll_link_account(
        &self,
        request: Request<PollLinkAccountRequest>,
    ) -> Result<Response<PollLinkAccountResponse>, Status> {
        let linking = self.require_linking()?;
        let request = request.into_inner();
        let provider = {
            let attempts = linking
                .attempts
                .lock()
                .map_err(|_| Status::internal("link attempts are unavailable"))?;
            match attempts.get(&request.link_id) {
                Some((session, provider)) if session == &request.session_token => provider.clone(),
                _ => return Err(link_status_for(LinkError::NoSuchLink)),
            }
        };
        let progress = linking
            .linker
            .poll(&request.link_id)
            .map_err(link_status_for)?;
        let state_only = |state: LinkState| PollLinkAccountResponse {
            state: state as i32,
            ..PollLinkAccountResponse::default()
        };
        let response = match progress {
            LinkProgress::Pending { interval_seconds } => PollLinkAccountResponse {
                state: LinkState::LinkPending as i32,
                interval_seconds: saturating_i64(interval_seconds),
                ..PollLinkAccountResponse::default()
            },
            LinkProgress::Denied => {
                self.finish_attempt(linking, &request.link_id)?;
                state_only(LinkState::LinkDenied)
            }
            LinkProgress::Expired => {
                self.finish_attempt(linking, &request.link_id)?;
                state_only(LinkState::LinkExpired)
            }
            LinkProgress::Approved {
                identity,
                access_token,
            } => {
                self.finish_attempt(linking, &request.link_id)?;
                match store_link(
                    linking,
                    &request.session_token,
                    &provider,
                    &identity,
                    &access_token,
                ) {
                    Ok(record) => PollLinkAccountResponse {
                        state: LinkState::LinkLinked as i32,
                        account: Some(summary_of(&record, self.sync_status.as_deref())),
                        ..PollLinkAccountResponse::default()
                    },
                    Err(LinkError::Locked) => state_only(LinkState::LinkVaultLocked),
                    Err(refusal) => return Err(link_status_for(refusal)),
                }
            }
        };
        Ok(Response::new(response))
    }
}

impl<S> AccountsServiceImpl<S> {
    /// The account the session was established with. `None` when no linking is wired, because
    /// nothing then knows which account that is.
    fn session_account_of(
        &self,
        session_token: &str,
    ) -> Result<Option<(ProviderId, AccountId)>, Status> {
        self.linking.as_ref().map_or(Ok(None), |linking| {
            linking
                .store
                .session_account(session_token)
                .map_err(link_status_for)
        })
    }

    fn finish_attempt(&self, linking: &Linking, link_id: &str) -> Result<(), Status> {
        linking
            .attempts
            .lock()
            .map_err(|_| Status::internal("link attempts are unavailable"))?
            .remove(link_id);
        Ok(())
    }
}

/// Deduplicate against what the vault holds and write the result. Mints nothing.
fn store_link(
    linking: &Linking,
    session_token: &str,
    provider: &ProviderId,
    identity: &crate::linking::LinkedIdentity,
    access_token: &str,
) -> Result<CredentialRecord, LinkError> {
    let held = linking.store.held(session_token, provider)?;
    let linked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| LinkError::Unavailable(error.to_string()))?
        .as_secs();
    let record = record_for_link(&held, provider, identity, access_token, linked_at);
    linking.store.put(session_token, record.clone())?;
    Ok(record)
}

fn saturating_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn link_status_for(refusal: LinkError) -> Status {
    match refusal {
        LinkError::NoSuchSession => {
            Status::unauthenticated("the session token names no signed-in session")
        }
        LinkError::Locked => Status::failed_precondition(
            "your credential vault is locked on this daemon; unlock it with your passphrase",
        ),
        LinkError::NoSuchLink => {
            Status::not_found("no link attempt by that id is in progress; begin a new one")
        }
        LinkError::UnsupportedProvider(provider) => {
            Status::invalid_argument(format!("accounts at {provider} cannot be linked"))
        }
        LinkError::Unavailable(reason) => Status::internal(reason),
    }
}

/// Group records by provider in the order the store returned them — the store already orders by
/// `(provider, account)`, so this never re-sorts.
fn grouped_by_provider(
    records: &[CredentialRecord],
    sync_status: Option<&dyn SyncStatusSource>,
) -> Vec<ProviderAccounts> {
    let mut groups: Vec<ProviderAccounts> = Vec::new();
    for record in records {
        let provider = record.provider.as_str();
        let summary = summary_of(record, sync_status);
        match groups.iter_mut().find(|group| group.provider == provider) {
            Some(group) => group.accounts.push(summary),
            None => groups.push(ProviderAccounts {
                provider: provider.to_string(),
                accounts: vec![summary],
            }),
        }
    }
    groups
}

/// What a person may see of a record. The secret is reduced to whether one is present.
fn summary_of(
    record: &CredentialRecord,
    sync_status: Option<&dyn SyncStatusSource>,
) -> AccountSummary {
    let sync_status = sync_status
        .and_then(|source| source.status_for(&record.provider, &record.account))
        .map_or(SyncStatus::Unspecified, sync_status_proto_of);
    AccountSummary {
        provider: record.provider.as_str().to_string(),
        account_id: record.account.as_str().to_string(),
        label: record.label.clone(),
        subject: record
            .metadata
            .get(SUBJECT_METADATA_KEY)
            .cloned()
            .unwrap_or_default(),
        updated_at: i64::try_from(record.updated_at).unwrap_or(i64::MAX),
        has_secret: !record.secret.expose().is_empty(),
        sync_status: sync_status as i32,
    }
}

/// `#keyring` 6/9's aggregate summary, mapped 1:1 onto the wire enum. `SYNC_STATUS_UNSPECIFIED`
/// is reserved for "nothing has synced it yet" and is never produced here — that case is handled
/// by `summary_of`, before this function ever sees a value.
fn sync_status_proto_of(summary: AccountSyncSummary) -> SyncStatus {
    match summary {
        AccountSyncSummary::Synced => SyncStatus::Synced,
        AccountSyncSummary::Pending => SyncStatus::Pending,
        AccountSyncSummary::Undeliverable => SyncStatus::Undeliverable,
        AccountSyncSummary::Conflict => SyncStatus::Conflict,
        AccountSyncSummary::Refused => SyncStatus::Refused,
    }
}

/// The RPC status a refusal becomes wherever it is an error rather than an answer.
fn status_for(refusal: AccountsError) -> Status {
    match refusal {
        AccountsError::NoSuchSession => {
            Status::unauthenticated("the session token names no signed-in session")
        }
        AccountsError::Locked => Status::failed_precondition(
            "your credential vault is locked on this daemon; \
             unlock it with your passphrase — nothing in it is lost",
        ),
        AccountsError::Uninitialized => Status::failed_precondition(
            "no credential vault exists for you on this daemon yet; \
             choosing a passphrase creates one",
        ),
        AccountsError::NotFound { provider, account } => Status::not_found(format!(
            "no {provider} account {account} is linked, so there is nothing to rename"
        )),
        AccountsError::Unavailable(reason) => Status::internal(reason),
    }
}

/// The registry entry a daemon pushes so a client can address this service by name.
#[must_use]
pub fn build_accounts_entry<S>(service: AccountsServiceImpl<S>) -> tddy_rpc::ServiceEntry
where
    S: AccountStore + 'static,
{
    use tddy_service::proto::accounts::AccountsServiceServer;

    tddy_rpc::ServiceEntry {
        name: "accounts.AccountsService",
        service: Arc::new(AccountsServiceServer::new(service)) as Arc<dyn tddy_rpc::RpcService>,
    }
}
