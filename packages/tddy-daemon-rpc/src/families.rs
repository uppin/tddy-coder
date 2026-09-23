//! The families this crate serves, bundled for the composition root — and handed back down to the
//! session host through its [`DaemonRpcFamilies`] port.
//!
//! **Construction order is the contract.** Every handler clones the host's `Arc`s when it is built,
//! so [`RpcHandlers::install`] must run after the host's last `with_*`: a `with_*` applied later would
//! replace a value these handlers still hold. The host then keeps a clone of the bundle, which holds
//! handlers and never the host, so there is no `Arc` cycle to leak it.

use std::sync::Arc;

use tddy_discovery::CatalogServiceImpl;
use tddy_projects::ProjectServiceImpl;
use tddy_rpc::ServiceEntry;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::{DaemonRpcFamilies, PrStackHandler};
use tddy_tool_engine::ExecToolServiceImpl;

use crate::{CatalogRpcHandler, ExecToolRpcHandler, ProjectRpcHandler};

/// Every family served from this crate, each built from the same host.
///
/// `Clone` shares the handlers: a clone serves the very `Arc`s the original does.
#[derive(Clone)]
pub struct RpcHandlers {
    project: Arc<ProjectRpcHandler>,
    catalog: Arc<CatalogRpcHandler>,
    exec_tool: Arc<ExecToolRpcHandler>,
}

impl RpcHandlers {
    /// Build every family's handler from `host`'s state.
    #[must_use]
    pub fn from_host(host: &DaemonSessionHost) -> Self {
        Self {
            project: Arc::new(ProjectRpcHandler::from_host(host)),
            catalog: Arc::new(CatalogRpcHandler::from_host(host)),
            exec_tool: Arc::new(ExecToolRpcHandler::from_host(host)),
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

    /// The transport entries of every family served from this crate.
    #[must_use]
    pub fn entries(&self) -> Vec<ServiceEntry> {
        vec![
            tddy_discovery::build_catalog_entry(self.catalog_service()),
            tddy_tool_engine::build_exec_tool_entry(self.exec_tool_service()),
            tddy_projects::build_project_entry(self.project_service()),
        ]
    }
}

impl DaemonRpcFamilies for RpcHandlers {
    fn pr_stack_handler(&self) -> Arc<dyn PrStackHandler> {
        // TODO(#carve 11): return the `PrStackRpcHandler` once the PR-stack family moves here. Until
        // then session start's stack paths still call the host's own `PrStackHandler`, so nothing
        // routes through this.
        todo!("RpcHandlers::pr_stack_handler: the PR-stack family has not moved to tddy-daemon-rpc")
    }

    fn service_entries(&self) -> Vec<ServiceEntry> {
        self.entries()
    }
}
