//! VmServiceImpl — wires VmManager to the generated VmService RPC trait.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::vm::{
    BuildVmImageProgress, BuildVmImageRequest, DefineVmRequest, DefineVmResponse,
    GetVmStatusRequest, GetVmStatusResponse, ListVmImagesRequest, ListVmImagesResponse,
    ListVmsRequest, ListVmsResponse, RemoveVmRequest, RemoveVmResponse, StartVmRequest,
    StartVmResponse, StopVmRequest, StopVmResponse, VmImageInfo, VmInfo, VmService,
};
use tddy_task::{ChannelKind, TaskRegistry};
use tokio_stream::wrappers::ReceiverStream;

use crate::build::VmBuildTaskBody;
use crate::registry::{VmManager, VmSpec, VmState};
use crate::vm::{PortForward, VmError};

/// Resolver that maps a session token to the authenticated GitHub login.
/// Returns `None` if the token is unknown or expired.
///
/// Defined locally to avoid a circular dependency with `tddy-daemon`
/// (tddy-daemon depends on tddy-vm, so tddy-vm must not depend on tddy-daemon).
pub type SessionUserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub struct VmServiceImpl {
    manager: Arc<VmManager>,
    user_resolver: SessionUserResolver,
    task_registry: TaskRegistry,
}

impl VmServiceImpl {
    pub fn new(
        manager: Arc<VmManager>,
        user_resolver: SessionUserResolver,
        task_registry: TaskRegistry,
    ) -> Self {
        Self {
            manager,
            user_resolver,
            task_registry,
        }
    }

    /// Authenticate a session token. Returns the GitHub login on success,
    /// or `Status::unauthenticated` if the token is invalid or expired.
    fn authenticate(&self, token: &str) -> Result<String, Status> {
        (self.user_resolver)(token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))
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
    type BuildVmImageStream = ReceiverStream<Result<BuildVmImageProgress, Status>>;

    async fn define_vm(
        &self,
        request: Request<DefineVmRequest>,
    ) -> Result<Response<DefineVmResponse>, Status> {
        let req = request.into_inner();
        self.authenticate(&req.session_token)?;
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
        request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        let req = request.into_inner();
        self.authenticate(&req.session_token)?;
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

    async fn list_vm_images(
        &self,
        request: Request<ListVmImagesRequest>,
    ) -> Result<Response<ListVmImagesResponse>, Status> {
        let req = request.into_inner();
        self.authenticate(&req.session_token)?;
        let records = crate::build::list_built_images().await;
        let images = records
            .into_iter()
            .map(|r| VmImageInfo {
                path: r.path,
                name: r.name,
                size_bytes: r.size_bytes,
                modified_unix_ms: r.modified_unix_ms,
            })
            .collect();
        Ok(Response::new(ListVmImagesResponse { images }))
    }

    async fn start_vm(
        &self,
        request: Request<StartVmRequest>,
    ) -> Result<Response<StartVmResponse>, Status> {
        let req = request.into_inner();
        self.authenticate(&req.session_token)?;
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
        self.authenticate(&req.session_token)?;
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
        self.authenticate(&req.session_token)?;
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
        self.authenticate(&req.session_token)?;
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
    ) -> Result<Response<Self::BuildVmImageStream>, Status> {
        let req = request.into_inner();
        // Validate token before spawning the long-running build task.
        self.authenticate(&req.session_token)?;
        let spec = req.buildroot_spec;

        // Channel for structured BuildVmImageProgress events → RPC stream.
        let (progress_tx, progress_rx) = tokio::sync::mpsc::channel(64);

        // Observable build-log channel for TaskService.WatchTask.
        let log_ch = tddy_task::TaskChannel::output_only("0", "build-log", ChannelKind::Combined);

        let body = VmBuildTaskBody {
            buildroot_spec: spec,
            progress_tx,
        };
        self.task_registry
            .spawn(body, "vm_build", "", vec![log_ch])
            .await;

        Ok(Response::new(ReceiverStream::new(progress_rx)))
    }
}
