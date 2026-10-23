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
use crate::cursor_cli::{
    cursor_credentials_copies, cursor_interactive_policy, process_cursor_exec_reads,
};
use crate::muse_cli::muse_interactive_policy;
use crate::opencode_cli::{opencode_interactive_policy, process_opencode_exec_reads};
use crate::route_files::{materialise_route_files, route_config_env};

/// Named sandbox recipe overlay applied on top of generic exec/read detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxRecipe {
    /// OS baseline + runner/primary binary reads only.
    Generic,
    /// Claude Code CLI (Node/V8 interactive PTY + credentials copy).
    ClaudeCli,
    /// Cursor Agent CLI (`agent` binary; Node/V8 interactive PTY + credentials copy).
    CursorCli,
    /// Interactive shell — fork + PTY, no JIT.
    Shell,
    /// `tddy-sandbox-runner` hosting a generic `--pty-command` action.
    RunnerPty,
    /// OpenCode's TUI (`opencode`) in the runner's PTY.
    OpenCodeCli,
    /// Muse Code's TUI (`muse`) in the runner's PTY.
    MuseCli,
}

/// Infer recipe from sandbox-runner argv (`--claude-binary` → Claude CLI; `--pty-command` → Shell).
pub fn detect_recipe_from_argv(argv: &[String]) -> SandboxRecipe {
    if let Some(idx) = argv.iter().position(|a| a == "--agent-kind") {
        return match argv.get(idx + 1).map(String::as_str) {
            Some("cursor") => SandboxRecipe::CursorCli,
            Some("opencode") => SandboxRecipe::OpenCodeCli,
            Some("muse") => SandboxRecipe::MuseCli,
            _ => SandboxRecipe::ClaudeCli,
        };
    }
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
        "cursor-cli" | "cursor_cli" => SandboxRecipe::CursorCli,
        "opencode-cli" | "opencode_cli" => SandboxRecipe::OpenCodeCli,
        "muse-cli" | "muse_cli" => SandboxRecipe::MuseCli,
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
    /// The session's provider route, when it runs on one: its secrets become jail secrets
    /// (`TDDY_SECRET_<NAME>`), and no host OAuth token is injected beside them.
    pub route: Option<tddy_core::provider_route::HarnessRoute>,
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

/// The in-jail agent's binary the runner argv names: `--claude-binary`, or `--agent-binary` for the
/// kinds that take one (`cursor`, `opencode`, `muse`).
fn agent_binary_from_runner_argv(argv: &[String]) -> Option<PathBuf> {
    if let Some(idx) = argv.iter().position(|a| a == "--claude-binary") {
        return argv.get(idx + 1).map(PathBuf::from);
    }
    if let Some(idx) = argv.iter().position(|a| a == "--agent-kind") {
        if argv
            .get(idx + 1)
            .is_some_and(|k| matches!(k.as_str(), "cursor" | "opencode" | "muse"))
        {
            if let Some(bidx) = argv.iter().position(|a| a == "--agent-binary") {
                return argv.get(bidx + 1).map(PathBuf::from);
            }
        }
    }
    None
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
    agent_binary: Option<&Path>,
) -> Vec<tddy_sandbox::ReadSpec> {
    let mut reads = Vec::new();
    match recipe {
        SandboxRecipe::OpenCodeCli => {
            if let Some(opencode) = agent_binary {
                let canon = canonical_binary_path(&opencode.to_string_lossy())
                    .unwrap_or_else(|| opencode.to_path_buf());
                reads.extend(process_opencode_exec_reads(&canon));
            } else {
                reads.extend(system_baseline_reads());
            }
        }
        // A native binary: the OS baseline and the binary's own dependencies, nothing toolchain.
        SandboxRecipe::MuseCli => {
            reads.extend(system_baseline_reads());
            if let Some(muse) = agent_binary {
                let canon = canonical_binary_path(&muse.to_string_lossy())
                    .unwrap_or_else(|| muse.to_path_buf());
                reads.extend(binary_exec_reads(&canon));
            }
        }
        SandboxRecipe::ClaudeCli => {
            if let Some(claude) = agent_binary {
                let canon = canonical_binary_path(&claude.to_string_lossy())
                    .unwrap_or_else(|| claude.to_path_buf());
                reads.extend(process_claude_exec_reads(&canon));
            } else {
                reads.extend(system_baseline_reads());
            }
        }
        SandboxRecipe::CursorCli => {
            if let Some(cursor) = agent_binary {
                let canon = canonical_binary_path(&cursor.to_string_lossy())
                    .unwrap_or_else(|| cursor.to_path_buf());
                reads.extend(process_cursor_exec_reads(&canon));
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
        SandboxRecipe::CursorCli => cursor_interactive_policy(),
        SandboxRecipe::Shell => shell_interactive_policy(),
        SandboxRecipe::RunnerPty => runner_pty_policy(),
        SandboxRecipe::Generic => generic_exec_policy(),
        SandboxRecipe::OpenCodeCli => opencode_interactive_policy(),
        SandboxRecipe::MuseCli => muse_interactive_policy(),
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
        SandboxRecipe::CursorCli => host_home
            .map(|home| cursor_credentials_copies(home, scratch_home))
            .unwrap_or_default(),
        SandboxRecipe::Generic | SandboxRecipe::Shell | SandboxRecipe::RunnerPty => Vec::new(),
        // A routed OpenCode or Muse session is handed its key by the route; nothing is copied.
        SandboxRecipe::OpenCodeCli | SandboxRecipe::MuseCli => Vec::new(),
    }
}

fn finish_builder(
    mut builder: SandboxBuilder,
    recipe: SandboxRecipe,
    host_home: Option<PathBuf>,
    scratch_dir: &Path,
    routed: bool,
) -> Result<SandboxPlan, SandboxError> {
    let scratch_home = scratch_dir.join("home");
    let copies = copies_for_recipe(recipe, host_home.as_deref(), &scratch_home);
    builder = builder.copies(copies).policy(policy_for_recipe(recipe));

    // A routed session's key comes from its route, so no host OAuth token is injected beside it.
    if recipe == SandboxRecipe::ClaudeCli && !routed {
        if let Some(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty())
        {
            builder = builder.secret("CLAUDE_CODE_OAUTH_TOKEN", SecretSource::Value(token));
        }
    }

    builder.build()
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
pub fn build_runner_plan(params: RunnerPlanRequest) -> Result<SandboxPlan, SandboxError> {
    let recipe = params
        .recipe
        .unwrap_or_else(|| detect_recipe_from_argv(&params.runner_argv));
    let agent_bin = agent_binary_from_runner_argv(&params.runner_argv);
    let runner = params.runner_argv.first().map(Path::new);
    let reads = collect_reads(recipe, runner, agent_bin.as_deref());
    let scratch_dir = params.scratch_dir.clone();

    // A routed session's non-secret environment joins the jail's, and the route's value wins a
    // clash. Its files are written owner-only into the scratch, and the harness's configuration
    // variable names the first of them.
    let routed = params.route.is_some();
    let mut env = params.env;
    if let Some(route) = &params.route {
        env.extend(route.env.iter().cloned());
        if let (Some(config_env), Some(config)) = (
            route_config_env(recipe),
            materialise_route_files(&scratch_dir, route)?,
        ) {
            let config_text = config.to_str().ok_or_else(|| {
                SandboxError::InvalidSpec(format!("{} is not valid UTF-8", config.display()))
            })?;
            env.insert(config_env.to_string(), config_text.to_string());
        }
    }

    let mut builder = SandboxBuilder::new(
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
        // A routed session's key is its route's; the host's own OAuth exchange is not opened beside it.
        allow_oauth_inbound: !routed
            && matches!(recipe, SandboxRecipe::ClaudeCli | SandboxRecipe::CursorCli),
    })
    .env_map(env);
    if let Some(route) = &params.route {
        builder = route_secrets(builder, route);
    }

    finish_builder(builder, recipe, params.host_home, &scratch_dir, routed)
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

    finish_builder(
        builder,
        params.recipe,
        params.host_home,
        &scratch_dir,
        false,
    )
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
    fn detect_recipe_from_argv_finds_cursor_agent_kind() {
        let argv = vec![
            "runner".into(),
            "--agent-kind".into(),
            "cursor".into(),
            "--agent-binary".into(),
            "/tmp/stub_agent.sh".into(),
        ];
        assert_eq!(detect_recipe_from_argv(&argv), SandboxRecipe::CursorCli);
        let bin = agent_binary_from_runner_argv(&argv).expect("agent binary");
        assert_eq!(bin, PathBuf::from("/tmp/stub_agent.sh"));
        let reads = collect_reads(
            SandboxRecipe::CursorCli,
            Some(Path::new("/usr/bin/runner")),
            Some(&bin),
        );
        assert!(
            reads
                .iter()
                .any(|r| { r.host.to_string_lossy().contains("/tmp") }),
            "cursor reads must include stub parent under /tmp: {reads:?}"
        );
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
    fn detect_recipe_from_argv_finds_opencode_agent_kind() {
        // Given
        let argv = vec!["runner".into(), "--agent-kind".into(), "opencode".into()];

        // When / Then
        assert_eq!(detect_recipe_from_argv(&argv), SandboxRecipe::OpenCodeCli);
    }

    #[test]
    fn detect_recipe_from_argv_finds_muse_agent_kind() {
        // Given
        let argv = vec!["runner".into(), "--agent-kind".into(), "muse".into()];

        // When / Then
        assert_eq!(detect_recipe_from_argv(&argv), SandboxRecipe::MuseCli);
    }

    #[test]
    fn recipe_from_name_resolves_the_opencode_session_type() {
        // When / Then
        assert_eq!(
            recipe_from_name(Some("opencode-cli")),
            SandboxRecipe::OpenCodeCli
        );
    }

    #[test]
    fn recipe_from_name_resolves_the_muse_session_type() {
        // When / Then
        assert_eq!(recipe_from_name(Some("muse-cli")), SandboxRecipe::MuseCli);
    }

    #[test]
    fn opencode_reads_grant_the_directory_of_its_binary() {
        // Given
        let binary = Path::new("/opt/opencode/bin/opencode");

        // When
        let reads = collect_reads(
            SandboxRecipe::OpenCodeCli,
            Some(Path::new("/usr/bin/runner")),
            Some(binary),
        );

        // Then
        assert!(
            reads
                .iter()
                .any(|r| r.host == Path::new("/opt/opencode/bin")),
            "opencode reads must include its binary's directory: {reads:?}"
        );
    }

    #[test]
    fn muse_reads_grant_the_directory_of_its_binary() {
        // Given
        let binary = Path::new("/opt/muse/bin/muse");

        // When
        let reads = collect_reads(
            SandboxRecipe::MuseCli,
            Some(Path::new("/usr/bin/runner")),
            Some(binary),
        );

        // Then
        assert!(
            reads.iter().any(|r| r.host == Path::new("/opt/muse/bin")),
            "muse reads must include its binary's directory: {reads:?}"
        );
    }

    #[test]
    fn opencode_policy_allows_the_run_time_code_generation_bun_needs() {
        // When / Then
        assert!(policy_for_recipe(SandboxRecipe::OpenCodeCli).allow_dynamic_code_generation);
    }

    #[test]
    fn opencode_policy_keeps_the_pty_its_tui_draws_on() {
        // When / Then
        assert!(policy_for_recipe(SandboxRecipe::OpenCodeCli).pseudo_tty);
    }

    #[test]
    fn muse_policy_forbids_run_time_code_generation_as_a_native_binary() {
        // When / Then
        assert!(!policy_for_recipe(SandboxRecipe::MuseCli).allow_dynamic_code_generation);
    }

    #[test]
    fn muse_policy_keeps_the_pty_its_tui_draws_on() {
        // When / Then
        assert!(policy_for_recipe(SandboxRecipe::MuseCli).pseudo_tty);
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
