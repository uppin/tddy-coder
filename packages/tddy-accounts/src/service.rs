//! `accounts.AccountsService` over an [`AccountStore`].

use std::sync::Arc;

use async_trait::async_trait;
use tddy_credential_sync::AccountSyncSummary;
use tddy_credentials::{AccountId, CredentialRecord, ProviderId};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::accounts::{
    AccountSummary, AccountsService, ListAccountsRequest, ListAccountsResponse, ProviderAccounts,
    RemoveAccountRequest, RemoveAccountResponse, SetAccountLabelRequest, SetAccountLabelResponse,
    SyncStatus,
};

use crate::store::{AccountStore, AccountsError};
use crate::sync_status::SyncStatusSource;

/// The metadata key a record's provider-side identifier is read from — a GitHub login, a
/// Cloudflare account id. Shown beside the label; never a credential.
const SUBJECT_METADATA_KEY: &str = "subject";

/// Serves `accounts.AccountsService` by reading and curating one [`AccountStore`].
pub struct AccountsServiceImpl<S> {
    store: Arc<S>,
    /// `#keyring` 6/9's aggregate sync standing per account. `None` when no sync engine is wired
    /// on this daemon — the common case — in which case every account reports
    /// `SYNC_STATUS_UNSPECIFIED`, exactly as it did before this port existed.
    sync_status: Option<Arc<dyn SyncStatusSource>>,
}

impl<S> AccountsServiceImpl<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            sync_status: None,
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
        self.store
            .remove(
                &request.session_token,
                &ProviderId::new(request.provider),
                &AccountId::new(request.account_id),
            )
            .map_err(status_for)?;
        let remaining = self
            .store
            .list(&request.session_token)
            .map_err(status_for)?;
        Ok(Response::new(RemoveAccountResponse {
            providers: grouped_by_provider(&remaining, self.sync_status.as_deref()),
        }))
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
