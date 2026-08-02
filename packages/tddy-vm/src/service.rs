//! VmServiceImpl — wires VmManager to the generated VmService RPC trait.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::vm::{
    build_tddy_host_image_progress::Stage as TddyHostStage, BuildTddyHostImageProgress,
    BuildTddyHostImageRequest, BuildVmImageProgress, BuildVmImageRequest,
    CreateVmFromPreparedBaseRequest, CreateVmFromPreparedBaseResponse, DefineVmRequest,
    DefineVmResponse, GetVmStatusRequest, GetVmStatusResponse, ListVmImagesRequest,
    ListVmImagesResponse, ListVmsRequest, ListVmsResponse, RemoveVmRequest, RemoveVmResponse,
    StartVmRequest, StartVmResponse, StopVmRequest, StopVmResponse, VmImageInfo, VmInfo,
    VmPortForward, VmRunPolicyProto, VmService,
};
use tddy_task::{ChannelKind, TaskRegistry};
use tokio_stream::wrappers::ReceiverStream;

use crate::build::VmBuildTaskBody;
use crate::registry::{VmManager, VmSpec, VmState};
use crate::tddy_host::{
    LiveKitCommonRoom, TddyHostBuildOptions, TddyHostSpec, BAKING_PROGRESS_LINE,
    DEFAULT_TDDY_HOST_USERNAME, IMPORTING_PROGRESS_LINE, SEEDING_PROGRESS_LINE,
};
use crate::vm::{PortForward, VmAccel, VmArch, VmError};
use crate::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};

/// How long the tddy host bake may run before it is abandoned.
///
/// The bake installs Nix and runs a cold `./release` for the whole workspace (including
/// `libwebrtc`) inside the guest, so hours is the expected duration, not the pathological
/// one. Six hours is a ceiling that catches a genuinely wedged build without failing a
/// merely slow one.
pub const TDDY_HOST_BAKE_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);

/// Capacity of the progress channel behind every streaming VM method's `ReceiverStream`.
const PROGRESS_CHANNEL_CAPACITY: usize = 64;

/// How many console lines may sit between the bake's synchronous progress callback and the
/// forwarder that puts them on the RPC stream.
///
/// A client that stops reading must not let an hours-long bake grow an unbounded backlog,
/// so lines past this point are dropped and counted — and the count is reported on the
/// stream before the terminal message, so the caller knows its log has a hole in it.
const BAKE_LINE_BACKLOG: usize = 512;

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

    /// The VM & Image Library the prepared-base methods work against. A JSON-backed manager
    /// has no image library at all, so those methods cannot be served from one.
    fn library(&self) -> Result<&crate::library::VmLibrary, Status> {
        self.manager.library().ok_or_else(|| {
            Status::failed_precondition(
                "this daemon's VM manager is not backed by a VM & Image Library",
            )
        })
    }
}

/// Reject an RPC-supplied value that is about to become a single path component under the
/// VM & Image Library root — a VM name, a prepared-base name, a base-image name.
///
/// These are `join`ed onto library paths, so `..` or a separator in one would place the
/// resulting VM directory, overlay or manifest outside the library the daemon owns.
///
/// Deliberately not applied to `base_image_path` and `source_dir`: those are whole host
/// paths the operator supplies on purpose, not components of a library path.
fn validate_library_name(field: &str, value: &str) -> Result<(), Status> {
    if value.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    if value.contains('/') || value.contains('\\') || value.contains("..") {
        return Err(Status::invalid_argument(format!(
            "{field} '{value}' must be a plain name: '/', '\\' and '..' would place it \
             outside the VM & Image Library"
        )));
    }
    Ok(())
}

/// Map the wire port forwards onto their [`PortForward`] equivalents.
fn port_forwards_from_proto(forwards: Vec<VmPortForward>) -> Vec<PortForward> {
    forwards
        .into_iter()
        .map(|p| PortForward {
            host_port: p.host_port as u16,
            guest_port: p.guest_port as u16,
        })
        .collect()
}

fn parse_vm_arch(value: &str) -> Result<VmArch, Status> {
    match value {
        "" => Ok(VmArch::host()),
        "aarch64" => Ok(VmArch::Aarch64),
        "x86_64" => Ok(VmArch::X86_64),
        other => Err(Status::invalid_argument(format!(
            "unsupported arch '{other}': expected 'aarch64', 'x86_64', or empty for the host's"
        ))),
    }
}

