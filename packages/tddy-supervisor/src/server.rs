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

pub(crate) mod listener;
mod rpc;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::authz::Authorizer;
use crate::cgroup_broker::CgroupBroker;
use crate::clone::{self, CloneEvents, RunningClone};
use crate::config::SupervisorConfig;
use crate::error::SupervisorError;
use crate::policy;
use crate::proto::supervisor::{self as wire, SupervisorServiceServer};
use crate::request::{
    CreateScopeRequest, ScopeHandle, SessionStatus, SpawnRelayedRequest, SpawnSandboxRequest,
    SpawnSessionRequest, SpawnedProcess,
};
use crate::server::rpc::SupervisorServiceImpl;
use crate::spawn_broker::accounts;
use crate::spawn_broker::accounts::TargetUser;
use crate::spawn_broker::{ChildOutput, EnvironmentBase, JailMount, SandboxJail, SpawnPlan};
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

    /// Spawn a coding-agent harness as an allowlisted user with its stdio relayed to the caller.
    ///
    /// The same user, environment and working-directory gates as [`Self::spawn_session`], plus the
    /// relay's own allowlist. The caller's socket is connected to **before** the fork, so a socket
    /// that is not the caller's costs nothing and spawns nothing.
    async fn spawn_relayed(
        &self,
        request: SpawnRelayedRequest,
        caller_uid: u32,
    ) -> Result<SpawnedProcess, SupervisorError> {
        let policy = &self.config.spawn_policy;
        // The relay allowlist first: with it empty (the default) nothing below runs at all.
        let program = policy::resolve_relay_tool_path(policy, &request.tool_path)?;
        // `resolve_spawn_of`, not `resolve_spawn`: the relay allowlist has passed the program, and
        // resolving it against `allowed_tool_paths` as well would make a relayed harness need both
        // lists — and so grant it `SpawnSession` too.
        let resolved = self.resolve_spawn_of(
            &request.os_user,
            program,
            request.working_dir.as_deref(),
            None,
            &request.env,
        )?;
        let stream =
            crate::relay::connect_to_callers_socket(&request.relay_socket, caller_uid).await?;
        let os_user = resolved.os_user.clone();
        let plan = resolved.plan(request.args, None);
        let (pid, stdio) = spawn_outcome(
            self.supervisor.spawn_session_piped(plan).await,
            "relayed session",
            &os_user,
            |(pid, _)| *pid,
        )?;
        crate::relay::relay(
            Arc::clone(&self.supervisor),
            pid,
            stream,
            stdio,
            self.shutdown_grace(),
        );
        Ok(SpawnedProcess { pid })
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
        let program = policy::resolve_tool_path(&self.config.spawn_policy, tool_path)?;
        self.resolve_spawn_of(os_user, program, working_dir, scope, env)
    }

    /// The rest of [`Self::resolve_spawn`], for a `program` an allowlist has already passed —
    /// `allowed_tool_paths` for a session, sandbox or clone, `allowed_relay_tool_paths` for a
    /// relayed spawn.
    fn resolve_spawn_of(
        &self,
        os_user: &str,
        program: PathBuf,
        working_dir: Option<&Path>,
        scope: Option<&str>,
        env: &BTreeMap<String, String>,
    ) -> Result<ResolvedSpawn, SupervisorError> {
        let policy = &self.config.spawn_policy;
        let os_user = policy::resolve_session_user(policy, os_user)?;
        // Here rather than in any caller: the environment is as much a way to choose what runs as
        // the tool path is, and a gate that only some of the spawn methods passed through would be a
        // gate the others are missing.
        let env = policy::resolve_env(policy, env)?;
        let scope_procs = match scope {
            Some(scope) => Some(self.cgroups.scope_procs_path(scope)?),
            None => None,
        };

        // An allowlisted account that does not exist on this host is refused exactly like one the
        // policy never listed. Distinguishing the two would tell the caller which names are
        // allowlisted, which is the oracle this boundary exists to close.
        let target = accounts::resolve_target_user(&os_user).map_err(|error| {
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

    /// Clone a repository as an allowlisted OS user and report on it as it runs.
    ///
    /// Everything a caller can ask is checked before git exists, in the order `SpawnSession` checks
    /// it: user, tool (git, found on the supervisor's own `PATH`, still has to be allowlisted),
    /// environment. A refusal is therefore an error on the call, not an event on the stream.
    async fn clone_repository(
        &self,
        request: wire::CloneRepositoryRequest,
    ) -> Result<CloneEvents, SupervisorError> {
        let destination = policy::plain_absolute_path(Path::new(&request.destination))?;
        let git = clone::git_on_supervisor_path()?;
        let env: BTreeMap<String, String> = request.env.into_iter().collect();
        let resolved = self.resolve_spawn_of(
            &request.os_user,
            policy::resolve_tool_path(&self.config.spawn_policy, &git)?,
            None,
            None,
            &env,
        )?;
        // The tree is already present: nothing to clone, and nothing to fork a git for.
        if destination.exists() {
            return Ok(clone::already_present());
        }
        let partial = clone::partial_sibling(destination)?;
        let os_user = resolved.os_user.clone();
        let mut plan = resolved.plan(
            vec![
                "clone".to_string(),
                "--progress".to_string(),
                // Whatever follows is a URL, even one that starts with a dash.
                "--".to_string(),
                request.git_url,
                partial.display().to_string(),
            ],
            None,
        );
        plan.output = ChildOutput::CaptureStderr;
        let (pid, stderr) = self
            .supervisor
            .spawn_session_capturing_stderr(plan)
            .await
            .map_err(|error| SupervisorError::OperationFailed {
                message: format!("spawn clone: {error}"),
            })?;
        log::info!(
            target: "tddy_supervisor::server",
            "spawned clone pid {pid} as `{os_user}`"
        );
        Ok(clone::report_on(RunningClone {
            supervisor: Arc::clone(&self.supervisor),
            pid,
            stderr,
            partial,
            destination: destination.to_path_buf(),
            stop_grace: Duration::from_secs(self.config.shutdown_grace_secs),
        }))
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
        let pid = spawn_outcome(
            self.supervisor.spawn_session(plan).await,
            kind,
            os_user,
            |pid| *pid,
        )?;
        Ok(SpawnedProcess { pid })
    }

    /// Stop a session, giving it the same grace before `SIGKILL` that the supervisor's own shutdown
    /// gives a child: how long a process gets to exit on request is a property of the host, not of
    /// who asked.
    fn stop_session(&self, pid: u32) -> Result<SessionStatus, SupervisorError> {
        self.supervisor.stop_session(pid, self.shutdown_grace())
    }

    /// How long a stopped session gets before `SIGKILL` — see [`Self::stop_session`].
    fn shutdown_grace(&self) -> Duration {
        Duration::from_secs(self.config.shutdown_grace_secs)
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

/// What the outcome of a fork is reported as, the same for every spawn method: a success is logged,
/// and a failure is reported as what it is.
///
/// In particular a jail the supervisor cannot build surfaces as `OperationFailed` carrying the
/// reason: dressing it as a denial would hide a host that cannot jail behind a policy-shaped error,
/// and spawning the session without the jail it asked for would be worse than either.
fn spawn_outcome<T>(
    outcome: std::io::Result<T>,
    kind: &str,
    os_user: &str,
    pid_of: impl FnOnce(&T) -> u32,
) -> Result<T, SupervisorError> {
    let spawned = outcome.map_err(|error| SupervisorError::OperationFailed {
        message: format!("spawn {kind}: {error}"),
    })?;
    log::info!(
        target: "tddy_supervisor::server",
        "spawned {kind} pid {} as `{os_user}`",
        pid_of(&spawned)
    );
    Ok(spawned)
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
            output: ChildOutput::Discard,
            // A session is spawned for somebody else and has no claim on the environment the
            // supervisor was started with — see [`EnvironmentBase`].
            environment: EnvironmentBase::Minimal,
            sandbox,
        }
    }
}

// TODO(restructure): every test here is about `listener`, which the move took out of this file;
// `restructure` cannot carry an inline `mod tests`, so they still live here.
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::server::listener::adopt_activation_listener;
    use crate::server::listener::classify_accept_failure;
    use crate::server::listener::AcceptFailure;
    use crate::server::listener::ConnectionSlots;
    use std::os::fd::AsRawFd;

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
