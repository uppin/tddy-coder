//! The per-session jail a **sandboxed `workspace` session** runs its tools in.
//!
//! PRD: `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox.
//!
//! `sandbox = true` on a `claude-cli` session confines the *agent* and lets the host run the tools
//! it asks for ([`crate::sandbox_session`]). A `workspace` session has no agent on this host at
//! all — it *is* the checkout — so the thing left to confine is the tool call itself. This module
//! declares that jail: where its artifacts live ([`WorkspaceSandboxLayout`]), what of the host is
//! inside it ([`build_workspace_tool_plan`]), which hosts can hold one
//! ([`workspace_sandbox_platform_support`]), and how a tool call finds the jail of the session it
//! names ([`WorkspaceSandboxRegistry`]).
//!
//! The provisioner is a trait rather than a function so the daemon's routing, refusal and ordering
//! contracts are testable without a kernel — what the jail then *confines* is proven against a real
//! one (`tests/workspace_tool_sandbox_seatbelt_acceptance.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use tddy_sandbox::{CgroupConfig, MountSpec, SandboxError, SandboxPlan};
// The budget both hosts of this exchange keep. Imported rather than restated: the daemon and the
// standalone app speak the same one-call-at-a-time protocol into the same jail, so a call still
// legitimate on one of them must not already have been abandoned on the other.
use tddy_sandbox_runner::{SessionChannelClient, IN_JAIL_TOOL_TIMEOUT};
use tddy_service::proto::exec_tools::{ExecuteToolRequest, ExecuteToolResponse};
use tddy_service::proto::sandbox::session_frame::Payload as SessionPayload;
use tddy_service::proto::sandbox::SessionFrame;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;

/// Where the jail runner records its pid so a later daemon can tear down an orphaned process.
pub const RUNNER_PID_FILE: &str = "runner.pid";

/// What became of one tool call sent into a jail.
///
/// The two cases are the two different things that can go wrong, and they call for opposite
/// answers: a tool that ran and said no is the caller's business, while a call that never reached
/// a tool leaves the jail unusable and is the daemon's.
pub enum ToolDispatchOutcome {
    /// The tool ran inside the jail and this is what it answered — including when it answered
    /// `is_error`, which is a tool result like any other and says nothing about the jail.
    Ran(ExecuteToolResponse),
    /// The call never reached a tool: the jail could not be spoken to, or did not answer. Carries
    /// what failed, already named after the session, for whoever reports or repairs it.
    ///
    /// The jail is not usable afterwards — see [`WorkspaceSandbox::execute_tool`].
    TransportFailed(String),
}

/// A live jail serving one sandboxed workspace session.
#[async_trait]
pub trait WorkspaceSandbox: Send + Sync {
    /// Run one tool call inside the jail.
    ///
    /// A tool that ran and failed answers `Ran` with `is_error`, exactly as the host tool engine
    /// does; a call that never reached a tool answers `TransportFailed`. The distinction is the
    /// jail's to draw because it is the only layer that knows which happened — and it exists for
    /// exactly one caller decision, whether to rebuild the jail: a dead jail refuses every call
    /// for the rest of the session, while a failing command is ordinary traffic on a healthy one
    /// (`docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox).
    ///
    /// It is **not** a licence to treat the two differently anywhere else. `TransportFailed` is
    /// still a failure the caller must report; the one thing it must never become is a reason to
    /// run the call on the host worktree the session was jailed away from.
    ///
    /// A jail that answers `TransportFailed` stays failed: the call is not retried in the same
    /// jail, because a channel that lost its answer would match the next response to the wrong
    /// request. Repair means replacing the jail, not reviving its channel.
    async fn execute_tool(&self, req: &ExecuteToolRequest) -> ToolDispatchOutcome;

    /// Tear the jail down. Idempotent: a jail already stopped stays stopped.
    fn stop(&self);
}

