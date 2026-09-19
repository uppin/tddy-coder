//! `accounts.AccountsService` over an [`AccountStore`].

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::accounts::{
    AccountsService, BeginLinkAccountRequest, BeginLinkAccountResponse, ListAccountsRequest,
    ListAccountsResponse, PollLinkAccountRequest, PollLinkAccountResponse, RemoveAccountRequest,
    RemoveAccountResponse, SetAccountLabelRequest, SetAccountLabelResponse,
};

use crate::linking::{AccountLinker, LinkedAccountStore};
use crate::store::AccountStore;

/// The two ports adding an account needs, wired together.
///
/// Held as one optional field rather than two, because they are only ever useful as a pair: the
/// provider's dance produces a credential, and the store is where it goes. A daemon that wires
/// neither serves the three read-and-curate methods and refuses the two link ones.
struct Linking {
    linker: Arc<dyn AccountLinker>,
    store: Arc<dyn LinkedAccountStore>,
}

/// Serves `accounts.AccountsService` by reading and curating one [`AccountStore`], and — when the
/// linking half is wired — by adding accounts to it.
pub struct AccountsServiceImpl<S> {
    store: Arc<S>,
    linking: Option<Linking>,
}

impl<S> AccountsServiceImpl<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            linking: None,
        }
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
        self.linking = Some(Linking { linker, store });
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

    async fn begin_link_account(
        &self,
        _request: Request<BeginLinkAccountRequest>,
    ) -> Result<Response<BeginLinkAccountResponse>, Status> {
        let linking = self.require_linking()?;
        let _ = (&linking.linker, &linking.store);
        todo!("TODO(keyring 8/9): begin the provider's dance; carry the operator's code through")
    }

    async fn poll_link_account(
        &self,
        _request: Request<PollLinkAccountRequest>,
    ) -> Result<Response<PollLinkAccountResponse>, Status> {
        let linking = self.require_linking()?;
        let _ = (&linking.linker, &linking.store);
        todo!("TODO(keyring 8/9): one poll; on approval store the record and return the summary")
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
