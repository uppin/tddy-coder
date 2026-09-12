//! RpcService entries for `#unbundle` node 8 — families A, L and P.

use std::sync::Arc;

use super::DaemonSessionHost;
use crate::pr_stack_rpc::{build_pr_stack_entry, PrStackServiceImpl};
use tddy_discovery::CatalogServiceImpl;
use tddy_tool_engine::ExecToolServiceImpl;

impl DaemonSessionHost {
    #[must_use]
    pub fn catalog_rpc_service(self: &Arc<Self>) -> CatalogServiceImpl<DaemonSessionHost> {
        CatalogServiceImpl::new(Arc::clone(self))
    }

    #[must_use]
    pub fn exec_tool_rpc_service(self: &Arc<Self>) -> ExecToolServiceImpl<DaemonSessionHost> {
        ExecToolServiceImpl::new(Arc::clone(self))
    }

    #[must_use]
    pub fn pr_stack_rpc_service(self: &Arc<Self>) -> PrStackServiceImpl<DaemonSessionHost> {
        PrStackServiceImpl::new(Arc::clone(self))
    }

    #[must_use]
    pub fn catalog_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_discovery::build_catalog_entry(self.catalog_rpc_service())
    }

    #[must_use]
    pub fn exec_tool_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_tool_engine::build_exec_tool_entry(self.exec_tool_rpc_service())
    }

    #[must_use]
    pub fn pr_stack_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        build_pr_stack_entry(self.pr_stack_rpc_service())
    }
}
