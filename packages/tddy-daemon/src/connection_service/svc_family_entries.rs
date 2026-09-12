//! RpcService entries for `#unbundle` node 8 — families A, L and P.

use std::sync::Arc;

use tddy_rpc::RpcService;
use tddy_service::proto::catalog::CatalogServiceServer;
use tddy_service::proto::exec_tools::ExecToolServiceServer;
use tddy_service::proto::pr_stack::PrStackServiceServer;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    #[must_use]
    pub fn catalog_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_discovery::build_catalog_entry(self.as_ref().clone())
    }

    #[must_use]
    pub fn exec_tool_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_tool_engine::build_exec_tool_entry(self.as_ref().clone())
    }

    #[must_use]
    pub fn pr_stack_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_rpc::ServiceEntry {
            name: tddy_workflow_recipes::PR_STACK_SERVICE,
            service: Arc::new(PrStackServiceServer::new(self.as_ref().clone()))
                as Arc<dyn RpcService>,
        }
    }
}
