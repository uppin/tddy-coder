//! The privileged surface: two gates, then a broker.
//!
//! Every request passes the same two checks, in the same order, and neither is ever skipped:
//!
//! 1. **[`Authorizer::authorize`]** — is this peer one of my services? Decided from `SO_PEERCRED`
//!    on the socket, before a single field of the request is interpreted.
//! 2. **[`crate::policy`]** — may that peer have *this*? Decided from the root-owned config, never
//!    from anything the request asserts about itself.
//!
//! They are separate because they answer different questions, and a regression in one must not be
//! able to hide behind the other.

use std::collections::BTreeMap;
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_stdio::StdioEndpoint;
use tokio::net::UnixListener;

use crate::authz::{Authorizer, PeerIdentity};
use crate::cgroup_broker::CgroupBroker;
use crate::config::{SocketConfig, SupervisorConfig};
use crate::error::SupervisorError;
use crate::policy;
use crate::proto::supervisor::{self as wire, SupervisorService, SupervisorServiceServer};
use crate::protocol;
use crate::request::{
    CreateScopeRequest, ScopeHandle, SessionStatus, SpawnSandboxRequest, SpawnSessionRequest,
    SpawnedProcess,
};
use crate::socket::{resolve_socket_source, SocketSource};
use crate::spawn_broker::{self, EnvironmentBase, JailMount, SandboxJail, SpawnPlan, TargetUser};
use crate::supervisor::Supervisor;

/// Name this service answers to on the wire, taken from the generated server so a client and a
/// server built from the same proto can never disagree about it.
pub const SERVICE_NAME: &str = SupervisorServiceServer::<SupervisorServiceImpl>::NAME;

/// Everything the privileged surface needs, shared by every connection.
pub struct PrivilegedSurface {
    config: Arc<SupervisorConfig>,
    authorizer: Authorizer,
    cgroups: CgroupBroker,
    supervisor: Arc<Supervisor>,
}

impl PrivilegedSurface {
    pub fn new(
        config: Arc<SupervisorConfig>,
        supervisor: Arc<Supervisor>,
        cgroups: CgroupBroker,
    ) -> PrivilegedSurface {
        // The allowed set is exactly the uids that own a declared service. A supervisor managing
        // nothing therefore serves nobody, which is the correct reading of "deny by default".
        let authorizer = Authorizer::from_service_uids(supervisor.service_uids());
        PrivilegedSurface {
            config,
            authorizer,
            cgroups,
            supervisor,
        }
    }

    async fn spawn_session(
        &self,
        request: SpawnSessionRequest,
    ) -> Result<SpawnedProcess, SupervisorError> {
        let resolved = self.resolve_spawn(
            &request.os_user,
            &request.tool_path,
            request.working_dir.as_deref(),
            request.scope.as_deref(),
            &request.env,
        )?;
        let os_user = resolved.os_user.clone();
        // `SpawnSession` spawns a plain process. Jailing is `SpawnSandbox`'s job.
        let plan = resolved.plan(request.args, None);
        self.spawn(plan, "session", &os_user).await
    }

    /// Resolve a sandbox request against policy and hand the jail plan to the supervisor.
    ///
    /// The jail is only *described* here. `spawn_broker::pre_exec_plan` decides the order its steps
    /// happen in, and the child performs them between `fork` and `exec`. A jail that cannot be built
    /// fails the spawn at whichever of the two points discovers it — `CompiledStep::compile` before
    /// the fork for a bind mount source it cannot resolve, the child itself for a namespace the host
    /// refuses it — and the caller gets that failure verbatim. Deliberate: an unjailed process
    /// returned from `SpawnSandbox` would be worse than no process at all.
    async fn spawn_sandbox(
        &self,
        request: SpawnSandboxRequest,
    ) -> Result<SpawnedProcess, SupervisorError> {
        let policy = &self.config.spawn_policy;
        let resolved = self.resolve_spawn(
            &request.os_user,
            &request.tool_path,
            request.working_dir.as_deref(),
            request.scope.as_deref(),
            &request.env,
        )?;

        // Every source, not just the first: the mount list is the one part of a sandbox request that
        // names host paths, so it is the part a compromised caller reaches for. A policy that granted
        // no roots therefore grants no mounts, which is the correct reading of an absent mount policy.
        let mut mounts = Vec::with_capacity(request.mounts.len());
        for mount in request.mounts {
            mounts.push(JailMount {
                source: policy::resolve_mount_source(policy, &mount.source)?,
                target: mount.target,
                readonly: mount.readonly,
            });
        }

        let os_user = resolved.os_user.clone();
        let plan = resolved.plan(
            request.args,
            Some(SandboxJail {
                mounts,
                isolate_network: request.isolate_network,
            }),
        );
        self.spawn(plan, "sandbox", &os_user).await
    }

