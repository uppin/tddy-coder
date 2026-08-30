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
use std::sync::Arc;

use async_trait::async_trait;
use tddy_sandbox::{CgroupConfig, MountSpec, SandboxError, SandboxPlan};
use tddy_service::proto::connection::{ExecuteToolRequest, ExecuteToolResponse};
use tokio::sync::Mutex;

/// A live jail serving one sandboxed workspace session.
#[async_trait]
pub trait WorkspaceSandbox: Send + Sync {
    /// Run one tool call inside the jail.
    ///
    /// A tool that failed answers with `is_error`, exactly as the host tool engine does: only the
    /// dispatch is this trait's concern, so the caller cannot tell "the tool said no" from "the
    /// jail said no" by the shape of the answer alone.
    async fn execute_tool(&self, req: &ExecuteToolRequest) -> ExecuteToolResponse;

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
/// The same shape [`crate::connection_service`] lays out for a sandboxed `claude-cli` session, with
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
    /// The AF_UNIX socket the runner binds for tool IPC.
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

    // TODO(workspace-tool-sandbox step 4): the runner still requires `--model` and spawns an agent;
    // teaching it to serve `in_jail_tool_request` with no agent at all is what makes this argv
    // spawnable.
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
        "--stdio".to_string(),
    ];

    let env = tddy_sandbox::scratch_runner_env(
        &scratch_home,
        &scratch_tmp,
        &session_id,
        &layout.tool_ipc_socket,
        &layout.egress_dir,
    );

    let mut plan =
        tddy_sandbox_recipes::build_runner_plan(tddy_sandbox_recipes::RunnerPlanRequest {
            project_root: layout.sandbox_root.clone(),
            scratch_dir: layout.scratch_dir.clone(),
            egress_dir: layout.egress_dir.clone(),
            profile_path: layout.profile_path.clone(),
            runner_argv,
            env,
            loopback_allow_ports: vec![],
            ipc_socket: Some(layout.tool_ipc_socket.clone()),
            // The boundary the feature sells: one mount, the session's checkout.
            mounts: vec![MountSpec::read_write(worktree_path)],
            // The jail runs the session's tools, `Shell` among them — the recipe that grants a
            // subprocess and a PTY, and nothing an agent runtime would need.
            recipe: Some(tddy_sandbox_recipes::SandboxRecipe::Shell),
            host_home: None,
        })?;
    plan.cgroup = cgroup;
    Ok(plan)
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
        // TODO(workspace-tool-sandbox step 4): spawn the plan from `build_workspace_tool_plan`,
        // bridge its stdio, and answer `execute_tool` with an `in_jail_tool_request` frame. Until
        // then a sandboxed workspace start is refused here rather than served unconfined — the
        // start path turns this into `failed_precondition`.
        Err(SandboxError::Unsupported {
            platform: std::env::consts::OS.to_string(),
            message: format!(
                "the workspace tool jail for session {} is not spawned yet; refusing rather than \
                 running its tools on the host",
                spec.session_id
            ),
        })
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
}
