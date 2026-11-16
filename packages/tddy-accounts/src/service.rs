//! `accounts.AccountsService` over an [`AccountStore`].

mod account_answers;
mod link_answers;
mod link_progress;
mod provider_routes;
mod wiring;

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tddy_credentials::{AccountId, ProviderId};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::accounts::{
    AccountsService, BeginLinkAccountRequest, BeginLinkAccountResponse, ConnectCoderAccountRequest,
    ConnectCoderAccountResponse, ConnectProviderAccountRequest, ConnectProviderAccountResponse,
    LinkState, ListAccountsRequest, ListAccountsResponse, ListProviderAccountModelsRequest,
    ListProviderAccountModelsResponse, ListProviderRoutesRequest, ListProviderRoutesResponse,
    PollLinkAccountRequest, PollLinkAccountResponse, RemoveAccountRequest, RemoveAccountResponse,
    SessionAccount, SetAccountLabelRequest, SetAccountLabelResponse,
    SetProviderAccountDefaultsRequest, SetProviderAccountDefaultsResponse,
};

use crate::attempts::{Attempts, PollAdmission};
use crate::linking::{removal_allowed, AccountLinker, LinkError, LinkedAccountStore};
use crate::provider_account::{
    refuse_undrivable, ProviderAccount, ProviderAccountDefaults, ProviderAccountStore,
    ProviderModelProbe, RouteSupport,
};
use crate::service::account_answers::grouped_by_provider;
use crate::service::account_answers::refusal_of_provider_account;
use crate::service::account_answers::requested_account;
use crate::service::account_answers::status_for;
use crate::service::account_answers::summary_of;
use crate::service::link_answers::link_status_for;
use crate::service::link_answers::pending;
use crate::service::link_answers::saturating_i64;
use crate::service::link_answers::state_only;
use crate::service::provider_routes::route_option;
use crate::store::{AccountStore, AccountsError};
use crate::sync_status::SyncStatusSource;

/// A source of the current instant. See [`AccountsServiceImpl::with_clock`].
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// The two ports adding an account needs, wired together.
///
/// Held as one optional field rather than two, because they are only ever useful as a pair: the
/// provider's dance produces a credential, and the store is where it goes. A daemon that wires
/// neither serves the three read-and-curate methods and refuses the two link ones.
struct Linking {
    linker: Arc<dyn AccountLinker>,
    store: Arc<dyn LinkedAccountStore>,
    /// Attempts begun and not yet finished, and when each may next be polled.
    attempts: Attempts,
}

/// The two ports connecting a model-provider account needs: somewhere to keep it, and a way to ask
/// its endpoint whether the key works. Wired together for the reason [`Linking`] is.
struct ProviderAccountPorts {
    store: Arc<dyn ProviderAccountStore>,
    probe: Arc<dyn ProviderModelProbe>,
}

/// Serves `accounts.AccountsService` by reading and curating one [`AccountStore`], and — when the
/// linking half is wired — by adding accounts to it.
pub struct AccountsServiceImpl<S> {
    store: Arc<S>,
    /// Where "now" comes from for attempt deadlines and poll pacing. [`Instant::now`] unless a test
    /// substitutes its own.
    clock: Clock,
    /// `#keyring` 6/9's aggregate sync standing per account. `None` when no sync engine is wired
    /// on this daemon — the common case — in which case every account reports
    /// `SYNC_STATUS_UNSPECIFIED`, exactly as it did before this port existed.
    sync_status: Option<Arc<dyn SyncStatusSource>>,
    linking: Option<Linking>,
    provider_accounts: Option<ProviderAccountPorts>,
    route_support: Option<Arc<dyn RouteSupport>>,
    defaults: Option<Arc<dyn ProviderAccountDefaults>>,
    /// The Coder half — see `coder_account`, which owns its builder and its RPC's answer.
    pub(crate) coder: Option<Arc<crate::coder_account::CoderAccounts>>,
}