/// The session a jail is being built for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSandboxSpec {
    pub session_id: String,
    /// The session's directory on this host — the jail's own artifacts live under it, so deleting
    /// the session takes the jail's scratch, profile and egress with it.
    pub session_dir: PathBuf,
    /// The checkout the jail holds, and the only thing of this host that is inside it.
    pub worktree_path: PathBuf,
}

/// Builds the jail a sandboxed workspace session dispatches through.
#[async_trait]
pub trait WorkspaceSandboxProvisioner: Send + Sync {
    async fn provision(
        &self,
        spec: &WorkspaceSandboxSpec,
    ) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError>;
}

/// Where a workspace jail keeps its artifacts, rooted at `<session_dir>/sandbox`.
///
/// The same shape `tddy-daemon`'s `connection_service` lays out for a sandboxed `claude-cli` session, with
/// one difference: the egress directory is inside the sandbox root rather than beside it, so the
/// jail's whole writable tree is the one directory the session owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSandboxLayout {
    pub sandbox_root: PathBuf,
    /// The jail's writable scratch (its `$HOME` and `$TMPDIR` live under here).
    pub scratch_dir: PathBuf,
    /// Files handed to the jail at spawn.
    pub context_dir: PathBuf,
    /// Where the jail writes what the host reads back out of it — logs, diagnostics.
    pub egress_dir: PathBuf,
    /// Written by the runner once it is serving; the host waits for it before dispatching.
    pub ready_marker: PathBuf,
    /// The generated backend profile (SBPL on macOS).
    pub profile_path: PathBuf,
    /// Where the runner's tool-IPC socket would live. A workspace jail hosts no agent, so nothing
    /// inside it ever calls back out and nothing binds this — it is declared with the rest of the
    /// jail's tree so a jail that one day does bind one keeps it inside the session's own
    /// directory.
    pub tool_ipc_socket: PathBuf,
}

impl WorkspaceSandboxLayout {
    /// The layout for the session whose directory is `session_dir`.
    pub fn under_session_dir(session_dir: &Path) -> Self {
        let sandbox_root = session_dir.join("sandbox");
        Self {
            scratch_dir: sandbox_root.join(".work"),
            context_dir: sandbox_root.join("context"),
            egress_dir: sandbox_root.join("egress"),
            ready_marker: sandbox_root.join("sandbox.ready"),
            profile_path: sandbox_root.join("sandbox.sb"),
            tool_ipc_socket: sandbox_root.join("tool_ipc.sock"),
            sandbox_root,
        }
    }
}

/// What a workspace jail is made of, as [`build_workspace_tool_plan`] needs it.
pub struct WorkspaceToolPlanRequest {
    pub layout: WorkspaceSandboxLayout,
    pub worktree_path: PathBuf,
    pub session_id: String,
    pub runner_path: String,
    pub tddy_tools_path: String,
    pub cgroup: CgroupConfig,
}

