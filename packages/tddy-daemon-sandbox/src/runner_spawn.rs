//! Spawning `tddy-sandbox-runner` in its jail: the spawn parameters and the plan built from them.
//! Moved out of `sandbox_session.rs` unchanged (`#tab-hardening` 3/4, file length gate); its items
//! are re-exported there, so every path is as before.

use std::path::PathBuf;

use tddy_sandbox::{MountSpec, SandboxError, SandboxPlan};

#[cfg(target_os = "macos")]
use crate::runner_paths::build_allow_read_paths;

/// Parameters for spawning `tddy-sandbox-runner` inside Seatbelt.
pub struct SandboxRunnerSpawn {
    pub project_root: PathBuf,
    pub scratch_dir: PathBuf,
    pub egress_dir: PathBuf,
    pub profile_path: PathBuf,
    pub runner_argv: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    pub loopback_allow_ports: Vec<u16>,
    pub ipc_socket: Option<PathBuf>,
    /// Host directories made available inside the jail (e.g. the project repo, or the persistent
    /// claude home). Empty for the daemon's remote-codebase sessions.
    pub mounts: Vec<MountSpec>,
    /// Host `$HOME` used by the recipe to auto-copy `.claude/.credentials.json` into the jail each
    /// session. `None` disables that per-session copy — required when a **persistent** claude home
    /// is mounted and seeded separately (a re-copy would clobber the jail's refreshed OAuth token).
    pub host_home: Option<PathBuf>,
    /// cgroup v2 delegation parameters from daemon config (Linux cgroups backend only; the macOS and
    /// QEMU backends ignore it). Defaults to an empty [`tddy_sandbox::CgroupConfig`], letting the
    /// backend derive the delegated base at runtime.
    pub cgroup: tddy_sandbox::CgroupConfig,
    /// The provider route the jailed agent runs on, when the session runs on a provider account.
    /// Its environment and secrets reach the agent child through [`build_sandbox_plan`]. `None` for
    /// the harness's own vendor, and for a jail that hosts no agent.
    pub route: Option<tddy_core::provider_route::HarnessRoute>,
    /// The directory the jail keeps read-only to the runner ([`SandboxPlan::context_dir`]).
    pub context_dir: Option<PathBuf>,
    /// The runner speaks RPC over its stdio ([`SandboxPlan::stdio`]).
    pub stdio: bool,
}

/// Assemble the explicit [`SandboxPlan`] for a runner spawn via `tddy-sandbox-recipes`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn build_sandbox_plan(params: SandboxRunnerSpawn) -> Result<SandboxPlan, SandboxError> {
    use tddy_sandbox_recipes::{build_runner_plan, RunnerPlanRequest};

    let mut plan = build_runner_plan(RunnerPlanRequest {
        project_root: params.project_root,
        scratch_dir: params.scratch_dir,
        egress_dir: params.egress_dir,
        profile_path: params.profile_path,
        runner_argv: params.runner_argv.clone(),
        env: params.env,
        loopback_allow_ports: params.loopback_allow_ports,
        ipc_socket: params.ipc_socket,
        mounts: params.mounts,
        recipe: None,
        host_home: params.host_home,
        route: params.route,
        context_dir: params.context_dir,
        stdio: params.stdio,
    })?;
    plan.cgroup = params.cgroup;
    #[cfg(target_os = "macos")]
    let plan = {
        let mut plan = plan;
        merge_allow_read_paths_into_plan(&mut plan, &params.runner_argv);
        plan
    };
    Ok(plan)
}

#[cfg(target_os = "macos")]
fn merge_allow_read_paths_into_plan(plan: &mut SandboxPlan, runner_argv: &[String]) {
    use tddy_sandbox::builder::{ReadReason, ReadSpec};
    for path in build_allow_read_paths(runner_argv) {
        let spec = ReadSpec::subpath(&path, ReadReason::BinaryDeps).executable();
        if !plan
            .reads
            .iter()
            .any(|r| r.host == spec.host && r.kind == spec.kind)
        {
            plan.reads.push(spec);
        }
    }
}

/// When set to `qemu`, [`spawn_sandbox_runner`] and [`crate::sandbox_action::spawn_confined_plan`]
/// route to [`tddy_sandbox_qemu::spawn_plan`] instead of the per-OS default (Seatbelt on macOS,
/// cgroups+namespaces on Linux). Unset (or any other value) leaves existing per-OS dispatch
/// unchanged — this is an explicit opt-in, not a fallback.
pub fn qemu_backend_requested() -> bool {
    std::env::var("TDDY_SANDBOX_BACKEND").as_deref() == Ok("qemu")
}

/// Spawn an already-assembled [`SandboxPlan`] on this host's backend: Seatbelt on macOS,
/// cgroups + namespaces on Linux, or the QEMU VM backend when explicitly requested.
///
/// The seam for a caller that builds its own plan rather than a runner spawn's
/// ([`crate::workspace_tool_sandbox::build_workspace_tool_plan`] does, because a workspace tool
/// jail holds a different recipe and a different mount set).
#[cfg(target_os = "macos")]
pub fn spawn_sandbox_plan(plan: SandboxPlan) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    if qemu_backend_requested() {
        return tddy_sandbox_qemu::spawn_plan(plan);
    }
    tddy_sandbox_darwin::spawn_plan(plan)
}

#[cfg(target_os = "linux")]
pub fn spawn_sandbox_plan(plan: SandboxPlan) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    if qemu_backend_requested() {
        return tddy_sandbox_qemu::spawn_plan(plan);
    }
    tddy_sandbox_cgroups::spawn_plan(plan)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn spawn_sandbox_plan(_plan: SandboxPlan) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    Err(SandboxError::Unsupported {
        platform: std::env::consts::OS.to_string(),
        message: "platform sandboxes are not available on this OS".to_string(),
    })
}

/// Spawn sandbox-runner inside the platform jail (Seatbelt on macOS, cgroups + namespaces on
/// Linux, or the QEMU VM backend when requested).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn spawn_sandbox_runner(
    params: SandboxRunnerSpawn,
) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    spawn_sandbox_plan(build_sandbox_plan(params)?)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn spawn_sandbox_runner(
    _params: SandboxRunnerSpawn,
) -> Result<tddy_sandbox::SandboxHandle, SandboxError> {
    Err(SandboxError::Unsupported {
        platform: std::env::consts::OS.to_string(),
        message: "platform sandboxes are not available on this OS".to_string(),
    })
}