fn parse_vm_accel(value: &str) -> Result<VmAccel, Status> {
    match value {
        "" => Ok(VmAccel::host_default()),
        "hvf" => Ok(VmAccel::Hvf),
        "kvm" => Ok(VmAccel::Kvm),
        "tcg" => Ok(VmAccel::Tcg),
        other => Err(Status::invalid_argument(format!(
            "unsupported accel '{other}': expected 'hvf', 'kvm', 'tcg', or empty for the host's"
        ))),
    }
}

/// Map the wire run policy onto the manifest's [`RunPolicy`].
fn run_policy_from_proto(run: Option<VmRunPolicyProto>) -> Result<RunPolicy, Status> {
    let run = run.ok_or_else(|| Status::invalid_argument("run is required"))?;
    Ok(RunPolicy {
        memory: run.memory,
        cpus: run.cpus,
        disk_size: run.disk_size,
        ssh_host_port: run.ssh_host_port as u16,
        port_forwards: port_forwards_from_proto(run.port_forwards),
        arch: parse_vm_arch(&run.arch)?,
        accel: parse_vm_accel(&run.accel)?,
    })
}

/// Which stage a [`crate::tddy_host::build_tddy_host_image`] progress line belongs to.
///
/// The bake reports plain lines — most of them raw serial-console output — and marks each
/// stage boundary with a known line, so the RPC stream can carry a stage without the
/// orchestrator itself knowing anything about protobuf. Lines that mark no boundary stay in
/// the stage that is already running.
fn stage_for_progress_line(line: &str, current: TddyHostStage) -> TddyHostStage {
    match line {
        IMPORTING_PROGRESS_LINE => TddyHostStage::Importing,
        SEEDING_PROGRESS_LINE => TddyHostStage::Seeding,
        BAKING_PROGRESS_LINE => TddyHostStage::Baking,
        _ => current,
    }
}

/// Send one `BuildTddyHostImage` progress message, ignoring a dropped receiver.
async fn send_tddy_host_progress(
    tx: &tokio::sync::mpsc::Sender<Result<BuildTddyHostImageProgress, Status>>,
    stage: TddyHostStage,
    message: impl Into<String>,
    prepared_base_name: impl Into<String>,
) {
    let _ = tx
        .send(Ok(BuildTddyHostImageProgress {
            stage: stage as i32,
            message: message.into(),
            prepared_base_name: prepared_base_name.into(),
        }))
        .await;
}

