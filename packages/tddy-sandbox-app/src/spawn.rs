//! Spawn `tddy-sandbox-runner` inside Seatbelt without a host `tddy-daemon`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tddy_daemon::sandbox_session::{
    build_sandbox_runner_env, copy_dir_all, pick_free_loopback_port, resolve_sandbox_runner_path,
    resolve_tddy_tools_path, spawn_sandbox_runner, wait_for_sandbox_ready, SandboxRunnerSpawn,
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
    /// Working directory for Claude inside the jail. Defaults to the mounted repo root.
    pub cwd: Option<PathBuf>,
    /// Persistent jail `$HOME`, mounted read-write and reused across restarts. Separate from the
    /// real host `~/.claude`.
    ///
    /// Deliberately shared across all `tddy-sandbox-app` invocations on a host, not per-session —
    /// that's the point (settings/session-history/credentials persist across restarts). Concurrent
    /// runs sharing this dir is analogous to a user running multiple concurrent `claude` CLI
    /// sessions against their real `~/.claude` today; this is not an oversight.
    pub claude_home_dir: PathBuf,
    /// Remote-codebase mode: don't mount `repo` into the jail. Claude reaches it only via
    /// `mcp__tddy-tools__*` calls, which the host relays against the real `repo` path (see
    /// `bridge::AppToolHandler`). Matches the daemon's sandboxed-session isolation model.
    pub remote_codebase: bool,
    /// Discovery subagent to wire into the in-jail `tddy-tools --mcp` process, if any.
    pub subagent: SubagentSpawnConfig,
}

/// Resolves the effective codebase mode from `--codebase-mode` and the deprecated
/// `--remote-codebase` boolean alias. Returns `true` for managed mode, `false` for mounted mode.
///
/// `--remote-codebase` predates `--codebase-mode` and remains a working alias for
/// `--codebase-mode managed`; an explicit `--codebase-mode mounted` alongside it is a
/// contradiction (the caller asked for both at once) and is rejected rather than silently
/// resolved to either value.
pub(crate) fn resolve_codebase_mode(
    codebase_mode: Option<&str>,
    remote_codebase_flag: bool,
) -> Result<bool, String> {
    match codebase_mode {
        Some("managed") => Ok(true),
        Some("mounted") => {
            if remote_codebase_flag {
                Err(
                    "conflicting codebase mode: --codebase-mode mounted was given together with \
                     --remote-codebase (which implies managed mode)"
                        .to_string(),
                )
            } else {
                Ok(false)
            }
        }
        Some(other) => Err(format!(
            "unrecognized --codebase-mode value {other:?}; expected \"mounted\" or \"managed\""
        )),
        None => Ok(remote_codebase_flag),
    }
}

/// Spawn-time discovery-subagent configuration. `discovery_subagent: None` means no subagent is
/// wired into the session.
#[derive(Default, Clone)]
pub(crate) struct SubagentSpawnConfig {
    pub discovery_subagent: Option<String>,
    pub fastcontext_url: Option<String>,
    pub fastcontext_model: Option<String>,
    pub fastcontext_max_turns: Option<u32>,
    /// `--subagent-replaces` override (comma-separated tool names). `None` means "use the
    /// subagent's declared default" — resolved via [`tddy_discovery::subagent::resolve_replaced_tools`].
    pub replaces: Option<String>,
}

