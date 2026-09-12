// The parent module's own imports, carried in: this block was the whole `impl
// ConnectionServiceTrait` and reaches the same traits and helpers it always did. Unused
// entries are pruned below by the compiler's own spans.
// `encode_to_vec` is a `prost::Message` method; the trait is imported anonymously because
// only its methods are used.
use tddy_service::proto::connection::start_session_event::Event as StartSessionEventKind;
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
use tddy_service::proto::connection::SessionEntry as ProtoSessionEntry;
use tddy_service::proto::connection::{
    DemoVmState, GetDemoVmStatusRequest, GetDemoVmStatusResponse, StartDemoVmRequest,
    StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse,
};
use tddy_service::proto::connection::{ExecuteToolRequest, ProjectEntry as ProtoProjectEntry};
use tddy_service::proto::connection::{
    GetWorktreeSnapshotRequest, GetWorktreeSnapshotResponse, MintLocalTokenRequest,
    MintLocalTokenResponse, StartSessionEvent,
};

use crate::{
    connection_service::{activity_hub, hooks_and_urls, service_util},
    project_storage, session_deletion, session_list_enrichment, session_reader,
};
use tddy_spawn::{spawn_worker, spawner};

use tddy_service::proto::connection::ListProjectBranchesResponse;

use tddy_service::proto::connection::ListProjectBranchesRequest;

use tddy_service::proto::connection::DeleteSessionResponse;

use tddy_service::proto::connection::DeleteSessionRequest;

use tddy_service::proto::connection::SignalSessionResponse;

use tddy_service::proto::connection::SignalSessionRequest;

use tddy_spawn::spawner::SpawnOptions;

use tddy_service::proto::connection::ResumeSessionResponse;

use tddy_service::proto::connection::ResumeSessionRequest;

use tddy_core::read_session_metadata;

use tddy_core::session_lifecycle::unified_session_dir_path;

use tddy_core::session_lifecycle::validate_session_id_segment;

use tddy_service::proto::connection::ConnectSessionResponse;

use tddy_service::proto::connection::ConnectSessionRequest;

use super::AttachmentProgressSink;

use tddy_service::proto::connection::StartSessionResponse;

use tddy_service::proto::connection::StartSessionRequest;

use tddy_service::proto::connection::SetProjectDefaultBranchResponse;

use tddy_service::proto::connection::SetProjectDefaultBranchRequest;

use crate::livekit_peer_discovery::PeerRoute;

use tddy_service::proto::connection::AddProjectToHostResponse;

use tddy_service::proto::connection::AddProjectToHostRequest;

use crate::project_storage::ProjectData;

use crate::user_sessions_path::repos_base_for_user;

use crate::user_sessions_path::project_path_under_home_from_user_relative;

use tddy_service::proto::connection::CreateProjectResponse;

use tddy_service::proto::connection::CreateProjectRequest;

use super::merge_listed_projects_with_peers;

use crate::user_sessions_path::projects_path_for_user;

use tddy_service::proto::connection::ListProjectsResponse;

use tddy_service::proto::connection::ListProjectsRequest;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_service::proto::connection::ListSessionsResponse;

use tddy_service::proto::connection::ListSessionsRequest;

use std::sync::Arc;

use uuid::Uuid;

use super::MpscResultStream;

use crate::livekit_peer_discovery::local_instance_id_for_config;

use std::path::PathBuf;

use std::path::Path;

use tddy_rpc::Status;

use tddy_rpc::Response;

use tddy_rpc::Request;

use super::ConnectionServiceImpl;

/// The coordinate a forwarded call from *this* service is addressed at on the peer. A forward has
/// to land on the same method of the same service there, and the four routed unaries below are the
/// ones this service still declares.
const CONNECTION_SERVICE: &str = "connection.ConnectionService";

