//! The jail half of a `sandboxed` session: the checkout inside a no-agent Seatbelt jail, and the
//! socket through which the host-run agent reaches it.
//!
//! PRD: `docs/ft/coder/sandboxed-codebase-mode.md`.
//!
//! `spawn::spawn_claude_sandbox` builds the other placement — an agent in the jail, its tool calls
//! relayed out to the host. This builds the inversion, and reuses every piece of that machinery
//! that is about the jail rather than about the agent: the same session tree, the same runner env,
//! the same loopback ports, the same ready marker, the same diagnostics. What changes is what is
//! inside: `tddy-sandbox-runner --workspace-tools <repo>` serving `in_jail_tool_request` against
//! the mounted checkout, with no PTY, no in-jail agent and no in-jail MCP server.
//!
//! Three things are wired here and the mode is nothing without any of them:
//!
//! 1. the jail, holding the checkout and running the build under the kernel's rules;
//! 2. the host relay, which both fulfils the jail's CONNECT tunnels (so a jailed `cargo build`
//!    reaches crates.io through this host's socket) and carries the tool calls the other way —
//!    which is why the dispatcher has to live *on* the relay rather than beside it;
//! 3. a Unix socket on this host speaking `connection.ConnectionService/ExecuteTool`, whose every
//!    call is forwarded into the jail. This is the mirror image of the in-jail tool-IPC server
//!    (`tddy_sandbox_runner::runner::start_tool_ipc_server`): same wire, opposite direction, and
//!    `tddy-tools --mcp` cannot tell which end it is talking to — nor should it.

use std::future::Future;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use bytes::Bytes;
use prost::Message;
use tddy_daemon::sandbox_session::{
    build_sandbox_plan, build_sandbox_runner_env, pick_free_loopback_port,
    resolve_sandbox_runner_path, resolve_tddy_tools_path, spawn_sandbox_plan,
    terminate_sandbox_process, wait_for_sandbox_ready, SandboxRunnerSpawn,
};
use tddy_sandbox::{ReadReason, ReadSpec, SandboxHandle};
use tddy_sandbox_runner::{
    run_host_relay_with_in_jail_tools, HostRelayConfig, InJailToolDispatcher, NullToolHandler,
};
use tddy_service::proto::connection::ExecuteToolRequest;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::codebase_mode::CodebaseMode;
use crate::spawn::{
    build_sandbox_mounts, build_workspace_tools_runner_argv, canonicalize_exec_path, spawn_trace,
    spawn_trace_quietly, WorkspaceToolsRunnerArgs,
};

/// How long the jail is given to write its ready marker. The same budget
/// `spawn::spawn_claude_sandbox` allows, and for the same reason: a cold `sandbox-exec` on a busy
/// machine is slow, and the alternative to waiting is failing a session that would have come up.
const JAIL_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// How long the tool-IPC listener waits before retrying the *first* `accept` that failed.
///
/// An accept error is almost always fd pressure — a large parallel build inside the jail can push
/// this process to `EMFILE`/`ENFILE` — and it is transient. Retrying it immediately turns that into
/// a silent hot loop on a core; waiting first costs a session nothing it can measure, since the
/// caller on the other side is about to retry its connect anyway.
const ACCEPT_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(100);

/// The longest the listener will wait between retries, however long the pressure lasts.
///
/// The cause is *sustained* — a build holding thousands of descriptors does not release them
/// between one `accept` and the next — so a fixed short delay would keep the retry (and its
/// report) running at ten a second for as long as the build takes. Backing off to here bounds
/// that, and bounds nothing else: the caller's connect retries are what the session actually
/// waits on, and half a minute of blocked tool calls is already a session in trouble.
const ACCEPT_RETRY_MAX_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

/// What the tool-IPC listener does about an `accept` that failed, given how many in a row have.
#[derive(Debug, PartialEq, Eq)]
struct AcceptRetry {
    /// How long to wait before trying again.
    delay: std::time::Duration,
    /// Whether this failure is written to the session's trace.
    report: bool,
}

/// The listener's answer to a failed `accept`: back off, and say so only when saying so is news.
///
/// Both halves are about a condition that lasts. The **delay** doubles from
/// [`ACCEPT_RETRY_DELAY`] to [`ACCEPT_RETRY_MAX_DELAY`], so a jail that has exhausted this
/// process's descriptors is not also spending its core on a retry loop. The **report** is the
/// first failure — which is the one that tells an operator what is happening — and then nothing
/// until the backoff tops out, after which it is at most one line per
/// [`ACCEPT_RETRY_MAX_DELAY`]. The alternative, a line per failure, is not merely noisy: this
/// runs while the host agent owns the terminal, so unbounded reporting is a session whose own
/// diagnostics scribble over the UI the operator is trying to read.
///
/// A pure function of the failure count because the policy is the part worth being sure about;
/// the loop that applies it is three lines with nothing to get wrong.
fn accept_retry(consecutive_failures: u32) -> AcceptRetry {
    let doublings = consecutive_failures.saturating_sub(1).min(u32::BITS - 1);
    let delay = ACCEPT_RETRY_DELAY
        .saturating_mul(1u32 << doublings)
        .min(ACCEPT_RETRY_MAX_DELAY);
    AcceptRetry {
        report: consecutive_failures <= 1 || delay >= ACCEPT_RETRY_MAX_DELAY,
        delay,
    }
}