/// Assemble the [`SandboxPlan`] for a workspace jail: `tddy-sandbox-runner --stdio` over the
/// session's own scratch tree, with the session's checkout mounted read-write **and nothing else of
/// the host**.
///
/// Read-write because the tools this jail serves are the mutating ones, and mounted at its own host
/// path (`jail: None`) because a path the daemon resolved outside the jail has to name the same
/// file inside it — a remapped root would make every tool argument mean two different things.
pub fn build_workspace_tool_plan(
    request: WorkspaceToolPlanRequest,
) -> Result<SandboxPlan, SandboxError> {
    let WorkspaceToolPlanRequest {
        layout,
        worktree_path,
        session_id,
        runner_path,
        tddy_tools_path,
        cgroup,
    } = request;

    let scratch_home = layout.scratch_dir.join("home");
    let scratch_tmp = layout.scratch_dir.join("tmp");

    // `--workspace-tools` is what makes this a jail with no agent in it: the runner serves the
    // host's `in_jail_tool_request`s against the worktree as mounted here, and spawns no PTY, no
    // in-jail `tddy-tools --mcp` server and no egress shim for one to reach the network through.
    let runner_argv = vec![
        runner_path,
        "--session-id".to_string(),
        session_id.clone(),
        "--context-dir".to_string(),
        layout.context_dir.to_string_lossy().to_string(),
        "--tool-ipc-socket".to_string(),
        layout.tool_ipc_socket.to_string_lossy().to_string(),
        "--tddy-tools-path".to_string(),
        tddy_tools_path,
        "--ready-marker".to_string(),
        layout.ready_marker.to_string_lossy().to_string(),
        "--workspace-tools".to_string(),
        worktree_path.to_string_lossy().to_string(),
        "--stdio".to_string(),
    ];

    let mut env = tddy_sandbox::scratch_runner_env(
        &scratch_home,
        &scratch_tmp,
        &session_id,
        &layout.tool_ipc_socket,
        &layout.egress_dir,
    );
    // FIXME(grep-in-jail): see `host_ripgrep_dir`. Both halves are needed and neither is enough on
    // its own — PATH so `Command::new("rg")` resolves, and the exec-marked read below so Seatbelt
    // lets the loader read and run what it resolved.
    let ripgrep_dir = host_ripgrep_dir();
    if let Some(dir) = &ripgrep_dir {
        if let Some(path) = env.get_mut("PATH") {
            *path = format!("{}:{path}", dir.display());
        }
    }

    let plan_worktree = worktree_path.clone();
    let mut plan =
        tddy_sandbox_recipes::build_runner_plan(tddy_sandbox_recipes::RunnerPlanRequest {
            project_root: layout.sandbox_root.clone(),
            scratch_dir: layout.scratch_dir.clone(),
            egress_dir: layout.egress_dir.clone(),
            profile_path: layout.profile_path.clone(),
            runner_argv,
            env,
            loopback_allow_ports: vec![],
            // No host-bound tool IPC: that socket exists for an in-jail `tddy-tools --mcp` calling
            // *out* to the host, which a jail with no agent in it never does. Declaring one would
            // also have to fit macOS's 104-byte `SUN_LEN` cap, which a path this deep under the
            // session directory does not.
            ipc_socket: None,
            // The boundary the feature sells: one mount, the session's checkout.
            mounts: vec![MountSpec::read_write(worktree_path)],
            // The jail runs the session's tools, `Shell` among them — the recipe that grants a
            // subprocess and a PTY, and nothing an agent runtime would need.
            recipe: Some(tddy_sandbox_recipes::SandboxRecipe::Shell),
            host_home: None,
        })?;
    // Path lookup for the worktree itself. Resolving `/a/b/worktree` walks `/a`, then `/a/b`, and
    // a jail that cannot stat those cannot canonicalize its own checkout — which is the first
    // thing every path-containing tool does. That lookup is all each ancestor is granted: a
    // `metadata` read, never a `literal` or a `subpath`. The distinction is not academic here —
    // `<tddyhome>/sessions` is one of these ancestors and its entries are the other live sessions
    // on this host, so an ordinary `file-read*` grant would let one session's jail enumerate its
    // neighbours by id. The only host tree it reaches into is still the worktree.
    for ancestor in worktree_ancestors(&plan_worktree) {
        plan.reads.push(tddy_sandbox::ReadSpec::metadata(
            ancestor,
            tddy_sandbox::ReadReason::Custom,
        ));
    }
    // The Shell recipe describes the tools this jail serves, but the process serving them is the
    // Rust runner, whose runtime reads a sysctl to size the guard page under its main stack before
    // `main()` runs. Denied, that read fails with `EINVAL` and the runtime aborts, so the jail
    // never comes up at all. Nothing else of the shell policy is relaxed.
    plan.policy.sysctl_read = true;
    // FIXME(grep-in-jail): see `host_ripgrep_dir`. `exec_paths` alone would not do — it grants
    // `process-exec*` and no read, and a binary the loader cannot read is a binary it cannot run.
    // `.executable()` is the grant that carries both.
    if let Some(dir) = ripgrep_dir {
        plan.reads.push(
            tddy_sandbox::ReadSpec::subpath(dir, tddy_sandbox::ReadReason::Toolchain).executable(),
        );
    }
    plan.cgroup = cgroup;
    Ok(plan)
}

