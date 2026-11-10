//! Assemble [`SandboxPlan`] from explicit command + recipe — no action-type awareness.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_sandbox::builder::{
    MachPolicy, MountSpec, NetworkSpec, PolicySpec, ReadSpec, SandboxBuilder, SandboxPlan,
};
use tddy_sandbox::{binary_exec_reads, system_baseline_reads, SandboxError, SecretSource};

use crate::route_files::materialise_route_files;

/// Named sandbox recipe overlay applied on top of generic exec/read detection.
///
/// The recipes are generic shapes of process. What a particular tool needs — its home, files,
/// secrets, policy, network — is declared in its tool file and lowered by
/// [`crate::tool_grants`], so no recipe names a tool (`#agent-tools 10/12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxRecipe {
    /// OS baseline + runner/primary binary reads only.
    Generic,
    /// Interactive shell — fork + PTY, no JIT.
    Shell,
    /// `tddy-sandbox-runner` hosting a generic `--pty-command` action.
    RunnerPty,
}

fn parse_recipe_name(name: &str) -> SandboxRecipe {
    match name {
        "bash" | "shell" => SandboxRecipe::Shell,
        _ => SandboxRecipe::Generic,
    }
}

/// Request to build a plan that runs `tddy-sandbox-runner` (or similar) inside the jail.
pub struct RunnerPlanRequest {
    pub project_root: PathBuf,
    pub scratch_dir: PathBuf,
    pub egress_dir: PathBuf,
    pub profile_path: PathBuf,
    pub runner_argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub loopback_allow_ports: Vec<u16>,
    pub ipc_socket: Option<PathBuf>,
    pub mounts: Vec<MountSpec>,
    pub recipe: Option<SandboxRecipe>,
    // TODO(jail-agnostic): read by no recipe since the agent recipes went
    // (docs/dev/todo/2026-11-03-plan-requests-host-home-is-unread.md).
    pub host_home: Option<PathBuf>,
    /// The session's provider route, when it runs on one: its secrets become jail secrets
    /// (`TDDY_SECRET_<NAME>`), and no host OAuth token is injected beside them.
    pub route: Option<tddy_core::provider_route::HarnessRoute>,
    /// The directory the jail keeps read-only to the process ([`SandboxPlan::context_dir`]).
    pub context_dir: Option<PathBuf>,
    /// The process speaks RPC over its stdio ([`SandboxPlan::stdio`]).
    pub stdio: bool,
}

/// Request to build a plan that runs an arbitrary command directly inside the jail.
pub struct ProcessPlanRequest {
    pub project_root: PathBuf,
    pub scratch_dir: PathBuf,
    pub egress_dir: PathBuf,
    pub profile_path: PathBuf,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub mounts: Vec<MountSpec>,
    pub recipe: SandboxRecipe,
    pub host_home: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub extra_reads: Vec<ReadSpec>,
    pub stdin: Option<Vec<u8>>,
}

fn primary_binary_from_command(command: &[String]) -> Option<PathBuf> {
    command.first().map(PathBuf::from)
}

fn canonical_binary_path(path: &str) -> Option<PathBuf> {
    if path.contains('/') {
        std::fs::canonicalize(path).ok()
    } else {
        None
    }
}

fn collect_reads(
    recipe: SandboxRecipe,
    runner_binary: Option<&Path>,
) -> Vec<tddy_sandbox::ReadSpec> {
    let mut reads = Vec::new();
    match recipe {
        SandboxRecipe::Generic | SandboxRecipe::Shell | SandboxRecipe::RunnerPty => {
            reads.extend(system_baseline_reads());
            if let Some(primary) = runner_binary {
                let canon = canonical_binary_path(&primary.to_string_lossy())
                    .unwrap_or_else(|| primary.to_path_buf());
                reads.extend(binary_exec_reads(&canon));
            }
        }
    }
    if let Some(runner) = runner_binary {
        let canon = canonical_binary_path(&runner.to_string_lossy())
            .unwrap_or_else(|| runner.to_path_buf());
        reads.extend(binary_exec_reads(&canon));
    }
    reads
}

fn generic_exec_policy() -> PolicySpec {
    PolicySpec {
        allow_dynamic_code_generation: true,
        allow_process_fork: true,
        mach_lookup: MachPolicy::All,
        sysctl_read: true,
        pseudo_tty: false,
        exec_paths: vec![],
    }
}

/// The policy of a jail whose runner hosts a PTY process: JIT, fork, every Mach service, a
/// pseudo-terminal, and the system directories a shell execs from.
pub fn runner_pty_policy() -> PolicySpec {
    PolicySpec {
        allow_dynamic_code_generation: true,
        allow_process_fork: true,
        mach_lookup: MachPolicy::All,
        sysctl_read: true,
        pseudo_tty: true,
        exec_paths: shell_interactive_policy().exec_paths,
    }
}

fn policy_for_recipe(recipe: SandboxRecipe) -> PolicySpec {
    match recipe {
        SandboxRecipe::Shell => shell_interactive_policy(),
        SandboxRecipe::RunnerPty => runner_pty_policy(),
        SandboxRecipe::Generic => generic_exec_policy(),
    }
}

