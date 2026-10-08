//! Family O — `demo_vm.DemoVmService`.

use std::sync::Arc;

use tddy_service::DemoVmServiceServer;

use super::DaemonSessionHost;
use crate::connection_service::demo_vm_service::DemoVmServiceImpl;

impl DaemonSessionHost {
    #[must_use]
    pub fn demo_vm_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_rpc::ServiceEntry {
            name: "demo_vm.DemoVmService",
            service: Arc::new(DemoVmServiceServer::new(DemoVmServiceImpl::new(
                self.demo_vm_service_state(),
            ))) as Arc<dyn tddy_rpc::RpcService>,
        }
    }
}