/// The directory holding the host's `rg`, when there is one.
///
/// FIXME(grep-in-jail): temporary. `tool_grep` shells out to `ripgrep`, and the jail's PATH is
/// `/usr/bin:/bin:/usr/sbin:/sbin` while Homebrew installs `rg` under `/opt/homebrew/bin` (or
/// `/usr/local/bin` on Intel). So `Grep` fails `spawn failed: No such file or directory` on every
/// call in every sandboxed session on a Mac — silently, as one refused tool among many, which is
/// how it went unnoticed.
///
/// This grants the *one directory the host's own `rg` lives in*, resolved at plan time rather than
/// hardcoded, so it is right on Intel, Apple Silicon and Linux alike and grants nothing on a host
/// that has no `rg`. It is still the wrong shape: it widens a jail's exec surface to a directory
/// full of unrelated Homebrew binaries because of one tool's dependency. The fix is to stop
/// depending on an external binary the jail cannot be guaranteed to have — either ship `rg`
/// beside `tddy-sandbox-runner` the way `tddy-tools` already is, or implement `Grep` in-process —
/// and to make its absence a startup refusal rather than a per-call error.
///
/// Tracked in `docs/dev/todo/2026-09-26-grep-is-unreachable-inside-every-jail.md`.
fn host_ripgrep_dir() -> Option<PathBuf> {
    let output = std::process::Command::new("/usr/bin/which")
        .arg("rg")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    path.parent().map(Path::to_path_buf)
}

/// Every directory above `worktree`, from the filesystem root down to its parent.
fn worktree_ancestors(worktree: &Path) -> Vec<PathBuf> {
    worktree
        .ancestors()
        .skip(1)
        .map(Path::to_path_buf)
        .collect()
}

/// Whether this host has a sandbox backend that can hold a workspace jail.
///
/// Both backends that exist — Seatbelt and cgroups+namespaces — can. Anywhere else the answer is a
/// refusal rather than a quiet fallback to running the tools on the bare host: a session that came
/// up unconfined is indistinguishable from the one that was asked for.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn workspace_sandbox_platform_support() -> Result<(), SandboxError> {
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn workspace_sandbox_platform_support() -> Result<(), SandboxError> {
    Err(SandboxError::Unsupported {
        platform: std::env::consts::OS.to_string(),
        message: "platform sandboxes are not available on this OS".to_string(),
    })
}

/// How long the jail is given to write its ready marker before the start is failed.
const JAIL_READY_TIMEOUT: Duration = Duration::from_secs(120);

/// The production provisioner: a real jail on this host's backend.
#[derive(Debug, Default)]
pub struct JailedWorkspaceSandboxProvisioner;