    /// The policy gate both spawn methods pass, in one place so the two cannot drift into checking
    /// different things.
    fn resolve_spawn(
        &self,
        os_user: &str,
        tool_path: &Path,
        working_dir: Option<&Path>,
        scope: Option<&str>,
        env: &BTreeMap<String, String>,
    ) -> Result<ResolvedSpawn, SupervisorError> {
        let policy = &self.config.spawn_policy;
        let os_user = policy::resolve_session_user(policy, os_user)?;
        let program = policy::resolve_tool_path(policy, tool_path)?;
        // Here rather than in either caller: the environment is as much a way to choose what runs as
        // the tool path is, and a gate that only one of the two spawn methods passed through would be
        // a gate the other one is missing.
        let env = policy::resolve_env(policy, env)?;
        let scope_procs = match scope {
            Some(scope) => Some(self.cgroups.scope_procs_path(scope)?),
            None => None,
        };

        // An allowlisted account that does not exist on this host is refused exactly like one the
        // policy never listed. Distinguishing the two would tell the caller which names are
        // allowlisted, which is the oracle this boundary exists to close.
        let target = spawn_broker::resolve_target_user(&os_user).map_err(|error| {
            log::warn!(
                target: "tddy_supervisor::server",
                "spawn refused: {error}"
            );
            SupervisorError::Denied
        })?;

        Ok(ResolvedSpawn {
            os_user,
            program,
            env,
            working_dir: working_dir
                .map(Path::to_path_buf)
                .unwrap_or_else(|| target.home.clone()),
            target,
            scope_procs,
        })
    }

    /// Hand a resolved plan to the supervisor.
    ///
    /// Through the supervisor rather than straight to the fork broker, so the session is accounted
    /// for from the instant it exists — `shutdown` takes it with it, an instant exit is attributed
    /// rather than lost, and a caller can ask afterwards what became of it.
    async fn spawn(
        &self,
        plan: SpawnPlan,
        kind: &str,
        os_user: &str,
    ) -> Result<SpawnedProcess, SupervisorError> {
        // A failure here is reported as what it is. In particular a jail the supervisor cannot build
        // surfaces as `OperationFailed` carrying the reason: dressing it as a denial would hide a
        // host that cannot jail behind a policy-shaped error, and spawning the session without the
        // jail it asked for would be worse than either.
        let pid = self.supervisor.spawn_session(plan).await.map_err(|error| {
            SupervisorError::OperationFailed {
                message: format!("spawn {kind}: {error}"),
            }
        })?;
        log::info!(
            target: "tddy_supervisor::server",
            "spawned {kind} pid {pid} as `{os_user}`"
        );
        Ok(SpawnedProcess { pid })
    }

    /// Stop a session, giving it the same grace before `SIGKILL` that the supervisor's own shutdown
    /// gives a child: how long a process gets to exit on request is a property of the host, not of
    /// who asked.
    fn stop_session(&self, pid: u32) -> Result<SessionStatus, SupervisorError> {
        self.supervisor
            .stop_session(pid, Duration::from_secs(self.config.shutdown_grace_secs))
    }

