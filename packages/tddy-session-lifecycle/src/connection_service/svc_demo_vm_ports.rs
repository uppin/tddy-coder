//! Family O — `demo_vm.DemoVmService` on [`DaemonSessionHost`].

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::demo_vm::{
    DemoVmService, GetDemoVmStatusRequest, GetDemoVmStatusResponse, StartDemoVmRequest,
    StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse,
};
use tddy_service::DemoVmServiceServer;

use super::DaemonSessionHost;

/// Thin `DemoVmService` adapter over a [`DaemonSessionHost`].
pub struct DemoVmServiceImpl {
    host: Arc<DaemonSessionHost>,
}

impl DemoVmServiceImpl {
    #[must_use]
    pub fn new(host: Arc<DaemonSessionHost>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl DemoVmService for DemoVmServiceImpl {
    async fn start_demo_vm(
        &self,
        request: Request<StartDemoVmRequest>,
    ) -> Result<Response<StartDemoVmResponse>, Status> {
        self.host.start_demo_vm_at_coordinate(request).await
    }

    async fn stop_demo_vm(
        &self,
        request: Request<StopDemoVmRequest>,
    ) -> Result<Response<StopDemoVmResponse>, Status> {
        self.host.stop_demo_vm_at_coordinate(request).await
    }

    async fn get_demo_vm_status(
        &self,
        request: Request<GetDemoVmStatusRequest>,
    ) -> Result<Response<GetDemoVmStatusResponse>, Status> {
        self.host.get_demo_vm_status_at_coordinate(request).await
    }
}

impl DaemonSessionHost {
    #[must_use]
    pub fn demo_vm_entry(self: &Arc<Self>) -> tddy_rpc::ServiceEntry {
        tddy_rpc::ServiceEntry {
            name: "demo_vm.DemoVmService",
            service: Arc::new(DemoVmServiceServer::new(DemoVmServiceImpl::new(Arc::clone(self))))
                as Arc<dyn tddy_rpc::RpcService>,
        }
    }
}
