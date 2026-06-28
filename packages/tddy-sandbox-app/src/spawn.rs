//! Spawn `tddy-sandbox-runner` inside Seatbelt without a host `tddy-daemon`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tddy_daemon::sandbox_session::{
    build_sandbox_runner_env, copy_dir_all, pick_free_loopback_port, prepare_context_dir,
    resolve_sandbox_runner_path, resolve_tddy_tools_path, spawn_sandbox_runner,
    wait_for_sandbox_ready, SandboxRunnerSpawn,
};
use tddy_sandbox::{append_line, SandboxContextDir, SandboxHandle};

fn spawn_trace(session_dir: &Path, message: &str) {
    eprintln!("{message}");
    let trace = session_dir.join("spawn.trace.log");
    let _ = append_line(&trace, message);
}

/// Parameters for a local sandboxed Claude session.
pub struct SpawnParams {
    pub repo: PathBuf,
    pub session_id: String,
    pub model: String,
    pub permission_mode: String,
    pub claude_binary: Option<String>,
    pub tddy_tools_path: Option<String>,
    pub sandbox_runner_path: Option<String>,
    pub session_dir: PathBuf,
}

/// A sandboxed Claude session ready for host `SessionChannel` attach.
pub struct SpawnedSandbox {
    pub handle: SandboxHandle,
    pub session_id: String,
    pub worktree_path: PathBuf,
    pub ready_marker: PathBuf,
    pub egress_dir: PathBuf,
    pub session_dir: PathBuf,
}

fn canonicalize_exec_path(path: &str) -> String {
    if path.contains('/') {
        std::fs::canonicalize(path)
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    }
}

