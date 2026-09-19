//! `accounts.AccountsService` over an [`AccountStore`].

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::accounts::{
    AccountsService, ListAccountsRequest, ListAccountsResponse, RemoveAccountRequest,
    RemoveAccountResponse, SetAccountLabelRequest, SetAccountLabelResponse,
};

use crate::store::AccountStore;

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
        _request: Request<ListAccountsRequest>,
    ) -> Result<Response<ListAccountsResponse>, Status> {
        let _ = &self.store;
        todo!("(#keyring 4/9): group the session's records by provider, with Locked as a field")
    }

    async fn set_account_label(
        &self,
        _request: Request<SetAccountLabelRequest>,
    ) -> Result<Response<SetAccountLabelResponse>, Status> {
        let _ = &self.store;
        todo!("(#keyring 4/9): rename one record, leaving its account id untouched")
    }

    async fn remove_account(
        &self,
        _request: Request<RemoveAccountRequest>,
    ) -> Result<Response<RemoveAccountResponse>, Status> {
        let _ = &self.store;
        todo!("(#keyring 4/9): forget one record and answer with what remains")
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