/// Mode for the host tool-IPC socket: owner only.
///
/// A `bind` leaves the socket `0777 & ~umask`, typically `0755` — and anyone who can connect to it
/// gets an unrestricted `ExecuteTool` into the jail, `Shell` included, with no agent in the way.
/// macOS's per-user `TMPDIR` happens to contain that today, but the socket's own mode is what makes
/// the confinement claim rather than the directory it was lucky enough to land in.
const TOOL_SOCKET_MODE: u32 = 0o600;

/// The build's `$HOME` for one repository, under the base this host keeps its build homes in.
///
/// Keyed on the checkout, because one home shared by every repository is a poisoning channel
/// pointing at the developer's own projects: a hostile build in an unaudited checkout writes
/// `$HOME/.cargo/config.toml` (`rustc-wrapper`, `source.replace-with`, `target.*.runner`) or drops
/// a binary in `$HOME/.cargo/bin`, and the *next* session's build — against a repository its owner
/// trusts — executes it. Nothing about the caching is given up: the same checkout comes back to the
/// same home every session, which is the whole reason the home outlives the session.
///
/// The key is [`tddy_core::session_actions::derive_repo_key`], the same stable, readable,
/// filesystem-safe rendering of a canonical repo path that names this host's per-repo action
/// stores — a directory an operator can still recognise, unlike a hash.
///
/// `canonical_repo` is the checkout as `canonicalize` reports it, so a repo reached by two
/// spellings — a symlink, a trailing slash — is one repository with one home rather than two.
pub fn repo_build_home(base: &Path, canonical_repo: &Path) -> PathBuf {
    base.join(tddy_core::session_actions::derive_repo_key(canonical_repo))
}

/// What a `sandboxed` session needs to provision its jail.
///
/// Deliberately smaller than [`crate::spawn::SpawnParams`]: no model, no permission mode, no agent
/// binary and no pass-through agent args, because none of them describe this jail. The agent they
/// describe runs on the host, and is configured there ([`crate::host_agent`]).
pub struct SandboxedCodebaseParams {
    /// The checkout to confine. Mounted read-write at this same path inside the jail.
    pub repo: PathBuf,
    pub session_id: String,
    /// The session's directory on this host; the jail's whole tree lives under it.
    pub session_dir: PathBuf,
    /// Path to `tddy-sandbox-runner` (default: sibling of this binary).
    pub sandbox_runner_path: Option<String>,
    /// Path to `tddy-tools` (default: sibling of this binary).
    pub tddy_tools_path: Option<String>,
    /// The jail's `$HOME` for **this one repository** — already resolved by
    /// [`repo_build_home`], never the base directory it was resolved under.
    ///
    /// The distinction is the whole of the protection. `--codebase-home-dir` (and the config's
    /// `codebase_home_dir:`) names a *base* holding one home per repository; handing that base
    /// straight to this field compiles, runs, and gives every checkout on the host one shared
    /// `$HOME` — which is exactly the cross-repo channel the keying exists to close, since a build
    /// that writes `$HOME/.cargo/config.toml` would then be writing the next session's other
    /// checkout's build config.
    ///
    /// Within one repository the home is deliberately **shared across sessions** and persistent: a
    /// build's dependency caches live under its home (`~/.cargo`, `~/.bun`), so a home discarded
    /// with the session would have every `sandboxed` session refetch them through the CONNECT
    /// relay. Two concurrent sessions against one repository ask the same question a developer's
    /// real `~/.cargo` already answers on any machine running two builds at once — cargo takes its
    /// own registry lock, and the sessions wait for each other rather than corrupt it.
    ///
    /// Required and caller-resolved, exactly like [`crate::spawn::SpawnParams::claude_home_dir`]:
    /// where a host keeps these trees is a deployment decision, and a default invented here would
    /// be a second, silent answer to it.
    pub repo_build_home: PathBuf,
}

