//! VmServiceImpl — wires VmManager to the generated VmService RPC trait.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::vm::{
    BuildVmImageRequest, BuildVmImageResponse, DefineVmRequest, DefineVmResponse,
    GetVmStatusRequest, GetVmStatusResponse, ListVmsRequest, ListVmsResponse, RemoveVmRequest,
    RemoveVmResponse, StartVmRequest, StartVmResponse, StopVmRequest, StopVmResponse, VmInfo,
    VmService,
};

use crate::registry::{VmManager, VmSpec, VmState};
use crate::vm::{PortForward, VmError};

pub struct VmServiceImpl {
    manager: Arc<VmManager>,
}

impl VmServiceImpl {
    pub fn new(manager: Arc<VmManager>) -> Self {
        Self { manager }
    }
}

fn vm_err_to_status(e: VmError) -> Status {
    use tddy_rpc::Code;
    match e {
        VmError::NotFound(msg) => Status::not_found(msg),
        VmError::AlreadyExists(msg) => Status {
            code: Code::AlreadyExists,
            message: msg,
        },
        VmError::InvalidState(msg) => Status::failed_precondition(msg),
        other => Status::internal(other.to_string()),
    }
}

fn vm_state_to_proto(state: &VmState) -> i32 {
    match state {
        VmState::Defined => 1,  // VM_STATE_DEFINED
        VmState::Booting => 2,  // VM_STATE_BOOTING
        VmState::Running => 3,  // VM_STATE_RUNNING
        VmState::Stopped => 4,  // VM_STATE_STOPPED
        VmState::Error(_) => 5, // VM_STATE_ERROR
    }
}

#[async_trait]
impl VmService for VmServiceImpl {
    async fn define_vm(
        &self,
        request: Request<DefineVmRequest>,
    ) -> Result<Response<DefineVmResponse>, Status> {
        let req = request.into_inner();
        let proto_spec = req
            .spec
            .ok_or_else(|| Status::invalid_argument("spec is required"))?;
        let spec = VmSpec {
            name: proto_spec.name,
            build_target: if proto_spec.build_target.is_empty() {
                None
            } else {
                Some(proto_spec.build_target)
            },
            image_path: if proto_spec.image_path.is_empty() {
                None
            } else {
                Some(proto_spec.image_path)
            },
            port_forwards: proto_spec
                .port_forwards
                .into_iter()
                .map(|p| PortForward {
                    host_port: p.host_port as u16,
                    guest_port: p.guest_port as u16,
                })
                .collect(),
            ssh_host_port: proto_spec.ssh_host_port as u16,
        };
        self.manager.define(spec).await.map_err(vm_err_to_status)?;
        Ok(Response::new(DefineVmResponse {
            ok: true,
            message: String::new(),
        }))
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        let vms = self.manager.list().await;
        let infos = vms
            .into_iter()
            .map(|(spec, state)| {
                let error_message = if let VmState::Error(ref msg) = state {
                    msg.clone()
                } else {
                    String::new()
                };
                VmInfo {
                    name: spec.name,
                    state: vm_state_to_proto(&state),
                    ssh_host_port: spec.ssh_host_port as u32,
                    share_url: String::new(),
                    error_message,
                }
            })
            .collect();
        Ok(Response::new(ListVmsResponse { vms: infos }))
    }

    async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        let req = request.into_inner();
        self.manager
            .start(&req.name)
            .await
            .map_err(vm_err_to_status)?;
        Ok(Response::new(StartVmResponse {
            state: vm_state_to_proto(&VmState::Running),
            message: String::new(),
        }))
    }

    async fn stop_vm(
        &self,
        request: Request<StopVmRequest>,
    ) -> Result<Response<StopVmResponse>, Status> {
        let req = request.into_inner();
        self.manager
            .stop(&req.name)
            .await
            .map_err(vm_err_to_status)?;
        Ok(Response::new(StopVmResponse {
            ok: true,
            message: String::new(),
        }))
    }

    async fn get_vm_status(
        &self,
        request: Request<GetVmStatusRequest>,
    ) -> Result<Response<GetVmStatusResponse>, Status> {
        let req = request.into_inner();
        let state = self
            .manager
            .status(&req.name)
            .await
            .map_err(vm_err_to_status)?;
        // Look up spec to get ssh_host_port
        let ssh_host_port = self
            .manager
            .list()
            .await
            .into_iter()
            .find(|(spec, _)| spec.name == req.name)
            .map(|(spec, _)| spec.ssh_host_port as u32)
            .unwrap_or(0);
        Ok(Response::new(GetVmStatusResponse {
            state: vm_state_to_proto(&state),
            ssh_host_port,
            share_url: String::new(),
            message: String::new(),
        }))
    }

    async fn remove_vm(
        &self,
        request: Request<RemoveVmRequest>,
    ) -> Result<Response<RemoveVmResponse>, Status> {
        let req = request.into_inner();
        self.manager
            .remove(&req.name)
            .await
            .map_err(vm_err_to_status)?;
        Ok(Response::new(RemoveVmResponse {
            ok: true,
            message: String::new(),
        }))
    }

    async fn build_vm_image(
        &self,
        request: Request<BuildVmImageRequest>,
    ) -> Result<Response<BuildVmImageResponse>, Status> {
        let _req = request.into_inner();
        // TODO: build_vm_image not yet implemented
        Err(Status::unimplemented("build_vm_image not yet implemented"))
    }
}