/// Run the bake, streaming each of its progress lines to `tx` tagged with the stage it
/// belongs to, and finish with exactly one terminal `STAGE_DONE`/`STAGE_ERROR` message.
///
/// [`crate::tddy_host::build_tddy_host_image`]'s progress callback is synchronous, so lines
/// go through a [`BAKE_LINE_BACKLOG`]-deep channel that a forwarder task drains onto the RPC
/// stream in order. A client that stops reading stalls the forwarder, and lines that no
/// longer fit behind it are dropped and counted rather than buffered without limit; the
/// count is reported on the stream just before the terminal message. The forwarder is joined
/// first, so the terminal message is always the last one on the stream.
async fn bake_tddy_host_image(
    options: &TddyHostBuildOptions,
    tx: &tokio::sync::mpsc::Sender<Result<BuildTddyHostImageProgress, Status>>,
    log_ch: &Option<Arc<tddy_task::TaskChannel>>,
) -> Result<PathBuf, String> {
    let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(BAKE_LINE_BACKLOG);
    let dropped_lines = Arc::new(AtomicU64::new(0));
    let forwarder = {
        let tx = tx.clone();
        let log_ch = log_ch.clone();
        tokio::spawn(async move {
            let mut stage = TddyHostStage::Unknown;
            while let Some(line) = line_rx.recv().await {
                stage = stage_for_progress_line(&line, stage);
                crate::build::write_to_channel(&log_ch, &line);
                send_tddy_host_progress(&tx, stage, &line, "").await;
            }
            stage
        })
    };

    // The callback owns `line_tx`; dropping it at the end of this statement closes the
    // channel, which is what ends the forwarder task awaited below.
    let counter = Arc::clone(&dropped_lines);
    let result = crate::tddy_host::build_tddy_host_image(options, &move |line: &str| {
        if line_tx.try_send(line.to_string()).is_err() {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    })
    .await;
    // A panicking forwarder leaves no stage to attribute the drop notice to; the notice
    // still has to reach the caller, so it goes out under the stream's initial stage.
    let last_stage = forwarder.await.unwrap_or(TddyHostStage::Unknown);

    let dropped = dropped_lines.load(Ordering::Relaxed);
    if dropped > 0 {
        let message = format!(
            "{dropped} console line(s) were dropped: the bake produced output faster than \
             this stream was read"
        );
        send_tddy_host_progress(tx, last_stage, &message, "").await;
        crate::build::write_to_channel(log_ch, &message);
    }

    match result {
        Ok(prepared_base) => {
            send_tddy_host_progress(
                tx,
                TddyHostStage::Done,
                format!("Prepared base baked at {}", prepared_base.display()),
                &options.name,
            )
            .await;
            crate::build::write_to_channel(log_ch, "Tddy host image build complete");
            Ok(prepared_base)
        }
        Err(e) => {
            let message = e.to_string();
            send_tddy_host_progress(tx, TddyHostStage::Error, &message, "").await;
            crate::build::write_to_channel(log_ch, &message);
            Err(message)
        }
    }
}

/// `TaskBody` for a tddy host bake, so the hours-long build is observable through
/// `TaskService.WatchTask` and outlives the RPC call that started it.
struct TddyHostBuildTaskBody {
    options: TddyHostBuildOptions,
    progress_tx: tokio::sync::mpsc::Sender<Result<BuildTddyHostImageProgress, Status>>,
}

#[async_trait]
impl tddy_task::TaskBody for TddyHostBuildTaskBody {
    async fn run(self: Box<Self>, ctx: tddy_task::TaskContext) -> tddy_task::TaskStatus {
        let log_ch = ctx.channel("0");
        match bake_tddy_host_image(&self.options, &self.progress_tx, &log_ch).await {
            Ok(_) => tddy_task::TaskStatus::Completed { exit_code: Some(0) },
            Err(message) => tddy_task::TaskStatus::Failed { message },
        }
    }
}

/// The generated per-VM public key, read from the path the persisted manifest records.
///
/// The error is a plain description rather than a [`Status`]: by the time this runs the VM
/// exists, so the caller is told about a created-but-unreadable-key VM in the response body,
/// not through a transport failure that implies nothing happened.
async fn read_generated_public_key(
    library: &crate::library::VmLibrary,
    name: &str,
) -> Result<String, String> {
    let manifest = library.read_manifest(name).map_err(|e| e.to_string())?;
    let path = manifest
        .login
        .ssh_public_key
        .ok_or_else(|| format!("VM '{name}' was created without recording a public key path"))?;
    tokio::fs::read_to_string(&path)
        .await
        .map(|key| key.trim().to_string())
        .map_err(|e| format!("failed to read public key {path}: {e}"))
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
    type BuildTddyHostImageStream = ReceiverStream<Result<BuildTddyHostImageProgress, Status>>;

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
            port_forwards: port_forwards_from_proto(proto_spec.port_forwards),
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
        let (progress_tx, progress_rx) = tokio::sync::mpsc::channel(PROGRESS_CHANNEL_CAPACITY);

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

    async fn build_tddy_host_image(
        &self,
        request: Request<BuildTddyHostImageRequest>,
    ) -> Result<Response<Self::BuildTddyHostImageStream>, Status> {
        let req = request.into_inner();
        // Validate the token before spawning the hours-long bake.
        self.authenticate(&req.session_token)?;
        let library = self.library()?.clone();
        validate_library_name("name", &req.name)?;
        validate_library_name("base_image_name", &req.base_image_name)?;
        let run = run_policy_from_proto(req.run)?;

        let options = TddyHostBuildOptions {
            library,
            base_image_name: req.base_image_name,
            // Whole host paths chosen by the operator running this daemon, not components
            // of a library path — deliberately taken as given, unlike the names above.
            base_image_src: PathBuf::from(req.base_image_path),
            source_dir: PathBuf::from(req.source_dir),
            spec: TddyHostSpec {
                // TODO: every VM baked from this base therefore announces the same
                // `daemon_instance_id`. Distinguish them once a VM's manifest can carry a
                // hostname the guest applies at boot.
                hostname: req.name.clone(),
                username: DEFAULT_TDDY_HOST_USERNAME.to_string(),
                livekit: req.livekit.map(|lk| LiveKitCommonRoom {
                    url: lk.url,
                    api_key: lk.api_key,
                    api_secret: lk.api_secret,
                    common_room: lk.common_room,
                }),
            },
            name: req.name,
            disk_size: run.disk_size,
            memory: run.memory,
            cpus: run.cpus,
            ssh_host_port: run.ssh_host_port,
            timeout: TDDY_HOST_BAKE_TIMEOUT,
        };

        let (progress_tx, progress_rx) = tokio::sync::mpsc::channel(PROGRESS_CHANNEL_CAPACITY);
        let log_ch =
            tddy_task::TaskChannel::output_only("0", "tddy-host-bake-log", ChannelKind::Combined);
        self.task_registry
            .spawn(
                TddyHostBuildTaskBody {
                    options,
                    progress_tx,
                },
                "tddy_host_bake",
                "",
                vec![log_ch],
            )
            .await;

        Ok(Response::new(ReceiverStream::new(progress_rx)))
    }

    async fn create_vm_from_prepared_base(
        &self,
        request: Request<CreateVmFromPreparedBaseRequest>,
    ) -> Result<Response<CreateVmFromPreparedBaseResponse>, Status> {
        let req = request.into_inner();
        self.authenticate(&req.session_token)?;
        let library = self.library()?;
        validate_library_name("name", &req.name)?;
        validate_library_name("prepared_base", &req.prepared_base)?;
        let run = run_policy_from_proto(req.run)?;
        if req.ssh_username.is_empty() {
            return Err(Status::invalid_argument(
                "ssh_username is required — it must name the account baked into the prepared base",
            ));
        }

        let manifest = VmManifest {
            name: req.name,
            prepared_base: Some(req.prepared_base),
            image_path: None,
            run,
            login: LoginPolicy {
                username: req.ssh_username,
                // Superseded by the keypair `create_from_prepared_base` generates.
                ssh_private_key: None,
                ssh_public_key: None,
            },
        };

        let overlay_path = match self.manager.create_from_prepared_base(&manifest).await {
            Ok(overlay_path) => overlay_path,
            // A VM that cannot be created is a reportable outcome of this call, not a
            // transport failure — the caller gets the reason, and no half-made VM.
            Err(e) => {
                return Ok(Response::new(CreateVmFromPreparedBaseResponse {
                    ok: false,
                    message: e.to_string(),
                    overlay_path: String::new(),
                    ssh_public_key: String::new(),
                }))
            }
        };

        // From here the VM exists on disk and in the registry. An unreadable public key is
        // therefore reported as a failed creation that nonetheless left something behind —
        // naming the overlay so the caller can inspect or remove it — rather than as a
        // transport error that would suggest nothing was created.
        match read_generated_public_key(library, &manifest.name).await {
            Ok(ssh_public_key) => Ok(Response::new(CreateVmFromPreparedBaseResponse {
                ok: true,
                message: String::new(),
                overlay_path: overlay_path.display().to_string(),
                ssh_public_key,
            })),
            Err(reason) => Ok(Response::new(CreateVmFromPreparedBaseResponse {
                ok: false,
                message: format!(
                    "VM '{}' was created at {} but its generated public key could not be \
                     read: {reason}",
                    manifest.name,
                    overlay_path.display()
                ),
                overlay_path: overlay_path.display().to_string(),
                ssh_public_key: String::new(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_rpc::Code;

    /// The rejection a validator produced, for a test that asserts on how a bad argument is
    /// refused rather than on a value.
    fn rejection_of<T: std::fmt::Debug>(result: Result<T, Status>) -> Status {
        result.expect_err("expected the argument to be rejected")
    }

    // ── Progress-line staging ────────────────────────────────────────────────

    #[test]
    fn the_import_line_moves_the_bake_stream_into_the_importing_stage() {
        // Given / When — the line the bake emits as it copies the operator's cloud image
        let stage = stage_for_progress_line(IMPORTING_PROGRESS_LINE, TddyHostStage::Unknown);

        // Then
        assert_eq!(stage, TddyHostStage::Importing);
    }

    #[test]
    fn the_cloud_init_line_moves_the_bake_stream_into_the_seeding_stage() {
        // Given / When
        let stage = stage_for_progress_line(SEEDING_PROGRESS_LINE, TddyHostStage::Importing);

        // Then
        assert_eq!(stage, TddyHostStage::Seeding);
    }

    #[test]
    fn the_bake_line_moves_the_bake_stream_into_the_baking_stage() {
        // Given / When
        let stage = stage_for_progress_line(BAKING_PROGRESS_LINE, TddyHostStage::Seeding);

        // Then
        assert_eq!(stage, TddyHostStage::Baking);
    }

    #[test]
    fn a_serial_console_line_stays_in_the_stage_that_is_already_running() {
        // Given / When — an ordinary guest console line, marking no stage boundary
        let stage = stage_for_progress_line(
            "[  12.345678] systemd[1]: Reached target Multi-User System.",
            TddyHostStage::Baking,
        );

        // Then
        assert_eq!(stage, TddyHostStage::Baking);
    }

    // ── Run policy: arch and accel ───────────────────────────────────────────

    #[test]
    fn an_empty_arch_means_the_architecture_this_daemon_runs_on() {
        // Given / When
        let arch = parse_vm_arch("").expect("an empty arch is the host's");

        // Then
        assert_eq!(arch, VmArch::host());
    }

    #[test]
    fn an_empty_accel_means_the_accelerator_this_daemon_can_use() {
        // Given / When
        let accel = parse_vm_accel("").expect("an empty accel is the host's");

        // Then
        assert_eq!(accel, VmAccel::host_default());
    }

    #[test]
    fn an_unsupported_arch_is_rejected_naming_the_ones_that_are_supported() {
        // Given / When
        let rejection = rejection_of(parse_vm_arch("riscv64"));

        // Then
        assert_eq!(rejection.code, Code::InvalidArgument);
        assert_eq!(
            rejection.message,
            "unsupported arch 'riscv64': expected 'aarch64', 'x86_64', or empty for the host's"
        );
    }

    #[test]
    fn an_unsupported_accel_is_rejected_naming_the_ones_that_are_supported() {
        // Given / When
        let rejection = rejection_of(parse_vm_accel("whpx"));

        // Then
        assert_eq!(rejection.code, Code::InvalidArgument);
        assert_eq!(
            rejection.message,
            "unsupported accel 'whpx': expected 'hvf', 'kvm', 'tcg', or empty for the host's"
        );
    }

    #[test]
    fn a_request_without_a_run_policy_is_rejected() {
        // Given / When — the field is optional on the wire but required by this service
        let rejection = rejection_of(run_policy_from_proto(None));

        // Then
        assert_eq!(rejection.code, Code::InvalidArgument);
        assert_eq!(rejection.message, "run is required");
    }

    #[test]
    fn every_wire_port_forward_is_carried_onto_the_manifest_in_order() {
        // Given — the forwards a caller asks for alongside SSH
        let proto = vec![
            VmPortForward {
                host_port: 8080,
                guest_port: 80,
            },
            VmPortForward {
                host_port: 5432,
                guest_port: 5432,
            },
        ];

        // When
        let mapped = port_forwards_from_proto(proto);

        // Then
        let pairs: Vec<(u16, u16)> = mapped.iter().map(|p| (p.host_port, p.guest_port)).collect();
        assert_eq!(pairs, vec![(8080, 80), (5432, 5432)]);
    }

    // ── Library-name validation ──────────────────────────────────────────────

    #[test]
    fn a_plain_library_name_is_accepted() {
        // Given / When / Then
        assert!(validate_library_name("name", "debian-12-tddy").is_ok());
    }

    #[test]
    fn an_empty_library_name_is_rejected_naming_the_field() {
        // Given / When
        let rejection = rejection_of(validate_library_name("prepared_base", ""));

        // Then
        assert_eq!(rejection.code, Code::InvalidArgument);
        assert_eq!(rejection.message, "prepared_base is required");
    }

    #[test]
    fn a_library_name_climbing_out_of_the_library_root_is_rejected() {
        // Given / When — the name would resolve above `<root>/vm/`
        let rejection = rejection_of(validate_library_name("name", "../../etc/tddy"));

        // Then
        assert_eq!(rejection.code, Code::InvalidArgument);
        assert_eq!(
            rejection.message,
            "name '../../etc/tddy' must be a plain name: '/', '\\' and '..' would place it \
             outside the VM & Image Library"
        );
    }

    #[test]
    fn a_library_name_with_a_path_separator_is_rejected() {
        // Given / When — a name that would nest the VM directory somewhere of its choosing
        let rejection = rejection_of(validate_library_name("base_image_name", "images/debian-12"));

        // Then
        assert_eq!(rejection.code, Code::InvalidArgument);
        assert_eq!(
            rejection.message,
            "base_image_name 'images/debian-12' must be a plain name: '/', '\\' and '..' would \
             place it outside the VM & Image Library"
        );
    }

    #[test]
    fn a_library_name_with_a_windows_path_separator_is_rejected() {
        // Given / When
        let rejection = rejection_of(validate_library_name("name", "vm\\escape"));

        // Then
        assert_eq!(rejection.code, Code::InvalidArgument);
        assert_eq!(
            rejection.message,
            "name 'vm\\escape' must be a plain name: '/', '\\' and '..' would place it \
             outside the VM & Image Library"
        );
    }
}
