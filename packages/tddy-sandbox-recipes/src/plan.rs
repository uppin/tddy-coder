//! Assemble [`SandboxPlan`] from explicit command + recipe — no action-type awareness.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_sandbox::builder::{
    MachPolicy, MountSpec, NetworkSpec, PolicySpec, ReadSpec, SandboxBuilder, SandboxPlan,
};
use tddy_sandbox::{binary_exec_reads, system_baseline_reads, SandboxError, SecretSource};

use crate::claude_cli::{
    claude_credentials_copies, claude_interactive_policy, process_claude_exec_reads,
};

/// Named sandbox recipe overlay applied on top of generic exec/read detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxRecipe {
    /// OS baseline + runner/primary binary reads only.
    Generic,
    /// Claude Code CLI (Node/V8 interactive PTY + credentials copy).
    ClaudeCli,
    /// Interactive shell — fork + PTY, no JIT.
    Shell,
    /// `tddy-sandbox-runner` hosting a generic `--pty-command` action.
    RunnerPty,
}

/// Infer recipe from sandbox-runner argv (`--claude-binary` → Claude CLI; `--pty-command` → Shell).
pub fn detect_recipe_from_argv(argv: &[String]) -> SandboxRecipe {
    if argv.iter().any(|a| a == "--claude-binary") {
        SandboxRecipe::ClaudeCli
    } else if argv
        .iter()
        .any(|a| a == "--pty-command" || a.starts_with("--pty-command="))
    {
        SandboxRecipe::RunnerPty
    } else {
        SandboxRecipe::Generic
    }
}

fn parse_recipe_name(name: &str) -> SandboxRecipe {
    match name {
        "claude-cli" | "claude_cli" => SandboxRecipe::ClaudeCli,
        "bash" | "shell" => SandboxRecipe::Shell,
        "generic" => SandboxRecipe::Generic,
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
    pub host_home: Option<PathBuf>,
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

fn claude_binary_from_runner_argv(argv: &[String]) -> Option<PathBuf> {
    let idx = argv.iter().position(|a| a == "--claude-binary")?;
    argv.get(idx + 1).map(PathBuf::from)
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
    claude_binary: Option<&Path>,
) -> Vec<tddy_sandbox::ReadSpec> {
    let mut reads = Vec::new();
    match recipe {
        SandboxRecipe::ClaudeCli => {
            if let Some(claude) = claude_binary {
                let canon = canonical_binary_path(&claude.to_string_lossy())
                    .unwrap_or_else(|| claude.to_path_buf());
                reads.extend(process_claude_exec_reads(&canon));
            } else {
                reads.extend(system_baseline_reads());
            }
        }
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

fn runner_pty_policy() -> PolicySpec {
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
        SandboxRecipe::ClaudeCli => claude_interactive_policy(),
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

fn copies_for_recipe(
    recipe: SandboxRecipe,
    host_home: Option<&Path>,
    scratch_home: &Path,
) -> Vec<tddy_sandbox::CopySpec> {
    match recipe {
        SandboxRecipe::ClaudeCli => host_home
            .map(|home| claude_credentials_copies(home, scratch_home))
            .unwrap_or_default(),
        SandboxRecipe::Generic | SandboxRecipe::Shell | SandboxRecipe::RunnerPty => Vec::new(),
    }
}

fn finish_builder(
    mut builder: SandboxBuilder,
    recipe: SandboxRecipe,
    host_home: Option<PathBuf>,
    scratch_dir: &Path,
) -> Result<SandboxPlan, SandboxError> {
    let scratch_home = scratch_dir.join("home");
    let copies = copies_for_recipe(recipe, host_home.as_deref(), &scratch_home);
    builder = builder.copies(copies).policy(policy_for_recipe(recipe));

    if recipe == SandboxRecipe::ClaudeCli {
        if let Some(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty())
        {
            builder = builder.secret("CLAUDE_CODE_OAUTH_TOKEN", SecretSource::Value(token));
        }
    }

    builder.build()
}

/// Build a [`SandboxPlan`] for `tddy-sandbox-runner` inside the jail.
pub fn build_runner_plan(params: RunnerPlanRequest) -> Result<SandboxPlan, SandboxError> {
    let recipe = params
        .recipe
        .unwrap_or_else(|| detect_recipe_from_argv(&params.runner_argv));
    let claude_bin = claude_binary_from_runner_argv(&params.runner_argv);
    let runner = params.runner_argv.first().map(Path::new);
    let reads = collect_reads(recipe, runner, claude_bin.as_deref());
    let scratch_dir = params.scratch_dir.clone();

    let builder = SandboxBuilder::new(
        params.project_root,
        params.scratch_dir,
        params.egress_dir,
        params.runner_argv,
    )
    .profile_path(params.profile_path)
    .ipc_socket(params.ipc_socket)
    .reads(reads)
    .mounts(params.mounts)
    .network(NetworkSpec {
        loopback_allow_ports: params.loopback_allow_ports,
        allow_oauth_inbound: recipe == SandboxRecipe::ClaudeCli,
    })
    .env_map(params.env);

    finish_builder(builder, recipe, params.host_home, &scratch_dir)
}

/// Build a [`SandboxPlan`] for running a command directly inside the jail (action orchestration).
pub fn build_process_plan(params: ProcessPlanRequest) -> Result<SandboxPlan, SandboxError> {
    let primary = primary_binary_from_command(&params.command);
    let mut reads = collect_reads(params.recipe, primary.as_deref(), None);
    reads.extend(params.extra_reads);
    let scratch_dir = params.scratch_dir.clone();

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

    finish_builder(builder, params.recipe, params.host_home, &scratch_dir)
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
    fn detect_recipe_from_argv_finds_pty_command_flag() {
        let argv = vec![
            "runner".into(),
            "--pty-command=/bin/sh".into(),
            "--pty-command=-c".into(),
        ];
        assert_eq!(detect_recipe_from_argv(&argv), SandboxRecipe::RunnerPty);
    }

    #[test]
    fn detect_recipe_from_argv_finds_claude_binary_flag() {
        let argv = vec![
            "runner".into(),
            "--claude-binary".into(),
            "/bin/claude".into(),
        ];
        assert_eq!(detect_recipe_from_argv(&argv), SandboxRecipe::ClaudeCli);
    }

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