/// Builds the `TDDY_SUBAGENT_*` env overlay for the in-jail `tddy-tools --mcp` process from the
/// spawn-time subagent flags. Empty when no subagent is configured. Only sets a `FASTCONTEXT_*`
/// key for a field that was actually given — env-var defaulting for absent fields happens on the
/// reading side (`tddy-tools`), not here.
pub(crate) fn subagent_env_overlay(
    config: &SubagentSpawnConfig,
) -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    let Some(ref subagent) = config.discovery_subagent else {
        return env;
    };
    env.insert("TDDY_SUBAGENT".to_string(), subagent.clone());
    if let Some(ref url) = config.fastcontext_url {
        env.insert("TDDY_SUBAGENT_FASTCONTEXT_URL".to_string(), url.clone());
    }
    if let Some(ref model) = config.fastcontext_model {
        env.insert("TDDY_SUBAGENT_FASTCONTEXT_MODEL".to_string(), model.clone());
    }
    if let Some(max_turns) = config.fastcontext_max_turns {
        env.insert(
            "TDDY_SUBAGENT_FASTCONTEXT_MAX_TURNS".to_string(),
            max_turns.to_string(),
        );
    }
    if let Some(ref replaces) = config.replaces {
        let resolved =
            tddy_discovery::subagent::resolve_replaced_tools(subagent, Some(replaces.as_str()));
        if !resolved.is_empty() {
            env.insert("TDDY_SUBAGENT_REPLACES".to_string(), resolved.join(","));
        }
    }
    env
}