    fn create_scope(&self, request: CreateScopeRequest) -> Result<ScopeHandle, SupervisorError> {
        self.cgroups.create_scope(&request)
    }

    /// Move one of this supervisor's live sessions into a scope.
    ///
    /// Gated on the session table exactly as [`Self::stop_session`] is, and for a stronger reason:
    /// `cgroup.procs` accepts any pid its writer may move, and the writer here is root. Ungated, a
    /// caller could name `sshd`, the daemon, or the supervisor itself and have it placed in a scope
    /// whose `memory.max` was clamped small — the clamping that makes `CreateScope` safe stops
    /// mattering the moment the scope can be pointed at somebody else's process.
    fn attach_pid(&self, scope: &str, pid: u32) -> Result<(), SupervisorError> {
        self.supervisor.require_running_session(pid)?;
        self.cgroups.attach_pid(scope, pid)
    }
}

/// A spawn request with every policy decision already made: the account, the binary, the directory
/// and the scope are all resolved values, and nothing the caller asserted is carried forward.
struct ResolvedSpawn {
    os_user: String,
    program: PathBuf,
    /// The caller's environment, with every variable checked against policy.
    env: BTreeMap<String, String>,
    working_dir: PathBuf,
    target: TargetUser,
    scope_procs: Option<PathBuf>,
}

impl ResolvedSpawn {
    fn plan(self, args: Vec<String>, sandbox: Option<SandboxJail>) -> SpawnPlan {
        SpawnPlan {
            program: self.program,
            args,
            env: self.env,
            working_dir: self.working_dir,
            target: self.target,
            scope_procs: self.scope_procs,
            // A session's output belongs to whoever asked for it, not to the supervisor's journal.
            inherit_output: false,
            // A session is spawned for somebody else and has no claim on the environment the
            // supervisor was started with — see [`EnvironmentBase`].
            environment: EnvironmentBase::Minimal,
            sandbox,
        }
    }
}

/// One connection's view of the surface: the surface itself, plus the peer that opened it.
///
/// Peer credentials are read once, at accept time, and belong to the connection — not to a request
/// field a caller could set.
pub struct SupervisorServiceImpl {
    surface: Arc<PrivilegedSurface>,
    peer: PeerIdentity,
}

impl SupervisorServiceImpl {
    pub fn new(surface: Arc<PrivilegedSurface>, peer: PeerIdentity) -> SupervisorServiceImpl {
        SupervisorServiceImpl { surface, peer }
    }

    /// The first gate. Runs before any request field is looked at, for every method.
    fn authorize_peer(&self) -> Result<(), Status> {
        self.surface
            .authorizer
            .authorize(&self.peer)
            .map_err(|denied| {
                log::warn!(
                    target: "tddy_supervisor::server",
                    "denied a request from uid {} pid {}",
                    self.peer.uid,
                    self.peer.pid
                );
                protocol::status_from_error(denied)
            })
    }
}

#[async_trait]
impl SupervisorService for SupervisorServiceImpl {
    async fn list_services(
        &self,
        _request: Request<wire::ListServicesRequest>,
    ) -> Result<Response<wire::ListServicesResponse>, Status> {
        self.authorize_peer()?;
        let services = self
            .surface
            .supervisor
            .statuses()
            .into_iter()
            .map(protocol::service_status_to_wire)
            .collect();
        Ok(Response::new(wire::ListServicesResponse { services }))
    }