#[async_trait]
impl WorkspaceSandboxProvisioner for JailedWorkspaceSandboxProvisioner {
    async fn provision(
        &self,
        spec: &WorkspaceSandboxSpec,
    ) -> Result<Arc<dyn WorkspaceSandbox>, SandboxError> {
        workspace_sandbox_platform_support()?;

        let layout = prepare_jail_tree(&spec.session_dir)?;
        // Seatbelt matches its rules against fully symlink-resolved paths (`/var` →
        // `/private/var`), so the mount the jail is built around is named the way the kernel will
        // report it.
        let worktree_path = canonical(&spec.worktree_path);
        let plan = build_workspace_tool_plan(WorkspaceToolPlanRequest {
            layout: layout.clone(),
            worktree_path: worktree_path.clone(),
            session_id: spec.session_id.clone(),
            runner_path: canonical_exec(&crate::sandbox_session::resolve_sandbox_runner_path()),
            tddy_tools_path: canonical_exec(&crate::sandbox_session::resolve_tddy_tools_path(None)),
            cgroup: CgroupConfig::default(),
        })?;

        let mut handle = crate::sandbox_session::spawn_sandbox_plan(plan)?;
        // From here on the child exists, so a failure has to take it with it: a provision that
        // answered with an error but left a jail running would leave a confined process behind for
        // a session that never came up.
        let started = start_jail_channel(&mut handle, &layout).await;
        let (out_tx, inbound) = match started {
            Ok(channel) => channel,
            Err(e) => {
                let _ = handle.child_mut().kill();
                let _ = handle.child_mut().wait();
                return Err(e);
            }
        };

        let pid = handle.pid();
        std::fs::write(layout.sandbox_root.join(RUNNER_PID_FILE), pid.to_string()).map_err(
            |e| {
                let _ = handle.child_mut().kill();
                let _ = handle.child_mut().wait();
                SandboxError::Io(format!(
                    "write runner pid under {}: {e}",
                    layout.sandbox_root.display()
                ))
            },
        )?;

        log::info!(
            target: "tddy_daemon_sandbox::workspace_tool_sandbox",
            "workspace session {} runs its tools in a jail (pid {}) holding {}",
            spec.session_id,
            pid,
            worktree_path.display()
        );

        Ok(Arc::new(JailedWorkspaceSandbox {
            session_id: spec.session_id.clone(),
            pid: handle.pid(),
            handle: StdMutex::new(Some(handle)),
            channel: Mutex::new(Some(InJailChannel { out_tx, inbound })),
        }))
    }
}

/// Wait for the spawned jail to come up and open the `SessionChannel` the host drives it over.
///
/// The jail is driven over its own piped stdio: the daemon hosts no service the runner can call
/// back into, it only sends frames within the `SessionChannel` opened here.
async fn start_jail_channel(
    handle: &mut tddy_sandbox::SandboxHandle,
    layout: &WorkspaceSandboxLayout,
) -> Result<
    (
        mpsc::Sender<SessionFrame>,
        Pin<Box<dyn Stream<Item = Result<SessionFrame, String>> + Send>>,
    ),
    SandboxError,
> {
    crate::sandbox_session::wait_for_sandbox_ready(
        handle,
        &layout.ready_marker,
        JAIL_READY_TIMEOUT,
        &layout.egress_dir,
    )
    .await
    .map_err(SandboxError::Io)?;

    let (client, _endpoint) = crate::sandbox_session::bridge_sandbox_stdio(
        handle,
        crate::sandbox_session::NoCallbackSandboxService,
    )
    .await
    .map_err(SandboxError::Io)?;
    let (out_tx, out_rx) = mpsc::channel::<SessionFrame>(16);
    let inbound = tddy_sandbox_runner::StdioSandboxClient::new(client)
        .open_session_channel(ReceiverStream::new(out_rx))
        .await
        .map_err(SandboxError::Io)?;
    Ok((out_tx, inbound))
}

