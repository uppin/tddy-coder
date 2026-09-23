//! RpcService entries for `#unbundle` node 8 — family P. Families A (the catalogue) and L (the exec
//! tools) are served by `tddy-daemon-rpc`'s `CatalogRpcHandler` and `ExecToolRpcHandler`.

use std::sync::Arc;

use super::DaemonSessionHost;
use crate::pr_stack_rpc::{build_pr_stack_entry, PrStackServiceImpl};

impl DaemonSessionHost {
    #[must_use]
    pub fn pr_stack_rpc_service(self: &Arc<Self>) -> PrStackServiceImpl<DaemonSessionHost> {
        PrStackServiceImpl::new(Arc::clone(self))
    }

    #[must_use]
    pub fn pr_stack_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        build_pr_stack_entry(self.pr_stack_rpc_service())
    }
}