    async fn start_service(
        &self,
        request: Request<wire::ServiceRef>,
    ) -> Result<Response<wire::ServiceStatus>, Status> {
        self.authorize_peer()?;
        let status = self
            .surface
            .supervisor
            .start_by_name(&request.into_inner().name)
            .await
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::service_status_to_wire(status)))
    }

    async fn stop_service(
        &self,
        request: Request<wire::ServiceRef>,
    ) -> Result<Response<wire::ServiceStatus>, Status> {
        self.authorize_peer()?;
        let status = self
            .surface
            .supervisor
            .stop_by_name(&request.into_inner().name)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::service_status_to_wire(status)))
    }

    async fn spawn_session(
        &self,
        request: Request<wire::SpawnSessionRequest>,
    ) -> Result<Response<wire::SpawnedProcess>, Status> {
        self.authorize_peer()?;
        let spawned = self
            .surface
            .spawn_session(protocol::spawn_session_from_wire(request.into_inner()))
            .await
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::spawned_process_to_wire(spawned)))
    }

    async fn spawn_sandbox(
        &self,
        request: Request<wire::SpawnSandboxRequest>,
    ) -> Result<Response<wire::SpawnedProcess>, Status> {
        self.authorize_peer()?;
        let spawned = self
            .surface
            .spawn_sandbox(protocol::spawn_sandbox_from_wire(request.into_inner()))
            .await
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::spawned_process_to_wire(spawned)))
    }

    async fn session_status(
        &self,
        request: Request<wire::SessionRef>,
    ) -> Result<Response<wire::SessionStatus>, Status> {
        self.authorize_peer()?;
        let status = self
            .surface
            .supervisor
            .session_status(request.into_inner().pid)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::session_status_to_wire(status)))
    }

    async fn stop_session(
        &self,
        request: Request<wire::SessionRef>,
    ) -> Result<Response<wire::SessionStatus>, Status> {
        self.authorize_peer()?;
        let status = self
            .surface
            .stop_session(request.into_inner().pid)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(protocol::session_status_to_wire(status)))
    }

    async fn create_scope(
        &self,
        request: Request<wire::CreateScopeRequest>,
    ) -> Result<Response<wire::ScopeHandle>, Status> {
        self.authorize_peer()?;
        let scope = self
            .surface
            .create_scope(protocol::create_scope_from_wire(request.into_inner()))
            .map_err(protocol::status_from_error)?;
        let scope = protocol::scope_handle_to_wire(scope).map_err(protocol::status_from_error)?;
        Ok(Response::new(scope))
    }

    async fn attach_pid(
        &self,
        request: Request<wire::AttachPidRequest>,
    ) -> Result<Response<wire::AttachPidResponse>, Status> {
        self.authorize_peer()?;
        let request = request.into_inner();
        self.surface
            .attach_pid(&request.scope, request.pid)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(wire::AttachPidResponse {}))
    }

    async fn destroy_scope(
        &self,
        request: Request<wire::ScopeRef>,
    ) -> Result<Response<wire::DestroyScopeResponse>, Status> {
        self.authorize_peer()?;
        self.surface
            .cgroups
            .destroy_scope(&request.into_inner().name)
            .map_err(protocol::status_from_error)?;
        Ok(Response::new(wire::DestroyScopeResponse {}))
    }
}

/// Bind the privileged socket, or adopt the one systemd created for us.
///
/// When systemd handed over a listener it also owns the socket node's ownership and mode, so
/// nothing here creates a directory, unlinks a stale file, or chmods anything.
pub fn bind_privileged_listener(socket: &SocketConfig) -> anyhow::Result<UnixListener> {
    let mode = socket
        .mode_bits()
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let source = resolve_socket_source(
        std::process::id(),
        std::env::var("LISTEN_PID").ok().as_deref(),
        std::env::var("LISTEN_FDS").ok().as_deref(),
        &socket.path,
    );

    match source {
        SocketSource::Activated(fd) => {
            // Consume the activation environment so managed services do not inherit it and adopt
            // whatever fd 3 happens to be for them.
            std::env::remove_var("LISTEN_PID");
            std::env::remove_var("LISTEN_FDS");
            std::env::remove_var("LISTEN_FDNAMES");

            let listener = adopt_activation_listener(fd)?;
            log::info!(
                target: "tddy_supervisor::server",
                "adopted the systemd activation listener on fd {fd}"
            );
            Ok(listener)
        }
        SocketSource::SelfBind(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    anyhow::anyhow!("create socket directory {}: {error}", parent.display())
                })?;
            }
            // A socket left behind by a previous run would make `bind` fail with EADDRINUSE.
            let _ = std::fs::remove_file(&path);
            let listener = UnixListener::bind(&path)
                .map_err(|error| anyhow::anyhow!("bind {}: {error}", path.display()))?;
            apply_socket_ownership(&path, socket.group.as_deref(), mode)?;
            log::info!(
                target: "tddy_supervisor::server",
                "listening on {} (mode {:o})",
                path.display(),
                mode
            );
            Ok(listener)
        }
    }
}