/// A live `sandboxed` session: the jail holding the checkout, the relay driving it, and the socket
/// the host-run agent's MCP server dispatches over.
///
/// Owning all three together is what makes teardown honest — dropping this kills the jail, so a
/// session that ends never leaves a confined `sandbox-exec` child behind holding the checkout.
pub struct SandboxedCodebaseSession {
    session_id: String,
    worktree: PathBuf,
    session_dir: PathBuf,
    egress_dir: PathBuf,
    host_dir: PathBuf,
    tool_ipc_socket: PathBuf,
    egress_shim_port: u16,
    /// The jail's `sandbox-exec` child, taken by whichever of [`Self::stop`] or `Drop` runs first.
    jail: Mutex<Option<SandboxHandle>>,
    /// The relay's inbound-frame task and the tool-IPC listener. Both are pure I/O loops over the
    /// jail; once it is gone they have nothing left to do, so teardown aborts them rather than
    /// waiting for a peer that no longer exists.
    relay: JoinHandle<()>,
    tool_ipc: JoinHandle<()>,
    /// Kept alive for the session's lifetime: the relay's poll loop stops once *both* the jail and
    /// this sender are gone, and the terminal sink would otherwise report a closed channel for a
    /// jail that has no terminal to speak of in the first place.
    _stdin_tx: mpsc::UnboundedSender<Bytes>,
    _terminal_rx: mpsc::UnboundedReceiver<Bytes>,
}

impl SandboxedCodebaseSession {
    /// The Unix socket the host `tddy-tools --mcp` dispatches over (`TDDY_SANDBOX_TOOL_IPC`).
    pub fn tool_ipc_socket(&self) -> &Path {
        &self.tool_ipc_socket
    }

    /// The loopback port the jail's CONNECT egress shim listens on, so a jailed build reaches the
    /// network through this host's relay.
    pub fn egress_shim_port(&self) -> u16 {
        self.egress_shim_port
    }

    /// The checkout this jail confines, canonicalized — the path that means the same thing on both
    /// sides of the jail boundary.
    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Where the jail writes what the host reads back out of it.
    pub fn egress_dir(&self) -> &Path {
        &self.egress_dir
    }

    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }

    /// The session's host-only directory: the one tree here that the jail is granted nothing over.
    ///
    /// What lives in it is the host-run agent's own configuration — starting with the MCP config,
    /// whose `command` and `env` this host executes, unconfined, on every MCP (re)connect. Written
    /// anywhere the jail can write, that file is a shell on the host for whatever the build decides
    /// to put in it, so it goes here: a sibling of `sandbox/` and `egress/`, which are the only
    /// parts of the session tree [`build_sandbox_plan`] turns into grants.
    pub fn host_dir(&self) -> &Path {
        &self.host_dir
    }

    /// Tear the jail down. Idempotent: a session already stopped stays stopped.
    ///
    /// The whole process group is signalled, not just the `sandbox-exec` leader: the runner is its
    /// child, and killing only the leader would orphan a confined process still holding the
    /// checkout open.
    pub fn stop(&self) {
        let Some(mut handle) = self.jail.lock().unwrap().take() else {
            return;
        };
        self.relay.abort();
        self.tool_ipc.abort();
        terminate_sandbox_process(handle.pid());
        let _ = handle.child_mut().kill();
        let _ = handle.child_mut().wait();
        let _ = std::fs::remove_file(&self.tool_ipc_socket);
    }
}

impl Drop for SandboxedCodebaseSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Provisioning was interrupted before the session came up.
///
/// A distinct type rather than a message, because the caller has to tell it apart: an interrupt is
/// what the operator asked for and should exit the way an interrupted program does, while a failure
/// is a session that could not be built and should say so.
#[derive(Debug)]
pub struct ProvisioningInterrupted;

impl std::fmt::Display for ProvisioningInterrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("interrupted while bringing the jail up")
    }
}

impl std::error::Error for ProvisioningInterrupted {}