impl<S> AccountsServiceImpl<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            clock: Arc::new(Instant::now),
            sync_status: None,
            linking: None,
            provider_accounts: None,
            route_support: None,
            defaults: None,
            coder: None,
        }
    }

    /// The provider-account half, or the refusal a daemon that never wired one owes the caller.
    fn require_provider_accounts(&self) -> Result<&ProviderAccountPorts, Status> {
        self.provider_accounts.as_ref().ok_or_else(|| {
            Status::failed_precondition(
                "this daemon cannot connect provider accounts: no store is wired",
            )
        })
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

impl<S> AccountsServiceImpl<S> {
    /// The harness × account matrix, or the refusal a route RPC answers without one.
    fn route_matrix(&self) -> Result<&dyn RouteSupport, Status> {
        self.route_support.as_deref().ok_or_else(|| {
            Status::failed_precondition(
                "this daemon cannot tell which harness runs on which provider account: \
                 no route matrix is wired",
            )
        })
    }
}

#[async_trait]
impl<S: AccountStore + 'static> AccountsService for AccountsServiceImpl<S> {
    async fn connect_provider_account(
        &self,
        request: Request<ConnectProviderAccountRequest>,
    ) -> Result<Response<ConnectProviderAccountResponse>, Status> {
        let request = request.into_inner();
        let ports = self.require_provider_accounts()?;
        let session_token = request.session_token.clone();
        let account = requested_account(request).map_err(refusal_of_provider_account)?;

        // The vault is consulted before the network: a locked vault refuses without the key ever
        // leaving this daemon.
        self.store.list(&session_token).map_err(status_for)?;
        ports
            .probe
            .list_models(&account.endpoints, &account.api_key)
            .await
            .map_err(|reason| {
                Status::invalid_argument(format!(
                    "the {} endpoint did not accept this key: {reason}",
                    account.preset.label
                ))
            })?;

        let record = ports
            .store
            .put_provider_account(&session_token, &account)
            .map_err(status_for)?;
        Ok(Response::new(ConnectProviderAccountResponse {
            account: Some(summary_of(&record, self.sync_status.as_deref())),
        }))
    }

    async fn connect_coder_account(
        &self,
        request: Request<ConnectCoderAccountRequest>,
    ) -> Result<Response<ConnectCoderAccountResponse>, Status> {
        let record = crate::coder_account::connect_coder_account(
            self.coder.as_deref(),
            request.into_inner(),
        )
        .await?;
        Ok(Response::new(ConnectCoderAccountResponse {
            account: Some(summary_of(&record, self.sync_status.as_deref())),
        }))
    }

    async fn list_provider_routes(
        &self,
        request: Request<ListProviderRoutesRequest>,
    ) -> Result<Response<ListProviderRoutesResponse>, Status> {
        let request = request.into_inner();
        let matrix = self.route_matrix()?;
        let records = self
            .store
            .list(&request.session_token)
            .map_err(status_for)?;
        let harness_reason = matrix.harness_routable(&request.harness).err();
        let options = records
            .iter()
            .filter_map(ProviderAccount::from_record)
            .map(|account| {
                route_option(matrix, &request.harness, harness_reason.as_deref(), account)
            })
            .collect();
        Ok(Response::new(ListProviderRoutesResponse {
            options,
            harness_routable: harness_reason.is_none(),
            harness_reason: harness_reason.unwrap_or_default(),
        }))
    }

    async fn set_provider_account_defaults(
        &self,
        request: Request<SetProviderAccountDefaultsRequest>,
    ) -> Result<Response<SetProviderAccountDefaultsResponse>, Status> {
        let request = request.into_inner();
        let defaults = self.defaults.as_deref().ok_or_else(|| {
            Status::failed_precondition("this daemon keeps no per-harness provider defaults")
        })?;
        let matrix = self.route_matrix()?;
        let wanted = AccountId::new(&request.account_id);
        let account = self
            .store
            .list(&request.session_token)
            .map_err(status_for)?
            .iter()
            .filter(|record| record.account == wanted)
            .find_map(ProviderAccount::from_record)
            .ok_or_else(|| {
                Status::not_found(format!(
                    "no provider account {} is connected",
                    request.account_id
                ))
            })?;
        refuse_undrivable(matrix, &account, &request.harnesses)
            .map_err(Status::invalid_argument)?;
        let changed = defaults
            .set_provider_account_defaults(&request.session_token, &wanted, &request.harnesses)
            .map_err(status_for)?;
        Ok(Response::new(SetProviderAccountDefaultsResponse {
            changed: changed
                .iter()
                .map(|record| summary_of(record, self.sync_status.as_deref()))
                .collect(),
        }))
    }

    async fn list_provider_account_models(
        &self,
        request: Request<ListProviderAccountModelsRequest>,
    ) -> Result<Response<ListProviderAccountModelsResponse>, Status> {
        let request = request.into_inner();
        let ports = self.require_provider_accounts()?;
        let account = ports
            .store
            .provider_account(&request.session_token, &AccountId::new(&request.account_id))
            .map_err(|refusal| match refusal {
                AccountsError::NotFound { .. } => Status::not_found(format!(
                    "no provider account {} is connected",
                    request.account_id
                )),
                other => status_for(other),
            })?;
        let model_ids = ports
            .probe
            .list_models(&account.endpoints, &account.api_key)
            .await
            .map_err(|reason| {
                Status::unavailable(format!(
                    "the {} endpoint did not list its models: {reason}",
                    account.preset.label
                ))
            })?;
        Ok(Response::new(ListProviderAccountModelsResponse {
            model_ids,
        }))
    }

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
            .register(&challenge, request.session_token, provider, (self.clock)())
            .map_err(link_status_for)?;
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
        let admission = linking
            .attempts
            .admit_poll(&request.link_id, &request.session_token, (self.clock)())
            .map_err(link_status_for)?;
        let provider = match admission {
            PollAdmission::Go { provider } => provider,
            PollAdmission::TooSoon { interval_seconds } => {
                return Ok(Response::new(pending(interval_seconds)));
            }
            PollAdmission::Expired => {
                return Ok(Response::new(state_only(LinkState::LinkExpired)));
            }
        };
        let progress = linking
            .linker
            .poll(&request.link_id)
            .map_err(link_status_for)?;
        let response = self.answer_for(linking, &request, &provider, progress)?;
        Ok(Response::new(response))
    }
}

/// The registry entry serving `service` by name; the caller may keep its own handle on the service.
#[must_use]
pub fn build_accounts_entry<S>(service: Arc<AccountsServiceImpl<S>>) -> tddy_rpc::ServiceEntry
where
    S: AccountStore + 'static,
{
    use tddy_service::proto::accounts::AccountsServiceServer;

    tddy_rpc::ServiceEntry {
        name: "accounts.AccountsService",
        service: Arc::new(AccountsServiceServer::from_arc(service)),
    }
}