/// Take ownership of a listener systemd handed over, and make it safe to keep.
///
/// Two things have to be true of the adopted descriptor, and systemd guarantees neither:
///
/// * **Non-blocking**, because tokio drives it.
/// * **Close-on-exec.** Activation descriptors arrive with `FD_CLOEXEC` *clear* — `sd_listen_fds(1)`
///   is what sets it for you, and this hand-rolled adoption is not that. Rust's `Command` manages
///   only stdio, so an inherited descriptor stays open across `exec`: without the flag, every child
///   the supervisor forks — a managed service, or a session running as another OS user — execs
///   holding a *listening* descriptor on the root broker's privileged socket, and can `accept()` the
///   daemon's connections to it. Measured, not inferred: before this, a managed service's fd 3 was
///   the same socket inode as the supervisor's listener.
fn adopt_activation_listener(fd: std::os::fd::RawFd) -> anyhow::Result<UnixListener> {
    // SAFETY: `resolve_socket_source` returned `Activated` only because LISTEN_PID names this
    // process and LISTEN_FDS >= 1, which is systemd's guarantee that `fd` is an open listening
    // AF_UNIX socket. Ownership is taken here and the raw fd is never touched again except through
    // the listener that now owns it.
    let inherited = unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) };
    inherited
        .set_nonblocking(true)
        .map_err(|error| anyhow::anyhow!("adopted activation socket: {error}"))?;
    // SAFETY: `inherited` owns the descriptor and keeps it open for the duration of the call.
    // `F_SETFD` writes only this descriptor's flags.
    let marked = unsafe { libc::fcntl(inherited.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };
    if marked != 0 {
        return Err(anyhow::anyhow!(
            "close-on-exec on the adopted activation socket: {}",
            std::io::Error::last_os_error()
        ));
    }
    UnixListener::from_std(inherited)
        .map_err(|error| anyhow::anyhow!("adopt activation listener: {error}"))
}

