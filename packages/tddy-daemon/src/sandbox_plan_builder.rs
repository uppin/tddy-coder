//! Build [`SandboxPlan`] from action orchestration inputs — outside `tddy-sandbox`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_actions::{ActionInput, ActionSpec};
use tddy_sandbox::builder::{MountSpec, ReadReason, ReadSpec};
use tddy_sandbox::{scratch_runner_env, SandboxError, SandboxPlan};
use tddy_sandbox_recipes::{
    build_process_plan, build_runner_plan, recipe_from_name, ProcessPlanRequest, RunnerPlanRequest,
    SandboxRecipe,
};

use crate::sandbox_session::{pick_free_loopback_port, resolve_sandbox_runner_path};

/// Paths produced when building a runner-based confined action (PTY mode).
pub struct ActionRunnerArtifacts {
    pub plan: SandboxPlan,
    pub ready_marker: PathBuf,
    pub grpc_socket: PathBuf,
    pub egress_dir: PathBuf,
}

/// Layout for a confined action: writable egress + scratch under a project root.
pub struct ActionSandboxLayout {
    pub project_root: PathBuf,
    pub scratch_dir: PathBuf,
    pub egress_dir: PathBuf,
    pub profile_path: PathBuf,
}

impl ActionSandboxLayout {
    pub fn under_output_dir(output_dir: &Path, action_id: &str) -> Self {
        let project_root = output_dir.join("sandbox").join(action_id);
        Self {
            scratch_dir: project_root.join(".work"),
            egress_dir: output_dir.to_path_buf(),
            profile_path: project_root.join("sandbox.sb"),
            project_root,
        }
    }
}

fn input_mounts(inputs: &[ActionInput]) -> Vec<MountSpec> {
    inputs
        .iter()
        .map(|input| MountSpec {
            host: input.host_path.clone(),
            jail: input
                .jail_path
                .clone()
                .or_else(|| Some(input.host_path.clone())),
            writable: input.writable,
        })
        .collect()
}

fn ensure_layout_dirs(layout: &ActionSandboxLayout) -> Result<(PathBuf, PathBuf), SandboxError> {
    std::fs::create_dir_all(&layout.project_root).map_err(|e| SandboxError::Io(e.to_string()))?;
    let scratch_home = layout.scratch_dir.join("home");
    let scratch_tmp = layout.scratch_dir.join("tmp");
    std::fs::create_dir_all(&scratch_home).map_err(|e| SandboxError::Io(e.to_string()))?;
    std::fs::create_dir_all(&scratch_tmp).map_err(|e| SandboxError::Io(e.to_string()))?;
    Ok((scratch_home, scratch_tmp))
}

fn canonicalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn is_allowed_cwd(cwd: &Path, layout: &ActionSandboxLayout, mounts: &[MountSpec]) -> bool {
    let cwd = canonicalize_path(cwd);
    let under = |base: &Path| cwd.starts_with(canonicalize_path(base));
    if under(&layout.project_root) || under(&layout.scratch_dir) || under(&layout.egress_dir) {
        return true;
    }
    mounts
        .iter()
        .any(|m| cwd.starts_with(canonicalize_path(&m.host)))
}

fn resolve_action_cwd(
    spec: &ActionSpec,
    layout: &ActionSandboxLayout,
    mounts: &[MountSpec],
) -> Result<PathBuf, SandboxError> {
    let cwd = match &spec.working_dir {
        Some(wd) => canonicalize_path(wd),
        None => canonicalize_path(&layout.project_root),
    };
    if !is_allowed_cwd(&cwd, layout, mounts) {
        return Err(SandboxError::InvalidSpec(format!(
            "working_dir {} is outside writable jail tree and mounts",
            cwd.display()
        )));
    }
    Ok(cwd)
}

fn extra_read_specs(paths: &[PathBuf]) -> Vec<ReadSpec> {
    paths
        .iter()
        .map(|p| ReadSpec::subpath(canonicalize_path(p), ReadReason::BinaryDeps))
        .collect()
}

/// Assemble a generic process [`SandboxPlan`] from an [`ActionSpec`] (orchestrator-only).
pub fn build_action_sandbox_plan(
    spec: &ActionSpec,
    layout: &ActionSandboxLayout,
    session_id: &str,
    extra_env: BTreeMap<String, String>,
) -> Result<SandboxPlan, SandboxError> {
    let sandbox = spec
        .sandbox
        .as_ref()
        .ok_or_else(|| SandboxError::InvalidSpec("missing sandbox request".into()))?;

    let recipe = recipe_from_name(sandbox.recipe.as_deref().or(Some(spec.kind.as_str())));

    let (scratch_home, scratch_tmp) = ensure_layout_dirs(layout)?;
    let ipc_stub = layout.project_root.join("tool_ipc.sock");
    let mounts = input_mounts(&spec.inputs);
    let cwd = resolve_action_cwd(spec, layout, &mounts)?;

    let mut env = scratch_runner_env(
        &scratch_home,
        &scratch_tmp,
        session_id,
        &ipc_stub,
        &sandbox.output_dir,
    );
    if recipe == SandboxRecipe::ClaudeCli {
        env.extend(tddy_sandbox_recipes::claude_runner_env_overlay(
            &scratch_tmp,
        ));
    }
    env.extend(extra_env);

    build_process_plan(ProcessPlanRequest {
        project_root: layout.project_root.clone(),
        scratch_dir: layout.scratch_dir.clone(),
        egress_dir: sandbox.output_dir.clone(),
        profile_path: layout.profile_path.clone(),
        command: spec.command.clone(),
        env,
        mounts,
        recipe,
        host_home: std::env::var_os("HOME").map(PathBuf::from),
        cwd: Some(cwd),
        extra_reads: extra_read_specs(&sandbox.extra_read_paths),
        stdin: sandbox.stdin.as_ref().map(|s| s.as_bytes().to_vec()),
    })
}