#[async_trait::async_trait]
impl ConnectionServiceTrait for ConnectionServiceImpl {
    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.list_sessions_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn list_projects(
        &self,
        request: Request<ListProjectsRequest>,
    ) -> Result<Response<ListProjectsResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.list_projects_at_project_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn create_project(
        &self,
        request: Request<CreateProjectRequest>,
    ) -> Result<Response<CreateProjectResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.create_project_at_project_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn add_project_to_host(
        &self,
        request: Request<AddProjectToHostRequest>,
    ) -> Result<Response<AddProjectToHostResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.add_project_to_host_at_project_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn set_project_default_branch(
        &self,
        request: Request<SetProjectDefaultBranchRequest>,
    ) -> Result<Response<SetProjectDefaultBranchResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.set_project_default_branch_at_project_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<StartSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.start_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn connect_session(
        &self,
        request: Request<ConnectSessionRequest>,
    ) -> Result<Response<ConnectSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.connect_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn resume_session(
        &self,
        request: Request<ResumeSessionRequest>,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.resume_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn signal_session(
        &self,
        request: Request<SignalSessionRequest>,
    ) -> Result<Response<SignalSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.signal_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.delete_session_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn list_project_branches(
        &self,
        request: Request<ListProjectBranchesRequest>,
    ) -> Result<Response<ListProjectBranchesResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.list_project_branches_at_project_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn start_demo_vm(
        &self,
        request: Request<StartDemoVmRequest>,
    ) -> Result<Response<StartDemoVmResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        let sessions_base =
            crate::user_sessions_path::sessions_base_for_user(os_user, Some(&self.tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve sessions path"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        let session_dir = unified_session_dir_path(&sessions_base, &req.session_id);

        // Read demo-plan.md from the session directory.
        let demo_plan = tddy_workflow_recipes::writer::read_demo_plan_file(&session_dir)
            .map_err(|e| Status::not_found(format!("demo-plan.md not found: {e}")))?;

        let qcow2_path = demo_plan
            .build_target
            .ok_or_else(|| Status::failed_precondition("demo-plan.md has no build_target"))?;
        // ssh_host_port defaults to 2222; the first hostfwd entry is the app port, not SSH.
        let ssh_host_port: u16 = 2222;
        let config = tddy_vm::VmConfig {
            qcow2_path,
            extra_hostfwd: demo_plan
                .hostfwd
                .iter()
                .map(|p| tddy_vm::PortForward {
                    host_port: p.host_port,
                    guest_port: p.guest_port,
                })
                .collect(),
            ssh_host_port,
            // Pinned to what the launcher did before any of this was configurable, because
            // nothing here knows the demo image's architecture — `demo-plan.md` names a
            // build target, and `tddy-build-qemu` produces x86_64 images. Deriving these
            // from the *host* would run an aarch64 emulator against an x86_64 image on an
            // Apple Silicon machine, and `virt` has no BIOS for it to fall back to.
            // TCG likewise matches the previous behaviour and avoids making the demo path
            // newly dependent on the daemon user's access to /dev/kvm.
            arch: tddy_vm::VmArch::X86_64,
            accel: tddy_vm::VmAccel::Tcg,
            // The resources the launcher hard-coded before they were configurable.
            memory: "512M".to_string(),
            cpus: 1,
            // The demo images boot through their own BIOS; they carry no cloud-init seed
            // and share nothing from the host.
            firmware: None,
            login: tddy_vm::VmLogin {
                username: "root".to_string(),
                private_key_path: None,
            },
            seed_iso: None,
            nine_p_shares: vec![],
        };

        // Reject if already booting/running for this session.
        {
            let state = self.demo_vm_state.lock().await;
            if let Some(h) = state.get(&req.session_id) {
                let (state_enum, msg) = match h {
                    activity_hub::DemoVmHandle::Booting => {
                        (DemoVmState::Booting, "already booting")
                    }
                    activity_hub::DemoVmHandle::Running { .. } => {
                        (DemoVmState::Running, "VM already running")
                    }
                    activity_hub::DemoVmHandle::Error(_) => {
                        // Allow retry after error.
                        return Ok(Response::new(StartDemoVmResponse {
                            state: DemoVmState::Booting as i32,
                            message: "retrying after previous error".to_string(),
                        }));
                    }
                };
                return Ok(Response::new(StartDemoVmResponse {
                    state: state_enum as i32,
                    message: msg.to_string(),
                }));
            }
        }

        // Mark as booting and spawn the boot task.
        {
            let mut state = self.demo_vm_state.lock().await;
            state.insert(req.session_id.clone(), activity_hub::DemoVmHandle::Booting);
        }

        // Build the share URL from the first app hostfwd entry (not the SSH port itself).
        let share_url = config
            .extra_hostfwd
            .first()
            .map(|p| format!("http://localhost:{}", p.host_port))
            .unwrap_or_default();

        let state_ref = Arc::clone(&self.demo_vm_state);
        let session_id = req.session_id.clone();
        tokio::spawn(async move {
            use tddy_vm::Vm as _;
            let vm_impl = tddy_vm::QemuVm;
            match vm_impl.boot(&config).await {
                Ok(vm) => {
                    let mut state = state_ref.lock().await;
                    state.insert(
                        session_id,
                        activity_hub::DemoVmHandle::Running { vm, share_url },
                    );
                }
                Err(e) => {
                    let mut state = state_ref.lock().await;
                    state.insert(session_id, activity_hub::DemoVmHandle::Error(e.to_string()));
                }
            }
        });

        log::info!(
            "start_demo_vm: booting VM for session_id={}",
            req.session_id
        );
        Ok(Response::new(StartDemoVmResponse {
            state: DemoVmState::Booting as i32,
            message: "booting".to_string(),
        }))
    }

    async fn stop_demo_vm(
        &self,
        request: Request<StopDemoVmRequest>,
    ) -> Result<Response<StopDemoVmResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let handle = {
            let mut state = self.demo_vm_state.lock().await;
            state.remove(&req.session_id)
        };

        match handle {
            Some(activity_hub::DemoVmHandle::Running { vm, .. }) => {
                use tddy_vm::Vm as _;
                let vm_impl = tddy_vm::QemuVm;
                match vm_impl.shutdown(vm).await {
                    Ok(()) => {
                        log::info!("stop_demo_vm: shutdown ok session_id={}", req.session_id);
                        Ok(Response::new(StopDemoVmResponse {
                            ok: true,
                            message: "shutdown".to_string(),
                        }))
                    }
                    Err(e) => Err(Status::internal(format!("shutdown failed: {e}"))),
                }
            }
            Some(activity_hub::DemoVmHandle::Booting) => Err(Status::failed_precondition(
                "VM is still booting; wait until Running before stopping",
            )),
            Some(activity_hub::DemoVmHandle::Error(msg)) => Ok(Response::new(StopDemoVmResponse {
                ok: true,
                message: format!("VM was in error state ({msg}); cleared"),
            })),
            None => Ok(Response::new(StopDemoVmResponse {
                ok: true,
                message: "no VM running for this session".to_string(),
            })),
        }
    }

    async fn get_demo_vm_status(
        &self,
        request: Request<GetDemoVmStatusRequest>,
    ) -> Result<Response<GetDemoVmStatusResponse>, Status> {
        let req = request.into_inner();
        self.record_rpc_activity();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let _os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        validate_session_id_segment(&req.session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;

        let state = self.demo_vm_state.lock().await;
        let resp = match state.get(&req.session_id) {
            None => GetDemoVmStatusResponse {
                state: DemoVmState::Stopped as i32,
                ssh_host_port: 0,
                message: "no VM for this session".to_string(),
                share_url: String::new(),
            },
            Some(activity_hub::DemoVmHandle::Booting) => GetDemoVmStatusResponse {
                state: DemoVmState::Booting as i32,
                ssh_host_port: 0,
                message: "booting".to_string(),
                share_url: String::new(),
            },
            Some(activity_hub::DemoVmHandle::Running { vm, share_url }) => {
                GetDemoVmStatusResponse {
                    state: DemoVmState::Running as i32,
                    ssh_host_port: vm.ssh_host_port as u32,
                    message: "running".to_string(),
                    share_url: share_url.clone(),
                }
            }
            Some(activity_hub::DemoVmHandle::Error(msg)) => GetDemoVmStatusResponse {
                state: DemoVmState::Error as i32,
                ssh_host_port: 0,
                message: msg.clone(),
                share_url: String::new(),
            },
        };
        Ok(Response::new(resp))
    }

    async fn get_worktree_snapshot(
        &self,
        request: Request<GetWorktreeSnapshotRequest>,
    ) -> Result<Response<GetWorktreeSnapshotResponse>, Status> {
        let inner = super::family_proto_bridge::wire_same(&request.into_inner())?;
        let out = self.get_worktree_snapshot_at_session_coordinate(Request::new(inner)).await?;
        Ok(Response::new(super::family_proto_bridge::wire_same(&out.into_inner())?))
    }
    async fn mint_local_token(
        &self,
        _request: Request<MintLocalTokenRequest>,
    ) -> Result<Response<MintLocalTokenResponse>, Status> {
        Err(Status::unauthenticated(
            "local token minting is only available over the local socket",
        ))
    }

    /// Associated output stream type for [`stream_start_session`].
    type StreamStartSessionStream = MpscResultStream<StartSessionEvent>;

    async fn stream_start_session(
        &self,
        request: Request<StartSessionRequest>,
    ) -> Result<Response<Self::StreamStartSessionStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();
        // Authenticate before classifying the route: forwarding opens an outbound RPC to a peer and
        // holds a pending-call slot on both hosts for the forward's whole deadline, so an
        // unauthenticated caller must never get that far. The resolved user is not used here — the
        // host that runs the session resolves it again under its own mapping — which is the same
        // order the unary `start_session` and `stream_read_host_document` already use.
        self.resolve_os_user(&req.session_token)?;

        if let PeerRoute::Forward { peer_instance_id } =
            self.classify_daemon_route(&req.daemon_instance_id)?
        {
            log::info!(
                "StreamStartSession: forwarding stream to remote daemon_instance_id={peer_instance_id}"
            );
            let slot = self.common_room_slot("StreamStartSession")?;
            // The session, its worktree and its attachments are created on the peer; only its
            // events cross back, so progress still reaches the client for the slowest case there
            // is — attachment bytes moving between two hosts.
            let rx = tddy_daemon_livekit::livekit_peer_discovery::forward_stream_start_session_via_livekit(
                slot,
                &peer_instance_id,
                &req,
            )
            .await?;
            return Ok(Response::new(MpscResultStream { rx }));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<StartSessionEvent, Status>>();
        // The work runs on its own task so progress reaches the client while the host is still
        // materializing, rather than all at once after the start completes. A failure terminates
        // the stream with the status — a result event is only ever sent on success.
        let service = self.clone();
        let progress_tx = tx.clone();
        tokio::spawn(async move {
            let sink = AttachmentProgressSink::streaming(progress_tx);
            let event = match service.start_session_core(req, &sink).await {
                Ok(response) => Ok(StartSessionEvent {
                    event: Some(StartSessionEventKind::Result(response.into_inner())),
                }),
                Err(status) => Err(status),
            };
            let _ = tx.send(event);
        });
        Ok(Response::new(MpscResultStream { rx }))
    }
}