/// Seed `claude_home_dir/.claude/.credentials.json` from the real host `~/.claude` once, so the
/// jail can authenticate on its first run. Never overwrites an existing file — the jail may have
/// since refreshed its own token, and the host copy must not clobber it on later restarts.
pub(crate) fn seed_claude_credentials(claude_home_dir: &Path) -> Result<()> {
    let dest_dir = claude_home_dir.join(".claude");
    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("create persistent claude home {}", dest_dir.display()))?;
    let dest = dest_dir.join(".credentials.json");
    if dest.exists() {
        return Ok(());
    }
    let Some(host_home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    let src = host_home.join(".claude").join(".credentials.json");
    if !src.is_file() {
        return Ok(());
    }
    std::fs::copy(&src, &dest)
        .with_context(|| format!("seed credentials {} -> {}", src.display(), dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Mirror the host's self-managed install layout
/// (`$HOME/.local/bin/claude` -> `$HOME/.local/share/claude/versions/<version>` -> real binary)
/// inside the persistent jail home, so Claude's own startup self-check — which looks for itself
/// at `$HOME/.local/bin/claude` — finds a consistent install instead of warning "missing or
/// broken — run claude install to repair". The actually-exec'd binary stays the resolved
/// `claude_binary` path passed to the runner; these are just symlinks pointing at the same file.
pub(crate) fn seed_claude_local_install(claude_home_dir: &Path, claude_binary: &str) -> Result<()> {
    use std::os::unix::fs::symlink;

    let real_bin = Path::new(claude_binary);
    let local_bin_dir = claude_home_dir.join(".local").join("bin");
    std::fs::create_dir_all(&local_bin_dir)
        .with_context(|| format!("create {}", local_bin_dir.display()))?;
    let local_bin_claude = local_bin_dir.join("claude");

    // Detect the installer's `.../versions/<version>/<real binary>` shape and mirror it so a
    // version-manifest check (if any) also finds a matching entry; otherwise fall back to a flat
    // symlink straight at the resolved binary.
    let link_target = if is_versioned_install_layout(real_bin) {
        mirror_versioned_symlink(claude_home_dir, real_bin)?
    } else {
        real_bin.to_path_buf()
    };

    let _ = std::fs::remove_file(&local_bin_claude);
    symlink(&link_target, &local_bin_claude).with_context(|| {
        format!(
            "symlink {} -> {}",
            local_bin_claude.display(),
            link_target.display()
        )
    })?;
    Ok(())
}

fn is_versioned_install_layout(real_bin: &Path) -> bool {
    real_bin
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "versions")
}

/// Mirror `real_bin` (`.../versions/<version>/<binary>`) under
/// `claude_home_dir/.local/share/claude/versions/<version>`, returning the mirrored symlink path.
fn mirror_versioned_symlink(claude_home_dir: &Path, real_bin: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::symlink;

    let version = real_bin
        .file_name()
        .map(|n| n.to_owned())
        .context("versioned claude binary has no file name")?;
    let versions_dir = claude_home_dir
        .join(".local")
        .join("share")
        .join("claude")
        .join("versions");
    std::fs::create_dir_all(&versions_dir)
        .with_context(|| format!("create {}", versions_dir.display()))?;
    let versioned_link = versions_dir.join(&version);
    let _ = std::fs::remove_file(&versioned_link);
    symlink(real_bin, &versioned_link).with_context(|| {
        format!(
            "symlink {} -> {}",
            versioned_link.display(),
            real_bin.display()
        )
    })?;
    Ok(versioned_link)
}

/// Resolve Claude's working directory inside the jail: the explicit `cwd` override if given,
/// else `context_dir` in remote-codebase mode (the repo isn't mounted there) or `repo` otherwise
/// (the repo is mounted read-write and Claude works on the real project tree).
pub(crate) fn resolve_jail_cwd(
    cwd: Option<&Path>,
    remote_codebase: bool,
    repo: &Path,
    context_dir: &Path,
) -> PathBuf {
    cwd.map(Path::to_path_buf).unwrap_or_else(|| {
        if remote_codebase {
            context_dir.to_path_buf()
        } else {
            repo.to_path_buf()
        }
    })
}

/// Build the list of read-write mounts passed to `spawn_sandbox_runner`: in remote-codebase mode
/// only the persistent jail home is mounted (the repo is reached only via `mcp__tddy-tools__*`
/// relayed by the host); otherwise both the repo and the jail home are mounted.
pub(crate) fn build_sandbox_mounts(
    remote_codebase: bool,
    repo: &Path,
    scratch_home: &Path,
) -> Vec<tddy_sandbox::MountSpec> {
    if remote_codebase {
        vec![tddy_sandbox::MountSpec::read_write(
            scratch_home.to_path_buf(),
        )]
    } else {
        vec![
            tddy_sandbox::MountSpec::read_write(repo.to_path_buf()),
            tddy_sandbox::MountSpec::read_write(scratch_home.to_path_buf()),
        ]
    }
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
    std::fs::create_dir_all(sandbox_root.join(".work").join("tmp"))
        .context("mkdir sandbox scratch tmp")?;
    std::fs::create_dir_all(sandbox_root.join("context")).context("mkdir sandbox context")?;
    std::fs::create_dir_all(&egress_dir).context("mkdir sandbox egress")?;
    seed_claude_credentials(&params.claude_home_dir)?;

    let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
    let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
    let scratch_dir = sandbox_root.join(".work");
    let scratch_home = std::fs::canonicalize(&params.claude_home_dir)
        .unwrap_or_else(|_| params.claude_home_dir.clone());
    let scratch_tmp = scratch_dir.join("tmp");
    let context_dir = sandbox_root.join("context");

    spawn_trace(
        &session_dir,
        &format!("preparing context from {} …", repo.display()),
    );
    let repo_for_context = repo.clone();
    let subagent_name = params.subagent.discovery_subagent.clone();
    let replaced_tools: Vec<String> = match subagent_name.as_deref() {
        Some(name) => tddy_discovery::subagent::resolve_replaced_tools(
            name,
            params.subagent.replaces.as_deref(),
        ),
        None => Vec::new(),
    };
    let ctx: SandboxContextDir = tokio::task::spawn_blocking(move || {
        let replaced_refs: Vec<&str> = replaced_tools.iter().map(String::as_str).collect();
        SandboxContextDir::create_with_subagent(
            &repo_for_context,
            subagent_name.as_deref(),
            &replaced_refs,
        )
    })
    .await
    .context("context prep task join")??;
    spawn_trace(&session_dir, "copying context into jail tree …");
    copy_dir_all(ctx.path(), &context_dir).map_err(|e| anyhow::anyhow!(e))?;
    spawn_trace(&session_dir, "context ready");

    spawn_trace(
        &session_dir,
        "resolving claude / tddy-tools / sandbox-runner paths …",
    );
    let tddy_tools_path =
        canonicalize_exec_path(&resolve_tddy_tools_path(params.tddy_tools_path.as_deref()));
    let sandbox_runner_path = params
        .sandbox_runner_path
        .clone()
        .map(|p| canonicalize_exec_path(&p))
        .unwrap_or_else(|| canonicalize_exec_path(&resolve_sandbox_runner_path()));
    let claude_binary = resolve_claude_binary(params.claude_binary.as_deref())?;
    seed_claude_local_install(&params.claude_home_dir, &claude_binary)?;

    let grpc_socket = sandbox_root.join("sandbox.grpc.sock");
    let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(&params.session_id);
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

    // Mount the repo into the jail (read-write) and start Claude there, so the agent works on the
    // real project tree instead of the (guidance-only) context dir — unless `remote_codebase` is
    // set, in which case the repo is never mounted and Claude starts in the read-only context dir,
    // reaching the real repo only via `mcp__tddy-tools__*` calls relayed by the host.
    let jail_cwd = resolve_jail_cwd(
        params.cwd.as_deref(),
        params.remote_codebase,
        &repo,
        &context_dir,
    );

    let runner_argv = vec![
        sandbox_runner_path,
        "--session-id".into(),
        params.session_id.clone(),
        "--context-dir".into(),
        context_dir.to_string_lossy().to_string(),
        "--cwd".into(),
        jail_cwd.to_string_lossy().to_string(),
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

    let mut env = build_sandbox_runner_env(
        &scratch_home,
        &scratch_tmp,
        &params.session_id,
        &tool_ipc_socket,
        &egress_dir,
    );
    env.extend(subagent_env_overlay(&params.subagent));

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
        mounts: build_sandbox_mounts(params.remote_codebase, &repo, &scratch_home),
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// `seed_claude_credentials` copies the real host `~/.claude/.credentials.json` into the jail
    /// home the first time it's called, so the jail can authenticate on its first run.
    #[test]
    #[serial]
    fn seed_claude_credentials_copies_source_file_when_dest_does_not_exist() {
        // Given
        let host_home = tempfile::tempdir().expect("temp host home");
        let claude_dir = host_home.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("mkdir host .claude");
        std::fs::write(claude_dir.join(".credentials.json"), "{\"token\":\"abc\"}")
            .expect("write host credentials");

        let claude_home_dir = tempfile::tempdir().expect("temp jail home");

        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", host_home.path());

        // When
        let result = seed_claude_credentials(claude_home_dir.path());

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        // Then
        assert!(result.is_ok(), "expected Ok(()), got: {result:?}");
        let dest = claude_home_dir
            .path()
            .join(".claude")
            .join(".credentials.json");
        let contents = std::fs::read_to_string(&dest).expect("dest credentials file must exist");
        assert_eq!(contents, "{\"token\":\"abc\"}");
    }

    /// `seed_claude_credentials` never overwrites an existing dest file — the jail may have since
    /// refreshed its own token, and the host copy must not clobber it on later restarts.
    #[test]
    #[serial]
    fn seed_claude_credentials_does_not_overwrite_existing_dest_file() {
        // Given
        let host_home = tempfile::tempdir().expect("temp host home");
        let claude_dir = host_home.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).expect("mkdir host .claude");
        std::fs::write(
            claude_dir.join(".credentials.json"),
            "{\"token\":\"from-host\"}",
        )
        .expect("write host credentials");

        let claude_home_dir = tempfile::tempdir().expect("temp jail home");
        let dest_dir = claude_home_dir.path().join(".claude");
        std::fs::create_dir_all(&dest_dir).expect("mkdir jail .claude");
        std::fs::write(
            dest_dir.join(".credentials.json"),
            "{\"token\":\"refreshed-by-jail\"}",
        )
        .expect("write existing jail credentials marker");

        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", host_home.path());

        // When
        let result = seed_claude_credentials(claude_home_dir.path());

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        // Then
        assert!(result.is_ok(), "expected Ok(()), got: {result:?}");
        let contents = std::fs::read_to_string(dest_dir.join(".credentials.json"))
            .expect("dest credentials file must still exist");
        assert_eq!(
            contents, "{\"token\":\"refreshed-by-jail\"}",
            "existing dest file must survive untouched, got: {contents}"
        );
    }

    /// `seed_claude_credentials` is a graceful no-op when the host has no `~/.claude/.credentials.json`
    /// to seed from (e.g. a fresh host, or a host that never authenticated).
    #[test]
    #[serial]
    fn seed_claude_credentials_no_ops_when_source_file_is_missing() {
        // Given
        let host_home = tempfile::tempdir().expect("temp host home");
        let claude_home_dir = tempfile::tempdir().expect("temp jail home");

        let prev_home = std::env::var_os("HOME");
        std::env::set_var("HOME", host_home.path());

        // When
        let result = seed_claude_credentials(claude_home_dir.path());

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }

        // Then
        assert!(
            result.is_ok(),
            "must no-op gracefully when source file is missing, got: {result:?}"
        );
        let dest = claude_home_dir
            .path()
            .join(".claude")
            .join(".credentials.json");
        assert!(
            !dest.exists(),
            "dest file must not be created when there's nothing to seed"
        );
    }

    /// `seed_claude_local_install` symlinks `claude_home_dir/.local/bin/claude` at the resolved
    /// binary path, so Claude's own startup self-check finds a consistent install.
    #[test]
    fn seed_claude_local_install_creates_symlink_at_local_bin_claude() {
        // Given
        let claude_home_dir = tempfile::tempdir().expect("temp jail home");
        let real_bin_dir = tempfile::tempdir().expect("temp bin dir");
        let real_bin = real_bin_dir.path().join("claude");
        std::fs::write(&real_bin, "#!/bin/sh\necho fake claude\n").expect("write fake binary");

        // When
        let result = seed_claude_local_install(claude_home_dir.path(), real_bin.to_str().unwrap());

        // Then
        assert!(result.is_ok(), "expected Ok(()), got: {result:?}");
        let local_bin_claude = claude_home_dir
            .path()
            .join(".local")
            .join("bin")
            .join("claude");
        assert!(
            local_bin_claude.is_symlink(),
            "expected a symlink at {}",
            local_bin_claude.display()
        );
        let resolved = std::fs::canonicalize(&local_bin_claude).expect("resolve symlink target");
        let expected = std::fs::canonicalize(&real_bin).expect("resolve real bin");
        assert_eq!(
            resolved, expected,
            "symlink must point at the given binary path"
        );
    }

    /// When the binary's parent directory is literally named `versions` (the self-managed
    /// install layout), `seed_claude_local_install` also mirrors a versioned symlink under
    /// `.local/share/claude/versions/<version>` so a version-manifest check finds a match too.
    #[test]
    fn seed_claude_local_install_mirrors_versioned_symlink_when_parent_dir_is_versions() {
        // Given
        let claude_home_dir = tempfile::tempdir().expect("temp jail home");
        let install_root = tempfile::tempdir().expect("temp install root");
        let versions_dir = install_root.path().join("versions");
        std::fs::create_dir_all(&versions_dir).expect("mkdir versions dir");
        let real_bin = versions_dir.join("1.2.3");
        std::fs::write(&real_bin, "#!/bin/sh\necho fake claude\n").expect("write fake binary");

        // When
        let result = seed_claude_local_install(claude_home_dir.path(), real_bin.to_str().unwrap());

        // Then
        assert!(result.is_ok(), "expected Ok(()), got: {result:?}");
        let versioned_link = claude_home_dir
            .path()
            .join(".local")
            .join("share")
            .join("claude")
            .join("versions")
            .join("1.2.3");
        assert!(
            versioned_link.is_symlink(),
            "expected a versioned mirror symlink at {}",
            versioned_link.display()
        );
        let resolved = std::fs::canonicalize(&versioned_link).expect("resolve versioned symlink");
        let expected = std::fs::canonicalize(&real_bin).expect("resolve real bin");
        assert_eq!(
            resolved, expected,
            "versioned symlink must point at the real binary"
        );
    }

    /// `build_sandbox_mounts` mounts only the repo and the persistent jail home — in that order —
    /// when not in remote-codebase mode.
    #[test]
    fn build_sandbox_mounts_mounts_repo_then_scratch_home_when_not_remote_codebase() {
        // Given
        let repo = PathBuf::from("/tmp/repo");
        let scratch_home = PathBuf::from("/tmp/scratch-home");

        // When
        let mounts = build_sandbox_mounts(false, &repo, &scratch_home);

        // Then
        assert_eq!(
            mounts.iter().map(|m| m.host.clone()).collect::<Vec<_>>(),
            vec![repo, scratch_home],
            "expected exactly [repo, scratch_home] in that order"
        );
    }

    /// `build_sandbox_mounts` mounts only the persistent jail home in remote-codebase mode — the
    /// repo is reached only via `mcp__tddy-tools__*` calls relayed by the host, never mounted.
    #[test]
    fn build_sandbox_mounts_mounts_only_scratch_home_when_remote_codebase() {
        // Given
        let repo = PathBuf::from("/tmp/repo");
        let scratch_home = PathBuf::from("/tmp/scratch-home");

        // When
        let mounts = build_sandbox_mounts(true, &repo, &scratch_home);

        // Then
        assert_eq!(
            mounts.iter().map(|m| m.host.clone()).collect::<Vec<_>>(),
            vec![scratch_home],
            "expected exactly [scratch_home] alone"
        );
    }

    /// `resolve_jail_cwd` starts Claude in the read-only context dir when in remote-codebase mode
    /// and no explicit `cwd` override was given.
    #[test]
    fn resolve_jail_cwd_returns_context_dir_when_remote_codebase_and_no_explicit_cwd() {
        // Given
        let repo = PathBuf::from("/tmp/repo");
        let context_dir = PathBuf::from("/tmp/context");

        // When
        let jail_cwd = resolve_jail_cwd(None, true, &repo, &context_dir);

        // Then
        assert_eq!(jail_cwd, context_dir);
    }

    /// `resolve_jail_cwd` starts Claude at the mounted repo root when not in remote-codebase mode
    /// and no explicit `cwd` override was given.
    #[test]
    fn resolve_jail_cwd_returns_repo_when_not_remote_codebase_and_no_explicit_cwd() {
        // Given
        let repo = PathBuf::from("/tmp/repo");
        let context_dir = PathBuf::from("/tmp/context");

        // When
        let jail_cwd = resolve_jail_cwd(None, false, &repo, &context_dir);

        // Then
        assert_eq!(jail_cwd, repo);
    }

    /// `resolve_jail_cwd` always honors an explicit `cwd` override verbatim, regardless of
    /// remote-codebase mode.
    #[test]
    fn resolve_jail_cwd_returns_explicit_cwd_verbatim_regardless_of_remote_codebase() {
        // Given
        let repo = PathBuf::from("/tmp/repo");
        let context_dir = PathBuf::from("/tmp/context");
        let explicit_cwd = PathBuf::from("/tmp/explicit");

        // When
        let jail_cwd_remote = resolve_jail_cwd(Some(&explicit_cwd), true, &repo, &context_dir);
        let jail_cwd_local = resolve_jail_cwd(Some(&explicit_cwd), false, &repo, &context_dir);

        // Then
        assert_eq!(jail_cwd_remote, explicit_cwd);
        assert_eq!(jail_cwd_local, explicit_cwd);
    }

    // ─── Managed-codebase mode + discovery subagent wiring ─────────────────────────
    //
    // Feature: docs/ft/coder/managed-codebase-subagents.md (criteria 11-12)
    // Changeset: docs/dev/1-WIP/2026-07-01-changeset-managed-codebase-subagents.md

    /// `--codebase-mode managed` resolves to managed mode (`true`), independent of the deprecated
    /// `--remote-codebase` boolean flag.
    #[test]
    fn resolve_codebase_mode_returns_true_for_explicit_managed_mode() {
        // Given / When
        let managed = resolve_codebase_mode(Some("managed"), false)
            .expect("'managed' must be a valid codebase mode");

        // Then
        assert!(
            managed,
            "--codebase-mode managed must resolve to managed mode"
        );
    }

    /// `--codebase-mode mounted` resolves to unmanaged mode (`false`).
    #[test]
    fn resolve_codebase_mode_returns_false_for_explicit_mounted_mode() {
        // Given / When
        let managed = resolve_codebase_mode(Some("mounted"), false)
            .expect("'mounted' must be a valid codebase mode");

        // Then
        assert!(
            !managed,
            "--codebase-mode mounted must resolve to unmanaged mode"
        );
    }

    /// With no `--codebase-mode` given, the deprecated `--remote-codebase` boolean flag remains a
    /// working alias for managed mode.
    #[test]
    fn resolve_codebase_mode_treats_remote_codebase_flag_as_a_managed_alias() {
        // Given / When
        let managed = resolve_codebase_mode(None, true)
            .expect("the --remote-codebase alias must resolve without error");

        // Then
        assert!(
            managed,
            "--remote-codebase must remain equivalent to --codebase-mode managed"
        );
    }

    /// With neither flag given, the default is unmanaged (mounted) mode — today's non-remote
    /// default behavior is preserved.
    #[test]
    fn resolve_codebase_mode_defaults_to_unmanaged_when_neither_flag_is_given() {
        // Given / When
        let managed =
            resolve_codebase_mode(None, false).expect("the default must resolve without error");

        // Then
        assert!(
            !managed,
            "default codebase mode must be unmanaged (mounted)"
        );
    }

    /// An explicit `--codebase-mode mounted` together with the deprecated `--remote-codebase` flag
    /// is a contradictory combination — it must be rejected, not silently resolved to either value.
    #[test]
    fn resolve_codebase_mode_errors_when_flags_conflict() {
        // Given / When
        let result = resolve_codebase_mode(Some("mounted"), true);

        // Then
        assert!(
            result.is_err(),
            "conflicting --codebase-mode mounted + --remote-codebase must be rejected"
        );
    }

    /// An unrecognized `--codebase-mode` value is a typed error, not a silent fallback.
    #[test]
    fn resolve_codebase_mode_errors_on_an_unrecognized_value() {
        // Given / When
        let result = resolve_codebase_mode(Some("bogus"), false);

        // Then
        assert!(
            result.is_err(),
            "an unrecognized --codebase-mode value must be rejected"
        );
    }

    /// With no discovery subagent configured, the env overlay is empty — nothing is threaded into
    /// the in-jail `tddy-tools --mcp` process.
    #[test]
    fn subagent_env_overlay_is_empty_when_no_discovery_subagent_is_configured() {
        // Given
        let config = SubagentSpawnConfig::default();

        // When
        let overlay = subagent_env_overlay(&config);

        // Then
        assert!(
            overlay.is_empty(),
            "overlay must be empty with no discovery subagent configured; got: {overlay:?}"
        );
    }

    /// With `--discovery-subagent fastcontext` and all FastContext flags given, every one of them
    /// is threaded into the `TDDY_SUBAGENT_*` env overlay.
    #[test]
    fn subagent_env_overlay_carries_every_configured_fastcontext_field() {
        // Given
        let config = SubagentSpawnConfig {
            discovery_subagent: Some("fastcontext".to_string()),
            fastcontext_url: Some("http://localhost:30000".to_string()),
            fastcontext_model: Some("microsoft/FastContext-1.0-4B-RL".to_string()),
            fastcontext_max_turns: Some(6),
            replaces: None,
        };

        // When
        let overlay = subagent_env_overlay(&config);

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT").map(String::as_str),
            Some("fastcontext")
        );
        assert_eq!(
            overlay
                .get("TDDY_SUBAGENT_FASTCONTEXT_URL")
                .map(String::as_str),
            Some("http://localhost:30000")
        );
        assert_eq!(
            overlay
                .get("TDDY_SUBAGENT_FASTCONTEXT_MODEL")
                .map(String::as_str),
            Some("microsoft/FastContext-1.0-4B-RL")
        );
        assert_eq!(
            overlay
                .get("TDDY_SUBAGENT_FASTCONTEXT_MAX_TURNS")
                .map(String::as_str),
            Some("6")
        );
    }

    /// Optional FastContext fields left unset are simply absent from the overlay — this function
    /// never invents a default value; env-var defaulting happens on the reading side
    /// (`tddy-tools`), not here.
    #[test]
    fn subagent_env_overlay_omits_unset_optional_fastcontext_fields() {
        // Given
        let config = SubagentSpawnConfig {
            discovery_subagent: Some("fastcontext".to_string()),
            fastcontext_url: None,
            fastcontext_model: None,
            fastcontext_max_turns: None,
            replaces: None,
        };

        // When
        let overlay = subagent_env_overlay(&config);

        // Then
        assert_eq!(
            overlay.keys().collect::<Vec<_>>(),
            vec!["TDDY_SUBAGENT"],
            "only TDDY_SUBAGENT must be set when the optional fields are all None; got: {overlay:?}"
        );
    }

    // ─── TDDY_SUBAGENT_REPLACES ──────────────────────────────────────────────────
    //
    // Feature: docs/ft/coder/managed-codebase-subagents.md § Tool replacement (criterion 17)
    // Changeset: docs/dev/1-WIP/2026-07-02-changeset-subagent-tool-replacement.md

    /// With `replaces: None` (no `--subagent-replaces` flag), the overlay never invents the
    /// per-subagent default — `TDDY_SUBAGENT_REPLACES` is simply absent, exactly like the
    /// `TDDY_SUBAGENT_FASTCONTEXT_*` fields when their source field is unset.
    #[test]
    fn subagent_env_overlay_omits_replaces_when_not_explicitly_given() {
        // Given
        let config = SubagentSpawnConfig {
            discovery_subagent: Some("fastcontext".to_string()),
            fastcontext_url: None,
            fastcontext_model: None,
            fastcontext_max_turns: None,
            replaces: None,
        };

        // When
        let overlay = subagent_env_overlay(&config);

        // Then
        assert!(
            !overlay.contains_key("TDDY_SUBAGENT_REPLACES"),
            "TDDY_SUBAGENT_REPLACES must be absent when replaces is None; got: {overlay:?}"
        );
    }

    /// With `--subagent-replaces read`, the overlay carries the normalized, CSV-joined resolved
    /// set — not the raw override string — so a jail that only trusts the env var still sees
    /// canonical tool casing.
    #[test]
    fn subagent_env_overlay_carries_the_normalized_replaces_override() {
        // Given
        let config = SubagentSpawnConfig {
            discovery_subagent: Some("fastcontext".to_string()),
            fastcontext_url: None,
            fastcontext_model: None,
            fastcontext_max_turns: None,
            replaces: Some("read".to_string()),
        };

        // When
        let overlay = subagent_env_overlay(&config);

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT_REPLACES").map(String::as_str),
            Some("Read")
        );
    }
}