pub fn shell_interactive_policy() -> PolicySpec {
    PolicySpec {
        allow_dynamic_code_generation: false,
        allow_process_fork: true,
        mach_lookup: tddy_sandbox::builder::MachPolicy::Names(vec![]),
        sysctl_read: false,
        pseudo_tty: true,
        exec_paths: [
            "/usr/bin",
            "/bin",
            "/sbin",
            "/usr/libexec",
            "/System",
            "/Library",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect(),
    }
}

fn finish_builder(
    builder: SandboxBuilder,
    recipe: SandboxRecipe,
) -> Result<SandboxPlan, SandboxError> {
    builder.policy(policy_for_recipe(recipe)).build()
}

/// Add `route`'s secret environment to `builder` as jail secrets — delivered to the agent child by
/// the runner from owner-only files, never in the jail's environment map or on argv.
pub fn route_secrets(
    builder: SandboxBuilder,
    route: &tddy_core::provider_route::HarnessRoute,
) -> SandboxBuilder {
    route
        .secret_env
        .iter()
        .fold(builder, |builder, (name, secret)| {
            builder.secret(
                name.as_str(),
                SecretSource::Value(secret.expose().to_string()),
            )
        })
}

/// Build a [`SandboxPlan`] for `tddy-sandbox-runner` inside the jail.
///
/// The recipe is what the caller names (a generic shape; [`SandboxRecipe::Generic`] when none): the
/// runner's argv is never read for what it runs. A jail lowered from a tool definition overrides the
/// plan's policy, reads, network and secrets with the tool's own
/// (`tddy_daemon_sandbox::tool_jail::build_tool_jail_plan`).
pub fn build_runner_plan(params: RunnerPlanRequest) -> Result<SandboxPlan, SandboxError> {
    let recipe = params.recipe.unwrap_or(SandboxRecipe::Generic);
    let runner = params.runner_argv.first().map(Path::new);
    let reads = collect_reads(recipe, runner);

    // A routed session's non-secret environment joins the jail's, and the route's value wins a
    // clash. Its files are written owner-only into the scratch; the variable that names the first
    // of them is the launch's (`coding_agent.route.config_env`), set where the process is rendered.
    let mut env = params.env;
    if let Some(route) = &params.route {
        env.extend(route.env.iter().cloned());
        materialise_route_files(&params.scratch_dir, route)?;
    }

    let mut builder = SandboxBuilder::new(
        params.project_root,
        params.scratch_dir,
        params.egress_dir,
        params.runner_argv,
    )
    .profile_path(params.profile_path)
    .ipc_socket(params.ipc_socket)
    .context_dir(params.context_dir)
    .stdio(params.stdio)
    .reads(reads)
    .mounts(params.mounts)
    .network(NetworkSpec {
        loopback_allow_ports: params.loopback_allow_ports,
        // The OAuth callback is opened by a tool that declares it (`tool_grants::lower_network`).
        allow_oauth_inbound: false,
        // A recipe-built jail keeps today's unrestricted egress until its tool declares hosts. A
        // jail lowered from tools overrides this with `tool_grants::lower_network`'s union.
        egress: tddy_sandbox::EgressPolicy::Any,
    })
    .env_map(env);
    if let Some(route) = &params.route {
        builder = route_secrets(builder, route);
    }

    finish_builder(builder, recipe)
}

/// Build a [`SandboxPlan`] for running a command directly inside the jail (action orchestration).
pub fn build_process_plan(params: ProcessPlanRequest) -> Result<SandboxPlan, SandboxError> {
    let primary = primary_binary_from_command(&params.command);
    let mut reads = collect_reads(params.recipe, primary.as_deref());
    reads.extend(params.extra_reads);

    let builder = SandboxBuilder::new(
        params.project_root,
        params.scratch_dir,
        params.egress_dir,
        params.command,
    )
    .profile_path(params.profile_path)
    .reads(reads)
    .mounts(params.mounts)
    .env_map(params.env)
    .cwd(params.cwd)
    .stdin(params.stdin);

    finish_builder(builder, params.recipe)
}

/// Parse an optional recipe name string (from action params).
pub fn recipe_from_name(name: Option<&str>) -> SandboxRecipe {
    name.map(parse_recipe_name)
        .unwrap_or(SandboxRecipe::Generic)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_plan_carries_cwd_into_spec() {
        let cwd = PathBuf::from("/tmp/tddy-plan-cwd-test/project");
        let plan = build_process_plan(ProcessPlanRequest {
            project_root: PathBuf::from("/tmp/tddy-plan-cwd-test/project"),
            scratch_dir: PathBuf::from("/tmp/tddy-plan-cwd-test/project/.work"),
            egress_dir: PathBuf::from("/tmp/tddy-plan-cwd-test/out"),
            profile_path: PathBuf::from("/tmp/tddy-plan-cwd-test/project/profile.sb"),
            command: vec!["/bin/echo".into(), "hi".into()],
            env: BTreeMap::new(),
            mounts: vec![],
            recipe: SandboxRecipe::Generic,
            host_home: None,
            cwd: Some(cwd.clone()),
            extra_reads: vec![],
            stdin: None,
        })
        .expect("plan must build");
        assert_eq!(plan.spec.cwd.as_ref(), Some(&cwd));
    }
}