/// Create the jail's own tree and return the layout naming it in the canonical spelling the
/// kernel will report accesses under.
fn prepare_jail_tree(session_dir: &Path) -> Result<WorkspaceSandboxLayout, SandboxError> {
    let layout = WorkspaceSandboxLayout::under_session_dir(session_dir);
    for dir in [
        &layout.sandbox_root,
        &layout.scratch_dir.join("home"),
        &layout.scratch_dir.join("tmp"),
        &layout.context_dir,
        &layout.egress_dir,
    ] {
        std::fs::create_dir_all(dir).map_err(|e| {
            SandboxError::Io(format!("create workspace jail dir {}: {e}", dir.display()))
        })?;
    }
    Ok(WorkspaceSandboxLayout::under_session_dir(&canonical(
        session_dir,
    )))
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Canonicalize a binary path the jail will exec, since its read allow-list is built from the
/// symlink-resolved parent directory. A bare PATH-resolved name has no directory to resolve.
fn canonical_exec(binary: &str) -> String {
    if binary.contains('/') {
        canonical(Path::new(binary)).to_string_lossy().into_owned()
    } else {
        binary.to_string()
    }
}

/// The open `SessionChannel` to one jail.
///
/// One tool call is outstanding at a time: `in_jail_tool_response` carries no request id, so the
/// answer belongs to the request the sender is holding the lock for. Holding the whole exchange
/// under one lock is what makes that true.
struct InJailChannel {
    out_tx: mpsc::Sender<SessionFrame>,
    inbound: Pin<Box<dyn Stream<Item = Result<SessionFrame, String>> + Send>>,
}

/// A live jail on this host, serving one sandboxed workspace session's tools.
struct JailedWorkspaceSandbox {
    session_id: String,
    pid: u32,
    /// Kept so the jail can be killed and reaped rather than left behind.
    handle: StdMutex<Option<tddy_sandbox::SandboxHandle>>,
    /// `None` once the channel is gone: a jail whose channel broke stays broken, because the only
    /// alternative to answering from inside it is answering from the host it was built to avoid.
    channel: Mutex<Option<InJailChannel>>,
}

#[async_trait]
impl WorkspaceSandbox for JailedWorkspaceSandbox {
    async fn execute_tool(&self, req: &ExecuteToolRequest) -> ToolDispatchOutcome {
        // The jail runs this session's own tools and authenticates nothing, so the caller's
        // session token stays on the host rather than crossing into the jail with the call.
        let request = ExecuteToolRequest {
            session_token: String::new(),
            daemon_instance_id: String::new(),
            session_id: self.session_id.clone(),
            tool_name: req.tool_name.clone(),
            args_json: req.args_json.clone(),
        };

        let mut guard = self.channel.lock().await;
        let outcome = match guard.as_mut() {
            Some(channel) => exchange_in_jail_tool_call(channel, request).await,
            None => Err("its channel is closed".to_string()),
        };
        match outcome {
            Ok(response) => ToolDispatchOutcome::Ran(response),
            Err(reason) => {
                // A channel that lost its answer cannot be reused: the next response would be
                // matched to the wrong request. Replacing this jail is the only repair, and it is
                // the dispatching caller's to make — this one reports what happened and stays
                // dead.
                *guard = None;
                let message = format!(
                    "session {}: the tool call could not be run in its jail ({reason})",
                    self.session_id
                );
                log::warn!(target: "tddy_daemon_sandbox::workspace_tool_sandbox", "{message}");
                ToolDispatchOutcome::TransportFailed(message)
            }
        }
    }

    fn stop(&self) {
        // A poisoned lock is a thread that panicked while holding the handle, not a reason to
        // leave the jail running: `stop` is reached from `Drop` and from the shutdown sweep, and
        // panicking here would orphan a `tddy-sandbox-runner` onto the host — the very thing this
        // method exists to prevent. The handle behind the poison is still the child to reap.
        let taken = self
            .handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(mut handle) = taken {
            let _ = handle.child_mut().kill();
            let _ = handle.child_mut().wait();
        } else {
            crate::sandbox_session::terminate_sandbox_process(self.pid);
        }
    }
}

impl Drop for JailedWorkspaceSandbox {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Send one tool call into the jail and wait for its answer.
async fn exchange_in_jail_tool_call(
    channel: &mut InJailChannel,
    request: ExecuteToolRequest,
) -> Result<ExecuteToolResponse, String> {
    let frame = SessionFrame {
        payload: Some(SessionPayload::InJailToolRequest(request)),
    };
    channel
        .out_tx
        .send(frame)
        .await
        .map_err(|_| "the jail is no longer reading its session channel".to_string())?;

    loop {
        match tokio::time::timeout(IN_JAIL_TOOL_TIMEOUT, channel.inbound.next()).await {
            Ok(Some(Ok(frame))) => match frame.payload {
                Some(SessionPayload::InJailToolResponse(response)) => return Ok(response),
                // A workspace jail sends nothing else, but a frame that is not the answer is
                // skipped rather than mistaken for one.
                _ => continue,
            },
            Ok(Some(Err(e))) => return Err(format!("its session channel failed: {e}")),
            Ok(None) => return Err("its session channel ended".to_string()),
            Err(_) => {
                return Err(format!(
                    "it did not answer within {}s",
                    IN_JAIL_TOOL_TIMEOUT.as_secs()
                ))
            }
        }
    }
}

/// The workspace jails this daemon holds, keyed by the session each one serves.
#[derive(Default)]
pub struct WorkspaceSandboxRegistry {
    inner: Mutex<HashMap<String, Arc<dyn WorkspaceSandbox>>>,
}

impl WorkspaceSandboxRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub async fn insert(&self, session_id: String, sandbox: Arc<dyn WorkspaceSandbox>) {
        self.inner.lock().await.insert(session_id, sandbox);
    }

    pub async fn get(&self, session_id: &str) -> Option<Arc<dyn WorkspaceSandbox>> {
        self.inner.lock().await.get(session_id).cloned()
    }

    pub async fn remove(&self, session_id: &str) -> Option<Arc<dyn WorkspaceSandbox>> {
        self.inner.lock().await.remove(session_id)
    }

    /// Stop every jail this registry holds and forget them, answering with the session ids that
    /// were stopped.
    ///
    /// The shutdown counterpart of [`Self::remove`]: a jail is a child process of this daemon, so
    /// a daemon that exits without reaching here leaves a `tddy-sandbox-runner` orphaned onto the
    /// host (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`).
    /// The map is drained under the lock and the jails stopped after it is released, so a
    /// concurrent `insert` racing the shutdown adds to an empty map rather than blocking on one.
    ///
    /// Draining first is also what makes a per-jail failure unrecoverable by anyone else: the
    /// jails are out of the registry before the first one is stopped, so nothing can reach the
    /// ones after it. The sweep therefore carries on past a failure and answers with the ids it
    /// actually stopped, rather than reporting the whole drained set as though it had.
    ///
    /// Each stop runs on the blocking pool because it is one: [`WorkspaceSandbox::stop`] kills
    /// its runner and `wait`s for it, and the fallback path sleeps between SIGTERM and SIGKILL.
    /// Run inline, a shutdown sweep would block a tokio worker for as long as every jail takes.
    pub async fn stop_all(&self) -> Vec<String> {
        let draining: Vec<(String, Arc<dyn WorkspaceSandbox>)> =
            self.inner.lock().await.drain().collect();
        let mut stopped = Vec::with_capacity(draining.len());
        for (session_id, jail) in draining {
            match tokio::task::spawn_blocking(move || jail.stop()).await {
                Ok(()) => stopped.push(session_id),
                Err(e) => log::error!(
                    target: "tddy_daemon_sandbox::workspace_tool_sandbox",
                    "session {session_id}: its jail could not be stopped ({e}); its runner may be \
                     orphaned onto this host, and the remaining jails are stopped regardless"
                ),
            }
        }
        log::info!(
            target: "tddy_daemon_sandbox::workspace_tool_sandbox",
            "stopped {} workspace jail(s): {}",
            stopped.len(),
            stopped.join(", ")
        );
        stopped
    }
}

/// Stop every jail still held when the registry itself goes away.
///
/// [`Self::stop_all`] is the *announced* shutdown, reached from the daemon's SIGTERM path. This is
/// the one nobody announces: a daemon dropped without that sweep — every test harness that builds
/// one and lets it fall out of scope — would otherwise leave each jail's `tddy-sandbox-runner`
/// orphaned onto the host, reparented to `launchd` in its own process group, where killing the
/// test binary never reaps it.
///
/// `Mutex::get_mut` needs no lock (`&mut self` already proves exclusivity) and no `await`, which
/// is what makes this expressible in `Drop` at all; [`WorkspaceSandbox::stop`] is sync for the
/// same reason.
impl Drop for WorkspaceSandboxRegistry {
    fn drop(&mut self) {
        for (_, jail) in self.inner.get_mut().drain() {
            jail.stop();
        }
    }
}