/// Serve the privileged surface until the listener itself becomes unusable.
///
/// Each connection gets its own [`SupervisorServiceImpl`] carrying that connection's peer
/// credentials, and its own task — a caller that stalls mid-frame cannot hold up anyone else.
pub async fn serve(listener: UnixListener, surface: Arc<PrivilegedSurface>) {
    let slots = ConnectionSlots::new(MAX_CONCURRENT_CONNECTIONS);
    loop {
        // A slot is taken *before* accepting, not after. At the cap the next caller waits in the
        // kernel's backlog, which is backpressure; accepting first and then finding nothing to serve
        // it with would mean spending a descriptor and a task on a connection anyway.
        if slots.available() == 0 {
            log::warn!(
                target: "tddy_supervisor::server",
                "the privileged surface is serving its maximum of {MAX_CONCURRENT_CONNECTIONS} \
                 connections; no further connection is accepted until one of them finishes"
            );
        }
        let Some(slot) = slots.acquire().await else {
            log::error!(
                target: "tddy_supervisor::server",
                "the connection limiter was closed; the privileged surface is no longer accepting"
            );
            return;
        };

        let stream = match listener.accept().await {
            Ok((stream, _address)) => stream,
            Err(error) => match classify_accept_failure(&error) {
                AcceptFailure::RetryImmediately => {
                    // One caller's connection died on its way in, or a signal arrived. The listener
                    // is untouched, and the next `accept` is the one that matters.
                    log::debug!(
                        target: "tddy_supervisor::server",
                        "a connection to the privileged socket was lost before it was served: {error}"
                    );
                    continue;
                }
                AcceptFailure::RetryAfterBackoff => {
                    // A shortage, not a broken listener: retrying without pausing would spin a core
                    // for as long as it lasts, and the shortage clears when something else releases
                    // what it is holding.
                    log::warn!(
                        target: "tddy_supervisor::server",
                        "accept on the privileged socket failed for lack of a host resource, \
                         retrying in {ACCEPT_BACKOFF:?}: {error}"
                    );
                    drop(slot);
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                    continue;
                }
                AcceptFailure::Fatal => {
                    // The listener is the thing that is broken, so every later `accept` fails the
                    // same way. Retrying would be an infinite loop that logs.
                    log::error!(
                        target: "tddy_supervisor::server",
                        "the privileged socket is no longer usable, so the supervisor is no longer \
                         serving it: {error}"
                    );
                    return;
                }
            },
        };

        // A connection whose credentials cannot be read is dropped rather than served: without
        // them the first gate has nothing to decide on.
        let peer = match stream.peer_cred() {
            Ok(credentials) => PeerIdentity {
                uid: credentials.uid(),
                gid: credentials.gid(),
                pid: credentials.pid().unwrap_or(0) as u32,
            },
            Err(error) => {
                log::warn!(
                    target: "tddy_supervisor::server",
                    "dropped a connection with unreadable peer credentials: {error}"
                );
                continue;
            }
        };

        let (reader, writer) = stream.into_split();
        let service =
            SupervisorServiceServer::new(SupervisorServiceImpl::new(Arc::clone(&surface), peer));
        // The supervisor never calls back into a caller, so the client half of the endpoint is
        // dropped; the connection is request/response in one direction only.
        let (_client, endpoint) = StdioEndpoint::from_duplex(reader, writer, service);
        tokio::spawn(async move {
            endpoint.run().await;
            // Held until the connection is finished with, which is what makes the cap a cap on
            // *concurrent* connections rather than on connections ever accepted.
            drop(slot);
        });
    }
}

/// How many connections the privileged surface serves at once.
///
/// Some cap is mandatory, because authorization is per-*request*: any peer that gets past the
/// socket's group check has its connection accepted and a task of its own for as long as it stays
/// connected, before it has asked for anything. Unbounded, a loop of `connect()` calls costs the
/// supervisor a descriptor and a task each until it runs out of descriptors.
///
/// 64 rather than a number fitted to a workload: the peers are the handful of processes that own a
/// declared service, and [`crate::SupervisorClient`] holds *one* connection per client for a burst of
/// requests. Sixty-four is far more than the daemon has ever needed at once, and small enough that
/// the surface cannot be made to exhaust the process's descriptors by connecting to it.
const MAX_CONCURRENT_CONNECTIONS: usize = 64;

/// How long the accept loop waits out a host resource shortage before trying again.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// Permits for the connections being served, one per connection, held for its lifetime.
#[derive(Debug, Clone)]
struct ConnectionSlots {
    permits: Arc<tokio::sync::Semaphore>,
}

impl ConnectionSlots {
    fn new(limit: usize) -> ConnectionSlots {
        ConnectionSlots {
            permits: Arc::new(tokio::sync::Semaphore::new(limit)),
        }
    }

    /// Wait for a free slot. `None` only if the limiter was closed, which nothing does.
    async fn acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        Arc::clone(&self.permits).acquire_owned().await.ok()
    }

    fn available(&self) -> usize {
        self.permits.available_permits()
    }
}

/// What an `accept` failure says about whether the listener is still worth using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcceptFailure {
    /// One inbound connection was lost, or a signal interrupted the call. The listener is fine.
    RetryImmediately,
    /// The host has run out of descriptors, memory or buffers. The listener is fine, but retrying
    /// without pausing would spin.
    RetryAfterBackoff,
    /// The listener itself is unusable, so every later `accept` fails identically.
    Fatal,
}