/// Assemble a sandbox-runner [`SandboxPlan`] for PTY-mode actions.
pub fn build_action_runner_plan(
    spec: &ActionSpec,
    layout: &ActionSandboxLayout,
    session_id: &str,
    extra_env: BTreeMap<String, String>,
) -> Result<ActionRunnerArtifacts, SandboxError> {
    let sandbox = spec
        .sandbox
        .as_ref()
        .ok_or_else(|| SandboxError::InvalidSpec("missing sandbox request".into()))?;

    if spec.command.is_empty() {
        return Err(SandboxError::InvalidSpec(
            "pty sandbox action requires non-empty command".into(),
        ));
    }

    let recipe = SandboxRecipe::RunnerPty;
    let (scratch_home, scratch_tmp) = ensure_layout_dirs(layout)?;
    let ipc_stub = layout.project_root.join("tool_ipc.sock");
    let mounts = input_mounts(&spec.inputs);
    let cwd = resolve_action_cwd(spec, layout, &mounts)?;

    let project_root = canonicalize_path(&layout.project_root);
    let egress_dir = canonicalize_path(&sandbox.output_dir);
    let scratch_dir = project_root.join(".work");
    let context_dir = project_root.join("context");
    std::fs::create_dir_all(&context_dir).map_err(|e| SandboxError::Io(e.to_string()))?;

    let ready_marker = project_root.join("sandbox.ready");
    let grpc_socket = project_root.join("sandbox.grpc.sock");
    let tool_ipc_socket = project_root.join("tool_ipc.sock");

    let grpc_port = pick_free_loopback_port().map_err(SandboxError::InvalidSpec)?;
    let shim_port = pick_free_loopback_port().map_err(SandboxError::InvalidSpec)?;

    let mut env = scratch_runner_env(
        &scratch_home,
        &scratch_tmp,
        session_id,
        &ipc_stub,
        &egress_dir,
    );
    env.extend(extra_env);

    let runner = resolve_sandbox_runner_path();
    let mut runner_argv = vec![
        runner,
        "--session-id".into(),
        session_id.to_string(),
        "--context-dir".into(),
        context_dir.to_string_lossy().into_owned(),
        "--cwd".into(),
        cwd.to_string_lossy().into_owned(),
        "--grpc-socket".into(),
        grpc_socket.to_string_lossy().into_owned(),
        "--tool-ipc-socket".into(),
        tool_ipc_socket.to_string_lossy().into_owned(),
        "--ready-marker".into(),
        ready_marker.to_string_lossy().into_owned(),
        "--grpc-listen-port".into(),
        grpc_port.to_string(),
        "--egress-shim-port".into(),
        shim_port.to_string(),
        "--model".into(),
        String::new(),
    ];
    for arg in &spec.command {
        runner_argv.push(format!("--pty-command={arg}"));
    }

    #[cfg(target_os = "linux")]
    {
        runner_argv.push("--grpc-uds".into());
        runner_argv.push(grpc_socket.to_string_lossy().into_owned());
    }

    let plan = build_runner_plan(RunnerPlanRequest {
        project_root: project_root.clone(),
        scratch_dir: scratch_dir.clone(),
        egress_dir: egress_dir.clone(),
        profile_path: layout.profile_path.clone(),
        runner_argv,
        env,
        loopback_allow_ports: vec![grpc_port, shim_port],
        ipc_socket: Some(tool_ipc_socket),
        mounts,
        recipe: Some(recipe),
        host_home: std::env::var_os("HOME").map(PathBuf::from),
    })?;

    Ok(ActionRunnerArtifacts {
        plan,
        ready_marker,
        grpc_socket,
        egress_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_actions::{ChannelMode, SandboxRequest};

    fn minimal_spec(output_dir: &Path) -> ActionSpec {
        ActionSpec {
            id: "test-action".into(),
            kind: "bash".into(),
            command: vec!["/bin/echo".into(), "hi".into()],
            inputs: vec![],
            outputs: vec![],
            env: BTreeMap::new(),
            working_dir: None,
            channel_mode: ChannelMode::Combined,
            sandbox: Some(SandboxRequest {
                output_dir: output_dir.to_path_buf(),
                extra_read_paths: vec![PathBuf::from("/usr/bin")],
                recipe: Some("bash".into()),
                stdin: None,
            }),
            session: None,
            pipeline: None,
        }
    }

    #[test]
    fn build_plan_includes_extra_read_paths() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let layout = ActionSandboxLayout::under_output_dir(tmp.path(), "test-action");
        let spec = minimal_spec(tmp.path());
        let plan = build_action_sandbox_plan(&spec, &layout, "sess", BTreeMap::new())
            .expect("plan must build");
        assert!(
            plan.reads.iter().any(|r| r.host == Path::new("/usr/bin")),
            "extra_read_paths must appear in plan reads"
        );
    }
}
