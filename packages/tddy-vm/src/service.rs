//! VmServiceImpl — stub implementation of the generated VmService trait.
//! All methods are unimplemented stubs (RED phase). Fill in during Task 3+.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::vm::{
    BuildVmImageRequest, BuildVmImageResponse, DefineVmRequest, DefineVmResponse,
    GetVmStatusRequest, GetVmStatusResponse, ListVmsRequest, ListVmsResponse, RemoveVmRequest,
    RemoveVmResponse, StartVmRequest, StartVmResponse, StopVmRequest, StopVmResponse, VmService,
};

use crate::registry::VmManager;

/// Stub implementation of the generated `VmService` trait.
/// Holds a reference to a `VmManager` for future use.
pub struct VmServiceImpl {
    #[allow(dead_code)]
    manager: Arc<VmManager>,
}

impl VmServiceImpl {
    pub fn new(manager: Arc<VmManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl VmService for VmServiceImpl {
    async fn build_vm_image(
        &self,
        _request: Request<BuildVmImageRequest>,
    ) -> Result<Response<BuildVmImageResponse>, Status> {
        unimplemented!()
    }

    async fn define_vm(
        &self,
        _request: Request<DefineVmRequest>,
    ) -> Result<Response<DefineVmResponse>, Status> {
        unimplemented!()
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        unimplemented!()
    }

    async fn start_vm(
        &self,
        _request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        unimplemented!()
    }

    async fn stop_vm(
        &self,
        _request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        unimplemented!()
    }

    async fn get_vm_status(
        &self,
        _request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        unimplemented!()
    }

    async fn remove_vm(
        &self,
        _request: Request<RemoveVmRequest>,
    ) -> Result<Response<RemoveVmResponse>, Status> {
        unimplemented!()
    }
}