/// Bring up a `sandboxed` session, abandoning it if the operator interrupts first.
///
/// See [`provision_with_interrupt`], which this is Ctrl-C's spelling of.
pub async fn provision(params: SandboxedCodebaseParams) -> Result<SandboxedCodebaseSession> {
    provision_with_interrupt(params, async {
        // A failed handler registration is not a reason to refuse the session; it costs the
        // interrupt, and the default disposition still ends the process.
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

/// Bring up a `sandboxed` session: the jail, the relay, and the socket between them.
///
/// Ordering is load-bearing. The jail comes up first and the relay attaches to it, because the
/// relay is what fulfils the jail's CONNECT tunnels; the tool socket is bound last, because the
/// moment it exists an agent may dispatch through it and there must be a jail on the other end.
///
/// `interrupt` is the caller's cancellation — Ctrl-C, for the app. It is handled *here*, inside the
/// function that owns the jail's handle, and deliberately not by a caller racing this future: a
/// dropped [`SandboxHandle`] does not kill its child (`std::process::Child` has no such `Drop`),
/// and the child is in its own process group, so the terminal's own SIGINT never reaches it either.
/// Abandoning this future would therefore leave a jail holding the checkout read-write with an
/// egress shim on loopback, and nothing left alive that knows its pid.
pub async fn provision_with_interrupt(
    params: SandboxedCodebaseParams,
    interrupt: impl Future<Output = ()>,
) -> Result<SandboxedCodebaseSession> {
    let repo = params
        .repo
        .canonicalize()
        .with_context(|| format!("canonicalize repo {}", params.repo.display()))?;
    if !repo.is_dir() {
        anyhow::bail!("repo is not a directory: {}", repo.display());
    }

    let session_dir = params.session_dir.clone();
    std::fs::create_dir_all(&session_dir).context("create session dir")?;

    // The same tree `spawn_claude_sandbox` lays out, so a `sandboxed` session's artifacts are found
    // where every other session's are (PRD criterion 11).
    let sandbox_root = session_dir.join("sandbox");
    let egress_dir = session_dir.join("egress");
    // The one directory in the session tree the jail is granted nothing over. `build_sandbox_plan`
    // makes grants out of the plan's project root (`sandbox/`), its scratch dir (`sandbox/.work/`),
    // its egress dir (`egress/`) and its writable mounts (the checkout and the build's home) — so a
    // sibling of those, named in none of them, is reachable from the host and from nowhere inside.
    // The host-run agent's configuration lives here for that reason: see
    // [`SandboxedCodebaseSession::host_dir`].
    let host_dir = session_dir.join("host");
    // The jail's `$HOME` is the build's home, and it deliberately does *not* live in the session's
    // tree: `~/.cargo` and `~/.bun` are what a build puts there, and a home discarded with the
    // session would have the next one refetch every dependency through the CONNECT relay. The
    // agent-hosting modes point their jail home outside the session for the same reason (settings
    // and credentials have to survive a restart) — this one has no agent, so what persists here is
    // the build's caches rather than an agent's identity.
    //
    // `TMPDIR` stays inside the session tree. Scratch is exactly what should not outlive the
    // session, and keeping it there is what makes a finished session's directory the whole story.
    let build_home_dir = params.repo_build_home;
    std::fs::create_dir_all(sandbox_root.join(".work").join("tmp"))
        .context("mkdir sandbox scratch tmp")?;
    std::fs::create_dir_all(&build_home_dir)
        .with_context(|| format!("mkdir build home {}", build_home_dir.display()))?;
    std::fs::create_dir_all(sandbox_root.join("context")).context("mkdir sandbox context")?;
    std::fs::create_dir_all(&egress_dir).context("mkdir sandbox egress")?;
    std::fs::create_dir_all(&host_dir).context("mkdir the session's host-only dir")?;

    // Seatbelt matches its rules against fully symlink-resolved paths (`/var` → `/private/var`), so
    // every path the profile is built from is named the way the kernel will report it.
    let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
    let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
    let scratch_dir = sandbox_root.join(".work");
    let build_home = std::fs::canonicalize(&build_home_dir).unwrap_or(build_home_dir);
    let scratch_tmp = scratch_dir.join("tmp");
    let context_dir = sandbox_root.join("context");

    let tddy_tools_path =
        canonicalize_exec_path(&resolve_tddy_tools_path(params.tddy_tools_path.as_deref()));
    let sandbox_runner_path = params
        .sandbox_runner_path
        .as_deref()
        .map(canonicalize_exec_path)
        .unwrap_or_else(|| canonicalize_exec_path(&resolve_sandbox_runner_path()));

    let grpc_socket = sandbox_root.join("sandbox.grpc.sock");
    let ready_marker = sandbox_root.join("sandbox.ready");
    let profile_path = sandbox_root.join("sandbox.sb");
    // Declared with the rest of the jail's tree and never bound: the runner's tool-IPC socket
    // exists for an in-jail `tddy-tools --mcp` calling *out*, and this jail hosts no agent to run
    // one. The socket that matters in this mode is the host's, below.
    let jail_tool_ipc_socket = sandbox_root.join("tool_ipc.sock");
    let tool_ipc_socket = host_tool_ipc_socket_path(&params.session_id);

    let grpc_listen_port =
        pick_free_loopback_port().map_err(|e| anyhow::anyhow!("pick grpc listen port: {e}"))?;
    let egress_shim_port =
        pick_free_loopback_port().map_err(|e| anyhow::anyhow!("pick egress shim port: {e}"))?;
    // Both ends of the jail's loopback surface: the gRPC listener this host dials to drive it, and
    // the egress shim a jailed build sends its CONNECT through.
    let loopback_allow_ports = vec![grpc_listen_port, egress_shim_port];

    let runner_argv = build_workspace_tools_runner_argv(WorkspaceToolsRunnerArgs {
        sandbox_runner_path,
        session_id: params.session_id.clone(),
        repo: repo.clone(),
        context_dir,
        tool_ipc_socket: jail_tool_ipc_socket.clone(),
        tddy_tools_path,
        ready_marker: ready_marker.clone(),
        grpc_socket,
        grpc_listen_port,
        egress_shim_port,
    });

    let env = build_sandbox_runner_env(
        &build_home,
        &scratch_tmp,
        &params.session_id,
        &jail_tool_ipc_socket,
        &egress_dir,
    );

    spawn_trace(
        &session_dir,
        &format!(
            "spawning sandbox-exec → tddy-sandbox-runner --workspace-tools {} …",
            repo.display()
        ),
    );

    let mut plan = build_sandbox_plan(SandboxRunnerSpawn {
        project_root: sandbox_root.clone(),
        scratch_dir: scratch_dir.clone(),
        egress_dir: egress_dir.clone(),
        profile_path,
        runner_argv,
        env,
        loopback_allow_ports,
        // No host-bound tool IPC: that socket exists for an in-jail agent calling out, which this
        // jail has none of.
        ipc_socket: None,
        mounts: build_sandbox_mounts(CodebaseMode::Sandboxed, &repo, &build_home),
        // No per-session credential copy: the agent whose credentials it would seed is on the host,
        // using the host's own `~/.claude`.
        host_home: None,
        // Standalone app path has no daemon config; empty config lets the cgroups backend derive
        // the delegated base at runtime (ignored by the macOS backend).
        cgroup: tddy_sandbox::CgroupConfig::default(),
    })
    .map_err(|e| anyhow::anyhow!("build the jail's sandbox plan: {e}"))?;

    // Path lookup for the two trees the jail holds: the checkout, and the build's `$HOME`.
    // Resolving `/a/b/checkout` walks `/a`, then `/a/b`, and a jail that cannot stat those cannot
    // canonicalize its own checkout — which is the first thing every path-containing tool does.
    // The home needs the same for the same reason and one more: it is a *per-repository* directory
    // under a base, so its parent is a directory nothing else grants, and a `mkdir -p $HOME/.cargo`
    // — which stats each component before creating it — fails on the base rather than on anything
    // it was refused. The agent-hosting jails need none of this because their tool calls are
    // relayed out and canonicalized on the host; here the tool engine and the build both run
    // *inside*, so the lookups happen under the profile.
    //
    // Each ancestor is granted as `metadata`, the `lstat` that resolution actually needs: a
    // `literal` would render as `file-read*`, which on a directory also permits listing its
    // entries — so the jail would learn the name of every other session's tree sitting beside its
    // own checkout, and, under the build-home base, of every other repository this host has ever
    // built.
    for ancestor in repo
        .ancestors()
        .skip(1)
        .chain(build_home.ancestors().skip(1))
    {
        plan.reads
            .push(ReadSpec::metadata(ancestor, ReadReason::Custom));
    }

    let mut handle = spawn_sandbox_plan(plan).map_err(|e| {
        let logs = tddy_sandbox::format_egress_logs(&egress_dir);
        anyhow::anyhow!("spawn sandbox-runner: {e}\n{logs}")
    })?;

    // From here on the jail exists, so every failure has to take it with it: a provision that
    // answered with an error but left the jail running would leave a confined process holding the
    // checkout for a session that never came up.
    let session = finish_provisioning(
        FinishProvisioning {
            handle: &mut handle,
            ready_marker: &ready_marker,
            egress_dir: &egress_dir,
            session_dir: &session_dir,
            session_id: &params.session_id,
            worktree: &repo,
            tool_ipc_socket: &tool_ipc_socket,
            egress_shim_port,
        },
        interrupt,
    )
    .await;

    match session {
        Ok(parts) => {
            spawn_trace(
                &session_dir,
                &format!(
                    "jail ready — tool calls dispatch through {}",
                    tool_ipc_socket.display()
                ),
            );
            Ok(SandboxedCodebaseSession {
                session_id: params.session_id,
                worktree: repo,
                session_dir,
                egress_dir,
                host_dir,
                tool_ipc_socket,
                egress_shim_port,
                jail: Mutex::new(Some(handle)),
                relay: parts.relay,
                tool_ipc: parts.tool_ipc,
                _stdin_tx: parts.stdin_tx,
                _terminal_rx: parts.terminal_rx,
            })
        }
        Err(e) => {
            terminate_sandbox_process(handle.pid());
            let _ = handle.child_mut().kill();
            let _ = handle.child_mut().wait();
            crate::spawn::log_spawn_diagnostics(&egress_dir, &session_dir);
            Err(e)
        }
    }
}

/// Everything between "the jail's child exists" and "the session is usable".
struct FinishProvisioning<'a> {
    handle: &'a mut SandboxHandle,
    ready_marker: &'a Path,
    egress_dir: &'a Path,
    session_dir: &'a Path,
    session_id: &'a str,
    worktree: &'a Path,
    tool_ipc_socket: &'a Path,
    egress_shim_port: u16,
}

/// The live halves of a provisioned session, before they are handed to the session that owns them.
struct ProvisionedChannels {
    relay: JoinHandle<()>,
    tool_ipc: JoinHandle<()>,
    stdin_tx: mpsc::UnboundedSender<Bytes>,
    terminal_rx: mpsc::UnboundedReceiver<Bytes>,
}

async fn finish_provisioning(
    args: FinishProvisioning<'_>,
    interrupt: impl Future<Output = ()>,
) -> Result<ProvisionedChannels> {
    spawn_trace(
        args.session_dir,
        &format!(
            "waiting for jail ready marker (timeout {}s): {}",
            JAIL_READY_TIMEOUT.as_secs(),
            args.ready_marker.display()
        ),
    );
    // The session's one long wait, and so the whole of the window in which an operator gets bored
    // and hits Ctrl-C. Losing the race here returns an error like any other, which is the point:
    // the caller's `Err` arm already kills the jail, so an interrupt takes the same path a failed
    // spawn does instead of needing a second, hand-written teardown. Everything after this point is
    // a bounded sequence handed straight to the session that owns it. This is the same shape
    // `spawn::spawn_claude_sandbox` uses around its own ready wait.
    let ready = tokio::select! {
        ready = wait_for_sandbox_ready(
            args.handle,
            args.ready_marker,
            JAIL_READY_TIMEOUT,
            args.egress_dir,
        ) => ready.map_err(|e| {
            let logs = tddy_sandbox::format_egress_logs(args.egress_dir);
            anyhow::anyhow!("{e}\n{logs}")
        }),
        () = interrupt => {
            spawn_trace(
                args.session_dir,
                "interrupted while waiting for the jail's ready marker — tearing the jail down",
            );
            Err(ProvisioningInterrupted.into())
        }
    };
    ready?;

    let client = tddy_sandbox_darwin::connect_sandbox_client(args.ready_marker)
        .await
        .context("dial the jail's SessionChannel on loopback")?;

    let (terminal_tx, terminal_rx) = mpsc::unbounded_channel::<Bytes>();
    let (stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
    // `NullToolHandler` is not a stub here, it is the truth: a jail with no agent in it never asks
    // the host to run a tool, and a request that did arrive would be one nothing in this session
    // could have made.
    let (relay, dispatcher) = run_host_relay_with_in_jail_tools(
        client,
        NullToolHandler,
        HostRelayConfig::new(args.session_id, terminal_tx),
        stdin_rx,
    )
    .await
    .map_err(|e| anyhow::anyhow!("attach the host relay to the jail: {e}"))?;

    let tool_ipc =
        serve_host_tool_ipc(args.tool_ipc_socket, args.session_dir, Arc::new(dispatcher)).await?;

    log::info!(
        target: "tddy_sandbox_app::sandboxed_session",
        "sandboxed codebase session {} confines {} (egress shim 127.0.0.1:{}, tools via {})",
        args.session_id,
        args.worktree.display(),
        args.egress_shim_port,
        args.tool_ipc_socket.display()
    );

    Ok(ProvisionedChannels {
        relay,
        tool_ipc,
        stdin_tx,
        terminal_rx,
    })
}

/// Where the host binds the socket the agent's MCP server dispatches over.
///
/// In the host's temp directory rather than under the session directory, because an `AF_UNIX` path
/// is capped at 104 bytes on macOS (`SUN_LEN`) and a session directory — nested under `~/.tddy`,
/// or under a test's temp directory — routinely exceeds that once a session id and a file name are
/// appended. The name carries the *tail* of the session id, which in a UUIDv7 is its random half:
/// the leading half is a millisecond timestamp, so two sessions started close together would
/// otherwise pick the same path and the second would dispatch into the first one's jail.
fn host_tool_ipc_socket_path(session_id: &str) -> PathBuf {
    let tmp = std::fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    let mut tail: Vec<char> = session_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .rev()
        .take(12)
        .collect();
    tail.reverse();
    let tail: String = tail.into_iter().collect();
    tmp.join(format!("tddy-app-{tail}.sock"))
}

/// Serve `connection.ConnectionService/ExecuteTool` on `path`, forwarding every call into the jail.
///
/// The mirror image of `tddy_sandbox_runner::runner::start_tool_ipc_server`: the same
/// length-prefixed `tddy-stdio` framing over the same Unix socket, so `tddy-tools`'
/// `SessionToolTransport::SandboxIpc` reaches this without knowing it — from its side the only
/// thing that changed is which direction the socket relays.
async fn serve_host_tool_ipc(
    path: &Path,
    session_dir: &Path,
    jail: Arc<InJailToolDispatcher>,
) -> Result<JoinHandle<()>> {
    // A stale socket file from a previous run of this session id would make `bind` fail with
    // `EADDRINUSE`; nothing is listening on it, since the process that did is gone.
    let _ = std::fs::remove_file(path);
    let listener = tokio::net::UnixListener::bind(path)
        .with_context(|| format!("bind the host tool IPC socket at {}", path.display()))?;
    // Narrowed the moment it exists, before a single `accept`: between `bind` and this the socket
    // carries the process umask, and the window is the only one in which another account on the
    // host could have dialled it. A session that cannot narrow its own socket is a session whose
    // confinement claim is not true, so this is an error rather than a warning.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(TOOL_SOCKET_MODE))
        .with_context(|| format!("restrict the host tool IPC socket at {}", path.display()))?;

    let session_dir = session_dir.to_path_buf();
    Ok(tokio::spawn(async move {
        let mut consecutive_failures: u32 = 0;
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(accepted) => {
                    consecutive_failures = 0;
                    accepted
                }
                Err(e) => {
                    consecutive_failures += 1;
                    let retry = accept_retry(consecutive_failures);
                    if retry.report {
                        spawn_trace_quietly(
                            &session_dir,
                            &format!(
                                "the host tool IPC socket could not accept a caller ({e}); \
                                 failure {consecutive_failures} in a row, retrying in {:?}",
                                retry.delay
                            ),
                        );
                    }
                    tokio::time::sleep(retry.delay).await;
                    continue;
                }
            };
            let jail = Arc::clone(&jail);
            // One connection per caller, which is what `tddy-tools` opens: each MCP tool call dials
            // the socket, dispatches, and drops it.
            tokio::spawn(async move {
                let (read_half, write_half) = tokio::io::split(stream);
                let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_duplex(
                    read_half,
                    write_half,
                    HostToolIpcService { jail },
                );
                endpoint.run().await;
            });
        }
    }))
}

/// Answers the host `tddy-tools --mcp` server's `ExecuteTool` calls by running them in the jail.
struct HostToolIpcService {
    jail: Arc<InJailToolDispatcher>,
}

#[async_trait::async_trait]
impl tddy_rpc::RpcService for HostToolIpcService {
    async fn handle_rpc(
        &self,
        service: &str,
        method: &str,
        message: &tddy_rpc::RpcMessage,
    ) -> tddy_rpc::RpcResult {
        if service != "connection.ConnectionService" || method != "ExecuteTool" {
            // The roster and conversation RPCs the in-jail server forwards to a facilitating daemon
            // have no counterpart here: there is no daemon in this session, and the specialized
            // subagents that would use them are refused where the session is configured.
            return tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::not_found(format!(
                "a sandboxed codebase session serves only ExecuteTool, got {service}/{method}"
            ))));
        }
        let request = match ExecuteToolRequest::decode(message.payload.as_ref()) {
            Ok(request) => request,
            Err(e) => {
                return tddy_rpc::RpcResult::Unary(Err(tddy_rpc::Status::invalid_argument(
                    format!("decode ExecuteToolRequest: {e}"),
                )));
            }
        };
        let response = self.jail.execute(request).await;
        tddy_rpc::RpcResult::Unary(Ok(response.encode_to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── The socket the host-run agent dispatches over ──────────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (criterion 8)
    // Changeset: docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md

    /// macOS caps an `AF_UNIX` path at 104 bytes, and a session directory is already most of that.
    /// A socket the kernel refuses to bind is a session that cannot start at all.
    #[test]
    fn the_host_tool_socket_path_fits_the_unix_socket_length_limit() {
        // Given
        let session_id = "019d1e2f-3456-7890-abcd-ef0123456789";

        // When
        let path = host_tool_ipc_socket_path(session_id);

        // Then
        assert!(
            path.as_os_str().len() < 104,
            "the socket path must fit SUN_LEN; it was {} bytes: {}",
            path.as_os_str().len(),
            path.display()
        );
    }

    // ─── What the listener does about an accept it could not serve ──────────────
    //
    // The condition is fd exhaustion under a large jailed build (`EMFILE`/`ENFILE`), which is
    // sustained rather than momentary — and this loop runs while the host agent owns the terminal.

    /// The first failure is the one that tells an operator what is happening, so it is never
    /// swallowed: a socket that stopped accepting callers is the agent's whole tool surface gone.
    #[test]
    fn the_first_failed_accept_is_reported() {
        // Given / When
        let retry = accept_retry(1);

        // Then
        assert!(retry.report);
    }

    /// The failures behind it are the same failure. Reporting each one would put ten lines a
    /// second onto the terminal the agent is drawing its own UI on, for as long as the build that
    /// caused it runs.
    #[test]
    fn the_failures_behind_the_first_one_do_not_repeat_it() {
        // Given / When
        let repeats: Vec<bool> = (2..=5).map(|n| accept_retry(n).report).collect();

        // Then
        assert_eq!(repeats, vec![false, false, false, false]);
    }

    /// Suppressed is not silent, though: once the backoff has topped out the listener says so
    /// again, so a session wedged for minutes is not a session that reported one line and stopped.
    #[test]
    fn a_failure_that_outlasts_the_backoff_is_reported_again() {
        // Given — enough consecutive failures to reach the ceiling
        let at_the_ceiling = failures_to_reach_the_ceiling();

        // When
        let retry = accept_retry(at_the_ceiling);

        // Then
        assert!(retry.report);
    }

    /// …and no more often than the ceiling itself, which is what bounds the reporting rate.
    #[test]
    fn a_sustained_failure_is_reported_no_faster_than_the_ceiling() {
        // Given / When
        let retry = accept_retry(failures_to_reach_the_ceiling() + 1);

        // Then
        assert_eq!(retry.delay, ACCEPT_RETRY_MAX_DELAY);
    }

    /// Retrying at a fixed short delay spends a core on a condition that will not clear until the
    /// build releases its descriptors, so each wait is longer than the one before it.
    #[test]
    fn each_failed_accept_waits_longer_than_the_one_before() {
        // Given / When
        let waits: Vec<std::time::Duration> = (1..=5).map(|n| accept_retry(n).delay).collect();

        // Then
        assert!(
            waits.windows(2).all(|pair| pair[1] > pair[0]),
            "the backoff must climb; waits were {waits:?}"
        );
    }

    /// The climb stops, because the caller on the other side is retrying its connect and a session
    /// that waited minutes between accepts would be unusable long after the pressure cleared.
    #[test]
    fn the_backoff_never_grows_past_its_ceiling() {
        // Given / When — far past any plausible episode, and past what the doubling can represent
        let retry = accept_retry(u32::MAX);

        // Then
        assert_eq!(retry.delay, ACCEPT_RETRY_MAX_DELAY);
    }

    /// How many consecutive failures it takes for the doubling to reach the ceiling — derived
    /// rather than written down, so the tests keep meaning what they say if the constants move.
    fn failures_to_reach_the_ceiling() -> u32 {
        (1..)
            .find(|n| accept_retry(*n).delay >= ACCEPT_RETRY_MAX_DELAY)
            .expect("the backoff must reach its ceiling")
    }

    /// A UUIDv7's leading half is a millisecond timestamp, so two sessions started close together
    /// share it. Naming the socket after the tail is what keeps the second session's tool calls out
    /// of the first session's jail.
    #[test]
    fn two_sessions_started_in_the_same_millisecond_get_different_tool_sockets() {
        // Given — two UUIDv7s differing only in their random tail.
        let earlier = "019d1e2f-3456-7890-abcd-ef0123456789";
        let later = "019d1e2f-3456-7890-abcd-ef0123456abc";

        // When
        let earlier_socket = host_tool_ipc_socket_path(earlier);
        let later_socket = host_tool_ipc_socket_path(later);

        // Then
        assert_ne!(earlier_socket, later_socket);
    }

    // ─── Where a sandboxed session's build keeps its home ───────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (criterion 2)
    // Changeset: docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md

    /// One `$HOME` shared by every repository is a poisoning channel: a hostile build in one
    /// checkout writes `~/.cargo/config.toml` (`rustc-wrapper`, `target.*.runner`) or drops a
    /// binary in `~/.cargo/bin`, and the developer's own project executes it on its next build.
    #[test]
    fn two_repositories_never_share_a_build_home() {
        // Given
        let base = Path::new("/Users/dev/.tddy/sandbox-codebase-home");

        // When
        let audited = repo_build_home(base, Path::new("/Users/dev/code/my-app"));
        let unaudited = repo_build_home(base, Path::new("/Users/dev/code/a-cloned-fork"));

        // Then
        assert_ne!(audited, unaudited);
    }

    /// …and the caching the persistent home exists for is untouched: the same checkout comes back
    /// to the same home every session, so `~/.cargo` and `~/.bun` are filled once.
    #[test]
    fn the_same_repository_comes_back_to_the_same_build_home() {
        // Given
        let base = Path::new("/Users/dev/.tddy/sandbox-codebase-home");
        let repo = Path::new("/Users/dev/code/my-app");

        // When
        let first_session = repo_build_home(base, repo);
        let second_session = repo_build_home(base, repo);

        // Then
        assert_eq!(first_session, second_session);
    }

    /// The home stays under the base the caller named, so `--codebase-home-dir` still decides where
    /// on this host these trees live.
    #[test]
    fn a_repositorys_build_home_lives_under_the_base_it_was_given() {
        // Given
        let base = Path::new("/Volumes/build-cache/tddy-codebase-homes");

        // When
        let home = repo_build_home(base, Path::new("/Users/dev/code/my-app"));

        // Then
        assert_eq!(home.parent(), Some(base));
    }
}
