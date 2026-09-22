//! `accounts.AccountsService` over an [`AccountStore`].

use std::sync::Arc;

use async_trait::async_trait;
use tddy_credentials::{AccountId, CredentialRecord, ProviderId};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::accounts::{
    AccountSummary, AccountsService, ListAccountsRequest, ListAccountsResponse, ProviderAccounts,
    RemoveAccountRequest, RemoveAccountResponse, SetAccountLabelRequest, SetAccountLabelResponse,
};

use crate::store::{AccountStore, AccountsError};

/// The metadata key a record's provider-side identifier is read from — a GitHub login, a
/// Cloudflare account id. Shown beside the label; never a credential.
const SUBJECT_METADATA_KEY: &str = "subject";

/// Serves `accounts.AccountsService` by reading and curating one [`AccountStore`].
pub struct AccountsServiceImpl<S> {
    store: Arc<S>,
}

impl<S> AccountsServiceImpl<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl<S: AccountStore + 'static> AccountsService for AccountsServiceImpl<S> {
    async fn list_accounts(
        &self,
        request: Request<ListAccountsRequest>,
    ) -> Result<Response<ListAccountsResponse>, Status> {
        let request = request.into_inner();
        // `Locked` is the one refusal that is an answer rather than an error: the vault exists and
        // this session cannot open it, which the screen explains instead of showing an empty list.
        let response = match self.store.list(&request.session_token) {
            Ok(records) => ListAccountsResponse {
                providers: grouped_by_provider(&records),
                vault_locked: false,
            },
            Err(AccountsError::Locked) => ListAccountsResponse {
                providers: Vec::new(),
                vault_locked: true,
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
            account: Some(summary_of(&renamed)),
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
            providers: grouped_by_provider(&remaining),
        }))
    }
}

/// Group records by provider in the order the store returned them — the store already orders by
/// `(provider, account)`, so this never re-sorts.
fn grouped_by_provider(records: &[CredentialRecord]) -> Vec<ProviderAccounts> {
    let mut groups: Vec<ProviderAccounts> = Vec::new();
    for record in records {
        let provider = record.provider.as_str();
        let summary = summary_of(record);
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
fn summary_of(record: &CredentialRecord) -> AccountSummary {
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
        has_secret: !record.secret.is_empty(),
    }
}

/// The RPC status a refusal becomes wherever it is an error rather than an answer.
fn status_for(refusal: AccountsError) -> Status {
    match refusal {
        AccountsError::NoSuchSession => {
            Status::unauthenticated("the session token names no signed-in session")
        }
        AccountsError::Locked => Status::failed_precondition(
            "the credential store cannot be opened with this session's key; \
             it was sealed under a different login and its accounts must be re-linked",
        ),
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
