//! The families this crate serves, bundled for the composition root — and handed back down to the
//! session host through its [`DaemonRpcFamilies`] port.
//!
//! **Construction order is the contract.** Every handler clones the host's `Arc`s when it is built,
//! so [`RpcHandlers::install`] must run after the host's last `with_*`: a `with_*` applied later would
//! replace a value these handlers still hold. The host then keeps a clone of the bundle, which holds
//! handlers and never the host, so there is no `Arc` cycle to leak it.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use tddy_discovery::CatalogServiceImpl;
use tddy_projects::ProjectServiceImpl;
use tddy_rpc::{ServiceEntry, Status};
use tddy_service::proto::accounts::ProviderRouteOption;
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::{DaemonRpcFamilies, PrStackHandler, PrStackServiceImpl};
use tddy_tool_engine::ExecToolServiceImpl;

use crate::{CatalogRpcHandler, ExecToolRpcHandler, PrStackRpcHandler, ProjectRpcHandler};

/// What `DescribeSessionStartOptions` reads from a host's credential vaults (`#start-parity` 5/9):
/// the provider accounts that can run a harness, and the models of one account. Both are answered
/// from the caller's own vault, by the `session_token` the caller holds and no other.
///
/// Attached by [`RpcHandlers::attach_provider_accounts`] when the host has credential vaults. A host
/// without them never attaches one, and the tool says so rather than offering no accounts silently.
#[async_trait]
pub trait ProviderAccountsPort: Send + Sync {
    /// The provider accounts `harness` can and cannot run on, as `ListProviderRoutes` answers them.
    async fn provider_routes(
        &self,
        session_token: &str,
        harness: &str,
    ) -> Result<Vec<ProviderRouteOption>, Status>;

    /// The model ids one provider account serves, as `ListProviderAccountModels` answers them.
    async fn provider_account_models(
        &self,
        session_token: &str,
        account_id: &str,
    ) -> Result<Vec<String>, Status>;
}

/// Every family served from this crate, each built from the same host.
///
/// `Clone` shares the handlers: a clone serves the very `Arc`s the original does.
#[derive(Clone)]
pub struct RpcHandlers {
    project: Arc<ProjectRpcHandler>,
    catalog: Arc<CatalogRpcHandler>,
    exec_tool: Arc<ExecToolRpcHandler>,
    pr_stack: Arc<PrStackRpcHandler>,
    /// The launch snapshot the exec-tool handler late-binds to, set by
    /// [`DaemonRpcFamilies::attach_session_start`] when the host is given these families.
    session_launch: Arc<OnceLock<LaunchSessions>>,
    /// The provider accounts the exec-tool handler reads, set by
    /// [`RpcHandlers::attach_provider_accounts`].
    provider_accounts: Arc<OnceLock<Arc<dyn ProviderAccountsPort>>>,
}

impl RpcHandlers {
    /// Build every family's handler from `host`'s state.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        let exec_tool = Arc::new(ExecToolRpcHandler::from_host(host));
        let session_launch = exec_tool.session_launch_cell();
        let provider_accounts = exec_tool.provider_accounts_cell();
        Self {
            project: Arc::new(ProjectRpcHandler::from_host(host)),
            catalog: Arc::new(CatalogRpcHandler::from_host(host)),
            exec_tool,
            pr_stack: Arc::new(PrStackRpcHandler::from_host(host)),
            session_launch,
            provider_accounts,
        }
    }

    /// Attach the provider accounts `DescribeSessionStartOptions` answers from. The composition root
    /// calls this once, on a host with credential vaults, with the accounts service that host serves.
    /// A second attach keeps the first.
    pub fn attach_provider_accounts(&self, port: Arc<dyn ProviderAccountsPort>) {
        if self.provider_accounts.set(port).is_err() {
            log::error!("the provider accounts were attached twice; keeping the first attachment");
        }
    }

    /// Build the handlers from `host` and install them on it as its [`DaemonRpcFamilies`] — the
    /// composition root's last step before the host goes behind its `Arc`. Returns both, so the
    /// root serves the same handlers on its own transports.
    #[must_use]
    pub fn install(host: DaemonSessionHost) -> (DaemonSessionHost, Self) {
        let handlers = Self::from_host(&host);
        let host = host.with_rpc_families(Arc::new(handlers.clone()));
        (host, handlers)
    }

    /// `project.ProjectService`, answered by the shared [`ProjectRpcHandler`].
    #[must_use]
    pub fn project_service(&self) -> ProjectServiceImpl<ProjectRpcHandler> {
        ProjectServiceImpl::new(Arc::clone(&self.project))
    }

    /// `catalog.CatalogService`, answered by the shared [`CatalogRpcHandler`].
    #[must_use]
    pub fn catalog_service(&self) -> CatalogServiceImpl<CatalogRpcHandler> {
        CatalogServiceImpl::new(Arc::clone(&self.catalog))
    }

    /// `exec_tools.ExecToolService`, answered by the shared [`ExecToolRpcHandler`].
    #[must_use]
    pub fn exec_tool_service(&self) -> ExecToolServiceImpl<ExecToolRpcHandler> {
        ExecToolServiceImpl::new(Arc::clone(&self.exec_tool))
    }

    /// `pr_stack.PrStackService`, answered by the shared [`PrStackRpcHandler`].
    #[must_use]
    pub fn pr_stack_service(&self) -> PrStackServiceImpl<PrStackRpcHandler> {
        PrStackServiceImpl::new(Arc::clone(&self.pr_stack))
    }

    /// The transport entries of every family served from this crate.
    #[must_use]
    pub fn entries(&self) -> Vec<ServiceEntry> {
        vec![
            tddy_discovery::build_catalog_entry(self.catalog_service()),
            tddy_tool_engine::build_exec_tool_entry(self.exec_tool_service()),
            tddy_projects::build_project_entry(self.project_service()),
            tddy_session_lifecycle::build_pr_stack_entry(self.pr_stack_service()),
        ]
    }
}

impl DaemonRpcFamilies for RpcHandlers {
    fn pr_stack_handler(&self) -> Arc<dyn PrStackHandler> {
        Arc::clone(&self.pr_stack) as Arc<dyn PrStackHandler>
    }

    fn service_entries(&self) -> Vec<ServiceEntry> {
        self.entries()
    }

    /// The project-home session start and list read the host through this snapshot. A second
    /// attach keeps the first, which is the one the families were wired to.
    fn attach_session_start(&self, launch: LaunchSessions) {
        if self.session_launch.set(launch).is_err() {
            log::error!("the session launch was attached twice; keeping the first attachment");
        }
    }
}