/// Decide what an `accept` failure means. Pure, so every branch is assertable.
///
/// The distinction is the whole point: treating a transient failure as fatal destroys the privileged
/// surface for the life of the process — its `JoinHandle` is only ever `abort`ed, so nothing restarts
/// it, and systemd goes on reporting the unit `active` — while treating a fatal one as transient turns
/// the accept loop into a spin.
fn classify_accept_failure(error: &std::io::Error) -> AcceptFailure {
    match error.raw_os_error() {
        Some(libc::EMFILE | libc::ENFILE | libc::ENOBUFS | libc::ENOMEM) => {
            AcceptFailure::RetryAfterBackoff
        }
        Some(libc::ECONNABORTED | libc::EINTR | libc::EAGAIN) => AcceptFailure::RetryImmediately,
        // Anything else — `EBADF`, `EINVAL`, `ENOTSOCK`, or an errno this does not know — is treated
        // as a broken listener. Unknown failures fail closed: stopping is loud and bounded, whereas
        // retrying an error whose cause is unknown is a loop nothing breaks out of.
        _ => AcceptFailure::Fatal,
    }
}

/// Give the socket the group and mode the config asks for.
///
/// The socket stays owned by root; the group is the entire grant, which is why a missing group is
/// an error rather than something to shrug at.
fn apply_socket_ownership(
    path: &std::path::Path,
    group: Option<&str>,
    mode: u32,
) -> anyhow::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    if let Some(group) = group {
        let gid = spawn_broker::resolve_group_gid(group)
            .map_err(|error| anyhow::anyhow!("socket group `{group}`: {error}"))?;
        let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| anyhow::anyhow!("socket path contains a nul byte"))?;
        // SAFETY: `c_path` outlives the call; `-1` leaves the owning uid untouched.
        let changed = unsafe { libc::chown(c_path.as_ptr(), u32::MAX, gid) };
        if changed != 0 {
            return Err(anyhow::anyhow!(
                "chown {} to group `{group}`: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
    }

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| anyhow::anyhow!("chmod {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    // -----------------------------------------------------------------------------------------
    // Adopting a listener systemd handed over
    // -----------------------------------------------------------------------------------------

    /// A listening socket on a descriptor that behaves like an activation one: open, and with
    /// `FD_CLOEXEC` *clear*, which is how systemd passes them and how `dup` hands them back.
    fn a_handed_over_listener() -> (tempfile::TempDir, std::os::fd::RawFd) {
        let directory = tempfile::TempDir::new().expect("create a socket directory");
        let listener = std::os::unix::net::UnixListener::bind(directory.path().join("handed.sock"))
            .expect("bind a listener to hand over");
        // SAFETY: `listener` is open for the duration of the call. `dup` returns a descriptor nothing
        // else owns, with the close-on-exec flag clear.
        let duplicate = unsafe { libc::dup(listener.as_raw_fd()) };
        assert!(duplicate >= 0, "dup the listener");
        assert!(
            !close_on_exec(duplicate),
            "an activation descriptor arrives with the flag clear; this fixture must too"
        );
        (directory, duplicate)
    }

    fn close_on_exec(fd: std::os::fd::RawFd) -> bool {
        // SAFETY: `F_GETFD` only reads the flags of a descriptor this process owns.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert!(flags >= 0, "read the flags of fd {fd}");
        flags & libc::FD_CLOEXEC != 0
    }

    #[tokio::test]
    async fn marks_an_adopted_activation_listener_close_on_exec() {
        // Given
        let (_directory, handed_over) = a_handed_over_listener();

        // When
        let listener = adopt_activation_listener(handed_over).expect("adopt the listener");

        // Then — without the flag, every child the supervisor forks execs holding a *listening*
        // descriptor on the root broker's socket, and can accept the daemon's connections to it.
        assert!(
            close_on_exec(listener.as_raw_fd()),
            "the adopted listener must not survive a child's exec"
        );
    }

    #[tokio::test]
    async fn makes_an_adopted_activation_listener_non_blocking() {
        // Given
        let (_directory, handed_over) = a_handed_over_listener();

        // When
        let listener = adopt_activation_listener(handed_over).expect("adopt the listener");

        // Then — tokio drives it, and a blocking accept would stall the runtime instead.
        // SAFETY: `F_GETFL` only reads the flags of a descriptor this process owns.
        let flags = unsafe { libc::fcntl(listener.as_raw_fd(), libc::F_GETFL) };
        assert_eq!(flags & libc::O_NONBLOCK, libc::O_NONBLOCK);
    }

    // -----------------------------------------------------------------------------------------
    // Surviving an accept failure
    // -----------------------------------------------------------------------------------------

    #[rstest]
    #[case::connection_aborted(libc::ECONNABORTED)]
    #[case::interrupted_by_a_signal(libc::EINTR)]
    fn keeps_serving_after_a_failure_that_left_the_listener_intact(#[case] errno: i32) {
        // Given
        let error = std::io::Error::from_raw_os_error(errno);

        // When
        let failure = classify_accept_failure(&error);

        // Then — one caller's connection is gone; the surface is not.
        assert_eq!(failure, AcceptFailure::RetryImmediately);
    }

    #[rstest]
    #[case::out_of_process_descriptors(libc::EMFILE)]
    #[case::out_of_host_descriptors(libc::ENFILE)]
    #[case::out_of_buffers(libc::ENOBUFS)]
    #[case::out_of_memory(libc::ENOMEM)]
    fn waits_before_serving_again_when_the_host_has_run_out_of_something(#[case] errno: i32) {
        // Given
        let error = std::io::Error::from_raw_os_error(errno);

        // When
        let failure = classify_accept_failure(&error);

        // Then — the listener is fine and the shortage will pass, but retrying without pausing would
        // spin a core until it does.
        assert_eq!(failure, AcceptFailure::RetryAfterBackoff);
    }

    #[rstest]
    #[case::not_a_descriptor(libc::EBADF)]
    #[case::not_a_socket(libc::ENOTSOCK)]
    #[case::not_listening(libc::EINVAL)]
    fn stops_serving_a_listener_that_can_never_accept_again(#[case] errno: i32) {
        // Given
        let error = std::io::Error::from_raw_os_error(errno);

        // When
        let failure = classify_accept_failure(&error);

        // Then — every later `accept` would fail identically, so retrying is a loop that only logs.
        assert_eq!(failure, AcceptFailure::Fatal);
    }

    #[test]
    fn treats_an_accept_failure_it_does_not_recognise_as_fatal() {
        // Given an errno this classification says nothing about.
        let error = std::io::Error::from_raw_os_error(libc::EDQUOT);

        // When
        let failure = classify_accept_failure(&error);

        // Then — failing closed. Stopping is loud and bounded; retrying a failure whose cause is
        // unknown is a loop nothing breaks out of.
        assert_eq!(failure, AcceptFailure::Fatal);
    }

    // -----------------------------------------------------------------------------------------
    // Bounding concurrent connections
    // -----------------------------------------------------------------------------------------

    #[tokio::test]
    async fn serves_no_more_connections_at_once_than_the_bound_allows() {
        // Given
        let slots = ConnectionSlots::new(2);

        // When
        let _first = slots.acquire().await.expect("a free connection slot");
        let _second = slots.acquire().await.expect("a free connection slot");

        // Then — the next caller waits in the kernel's backlog instead of costing a descriptor and a
        // task, which is what stops a loop of `connect()` calls exhausting the process.
        assert_eq!(slots.available(), 0);
    }

    #[tokio::test]
    async fn serves_another_connection_once_an_open_one_finishes() {
        // Given
        let slots = ConnectionSlots::new(2);
        let first = slots.acquire().await.expect("a free connection slot");
        let _second = slots.acquire().await.expect("a free connection slot");

        // When
        drop(first);

        // Then — the bound is on connections being served, not on connections ever accepted.
        assert_eq!(slots.available(), 1);
    }
}
