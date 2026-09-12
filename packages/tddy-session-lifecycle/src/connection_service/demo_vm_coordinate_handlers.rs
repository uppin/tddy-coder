//! Family O demo VM RPCs at the `demo_vm.DemoVmService` coordinate.

use std::sync::Arc;

use super::activity_hub;
use super::DaemonSessionHost;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::demo_vm::{
    DemoVmState, GetDemoVmStatusRequest, GetDemoVmStatusResponse, StartDemoVmRequest,
    StartDemoVmResponse, StopDemoVmRequest, StopDemoVmResponse,
};

impl DaemonSessionHost {
    pub(crate) async fn start_demo_vm_at_coordinate(
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

    pub(crate) async fn stop_demo_vm_at_coordinate(
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

    pub(crate) async fn get_demo_vm_status_at_coordinate(
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

}
