use crate::activity_hub;
use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::demo_vm::{
    DemoVmService, GetDemoVmStatusRequest, GetDemoVmStatusResponse, StartDemoVmRequest,
    StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse,
};

/// Thin `DemoVmService` adapter over the host's [`DemoVmState`](activity_hub::DemoVmState).
pub struct DemoVmServiceImpl {
    state: activity_hub::DemoVmState,
}

impl DemoVmServiceImpl {
    #[must_use]
    pub fn new(state: activity_hub::DemoVmState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl DemoVmService for DemoVmServiceImpl {
    async fn start_demo_vm(
        &self,
        request: Request<StartDemoVmRequest>,
    ) -> Result<Response<StartDemoVmResponse>, Status> {
        self.state.start_demo_vm_at_coordinate(request).await
    }

    async fn stop_demo_vm(
        &self,
        request: Request<StopDemoVmRequest>,
    ) -> Result<Response<StopDemoVmResponse>, Status> {
        self.state.stop_demo_vm_at_coordinate(request).await
    }

    async fn get_demo_vm_status(
        &self,
        request: Request<GetDemoVmStatusRequest>,
    ) -> Result<Response<GetDemoVmStatusResponse>, Status> {
        self.state.get_demo_vm_status_at_coordinate(request).await
    }
}