fn resolve_claude_binary(configured: Option<&str>) -> Result<String> {
    let name = configured
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("claude");
    if name.contains('/') {
        let path = Path::new(name);
        anyhow::ensure!(
            path.is_file() || path.is_symlink(),
            "claude binary not found at {}",
            path.display()
        );
        return Ok(canonicalize_exec_path(name));
    }
    let which_out = std::process::Command::new("which")
        .arg(name)
        .output()
        .context("run which to locate claude")?;
    if which_out.status.success() {
        let path = String::from_utf8_lossy(&which_out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !path.is_empty() {
            return Ok(canonicalize_exec_path(&path));
        }
    }
    anyhow::bail!(
        "claude binary {name:?} not found on host PATH.\n\
         The sandbox jail only includes /usr/bin:/bin — pass an absolute path, e.g.:\n\
         --claude-binary $(which claude)"
    );
}

async fn wait_for_runner_failure_or_settle(egress_dir: &Path) -> Result<()> {
    use tddy_sandbox::SANDBOX_RUNNER_FAILURE;

    let failure_marker = egress_dir.join(SANDBOX_RUNNER_FAILURE);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        if failure_marker.exists() {
            let message = std::fs::read_to_string(&failure_marker).unwrap_or_default();
            let logs = tddy_sandbox::format_egress_logs(egress_dir);
            anyhow::bail!(
                "sandbox runner failed to start claude inside the jail.\n{message}\n{logs}"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Ok(())
}

/// Prepare sandbox dirs, context, and spawn `sandbox-exec` → `tddy-sandbox-runner`.
#[cfg(target_os = "macos")]
pub async fn spawn_claude_sandbox(params: SpawnParams) -> Result<SpawnedSandbox> {
    let repo = params
        .repo
        .canonicalize()
        .with_context(|| format!("canonicalize repo {}", params.repo.display()))?;
    if !repo.is_dir() {
        anyhow::bail!("repo is not a directory: {}", repo.display());
    }

    let session_dir = params.session_dir.clone();
    std::fs::create_dir_all(&session_dir).context("create session dir")?;

    let sandbox_root = session_dir.join("sandbox");
    let egress_dir = session_dir.join("egress");
    std::fs::create_dir_all(sandbox_root.join(".work").join("home"))
        .context("mkdir sandbox scratch home")?;
    std::fs::create_dir_all(sandbox_root.join(".work").join("tmp"))
        .context("mkdir sandbox scratch tmp")?;
    std::fs::create_dir_all(sandbox_root.join("context")).context("mkdir sandbox context")?;
    std::fs::create_dir_all(&egress_dir).context("mkdir sandbox egress")?;

    let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
    let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
    let scratch_dir = sandbox_root.join(".work");
    let scratch_home = scratch_dir.join("home");
    let scratch_tmp = scratch_dir.join("tmp");
    let context_dir = sandbox_root.join("context");

    spawn_trace(
        &session_dir,
        &format!("preparing context from {} …", repo.display()),
    );
    let repo_for_context = repo.clone();
    let ctx: SandboxContextDir = tokio::task::spawn_blocking(move || {
        prepare_context_dir(&repo_for_context).map_err(|e| anyhow::anyhow!(e))
    })
    .await
    .context("context prep task join")??;
    spawn_trace(&session_dir, "copying context into jail tree …");
    copy_dir_all(ctx.path(), &context_dir).map_err(|e| anyhow::anyhow!(e))?;
    tddy_daemon::sandbox_session::seed_claude_home_config(&scratch_home)
        .map_err(|e| anyhow::anyhow!(e))?;
    spawn_trace(&session_dir, "context ready");

    spawn_trace(&session_dir, "resolving claude / tddy-tools / sandbox-runner paths …");
    let tddy_tools_path = canonicalize_exec_path(&resolve_tddy_tools_path(
        params.tddy_tools_path.as_deref(),
    ));
    let sandbox_runner_path = params
        .sandbox_runner_path
        .clone()
        .map(|p| canonicalize_exec_path(&p))
        .unwrap_or_else(|| canonicalize_exec_path(&resolve_sandbox_runner_path()));
    let claude_binary = resolve_claude_binary(params.claude_binary.as_deref())?;

    let grpc_socket = sandbox_root.join("sandbox.grpc.sock");
    let tool_ipc_socket =
        tddy_sandbox::SandboxSpec::short_ipc_socket_path(&params.session_id);
    let ready_marker = sandbox_root.join("sandbox.ready");
    let profile_path = sandbox_root.join("sandbox.sb");

    let grpc_listen_port =
        pick_free_loopback_port().map_err(|e| anyhow::anyhow!("pick grpc listen port: {e}"))?;
    let egress_shim_port =
        pick_free_loopback_port().map_err(|e| anyhow::anyhow!("pick egress shim port: {e}"))?;
    let loopback_allow_ports = vec![grpc_listen_port, egress_shim_port];

    let perm = if params.permission_mode.trim().is_empty() {
        "auto"
    } else {
        params.permission_mode.trim()
    };

    let runner_argv = vec![
        sandbox_runner_path,
        "--session-id".into(),
        params.session_id.clone(),
        "--context-dir".into(),
        context_dir.to_string_lossy().to_string(),
        "--grpc-socket".into(),
        grpc_socket.to_string_lossy().to_string(),
        "--tool-ipc-socket".into(),
        tool_ipc_socket.to_string_lossy().to_string(),
        "--tddy-tools-path".into(),
        tddy_tools_path,
        "--ready-marker".into(),
        ready_marker.to_string_lossy().to_string(),
        "--claude-binary".into(),
        claude_binary.clone(),
        "--model".into(),
        params.model.clone(),
        "--permission-mode".into(),
        perm.to_string(),
        "--grpc-listen-port".into(),
        grpc_listen_port.to_string(),
        "--egress-shim-port".into(),
        egress_shim_port.to_string(),
    ];

    let env = build_sandbox_runner_env(
        &scratch_home,
        &scratch_tmp,
        &params.session_id,
        &tool_ipc_socket,
        &egress_dir,
    );

    spawn_trace(
        &session_dir,
        "spawning sandbox-exec → tddy-sandbox-runner …",
    );

    let mut handle = spawn_sandbox_runner(SandboxRunnerSpawn {
        project_root: sandbox_root.clone(),
        scratch_dir: scratch_dir.clone(),
        egress_dir: egress_dir.clone(),
        profile_path,
        runner_argv: runner_argv.clone(),
        env,
        loopback_allow_ports,
        ipc_socket: Some(tool_ipc_socket),
    })
    .map_err(|e| {
        let logs = tddy_sandbox::format_egress_logs(&egress_dir);
        anyhow::anyhow!("spawn sandbox-runner: {e}\n{logs}")
    })?;

    spawn_trace(
        &session_dir,
        &format!(
            "waiting for sandbox ready marker (timeout 120s): {}",
            ready_marker.display()
        ),
    );

    tokio::select! {
        res = wait_for_sandbox_ready(
            &mut handle,
            &ready_marker,
            std::time::Duration::from_secs(120),
            &egress_dir,
        ) => res,
        _ = tokio::signal::ctrl_c() => {
            spawn_trace(&session_dir, "interrupted (Ctrl-C) while waiting for sandbox ready");
            let _ = handle.child_mut().kill();
            let _ = handle.child_mut().wait();
            Err("interrupted waiting for sandbox ready".to_string())
        }
    }
    .map_err(|e| {
        let logs = tddy_sandbox::format_egress_logs(&egress_dir);
        anyhow::anyhow!("{e}\n{logs}")
    })?;

    wait_for_runner_failure_or_settle(&egress_dir).await?;

    log::info!(
        target: "tddy_sandbox_app::spawn",
        "sandbox ready session_id={} repo={} egress={}",
        params.session_id,
        repo.display(),
        egress_dir.display()
    );
    spawn_trace(
        &session_dir,
        "sandbox ready — attaching terminal (blank screen until Claude starts is normal)",
    );

    Ok(SpawnedSandbox {
        handle,
        session_id: params.session_id,
        worktree_path: repo,
        ready_marker,
        egress_dir,
        session_dir,
    })
}

#[cfg(not(target_os = "macos"))]
pub async fn spawn_claude_sandbox(_params: SpawnParams) -> Result<SpawnedSandbox> {
    anyhow::bail!(
        "tddy-sandbox-app requires macOS (darwin Seatbelt). \
         On this platform use unsandboxed tooling instead."
    )
}

/// Log paths useful when the sandbox child fails before attach.
pub fn log_spawn_diagnostics(egress_dir: &Path, session_dir: &Path) {
    let project_root = session_dir.join("sandbox");
    let logs = tddy_sandbox::format_sandbox_diagnostics(egress_dir, Some(&project_root));
    log::error!(target: "tddy_sandbox_app::spawn", "sandbox diagnostics:\n{logs}");
}
