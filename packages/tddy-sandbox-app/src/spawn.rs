//! Spawn `tddy-sandbox-runner` inside Seatbelt without a host `tddy-daemon`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tddy_daemon::sandbox_session::{
    build_sandbox_runner_env, copy_dir_all, pick_free_loopback_port, resolve_sandbox_runner_path,
    resolve_tddy_tools_path, spawn_sandbox_runner, wait_for_sandbox_ready, SandboxRunnerSpawn,
};
use tddy_sandbox::{append_line, SandboxContextDir, SandboxHandle, SubagentReplacement};

use crate::codebase_mode::CodebaseMode;

pub(crate) fn spawn_trace(session_dir: &Path, message: &str) {
    eprintln!("{message}");
    let trace = session_dir.join("spawn.trace.log");
    let _ = append_line(&trace, message);
}

/// [`spawn_trace`] without the copy to stderr: the same session trace file, for the things a
/// session has to say *after* it stops owning the terminal.
///
/// Every other trace here runs before the agent starts, when stderr is still this process's to
/// write on. A `sandboxed` session then hands the real controlling terminal to `claude`, which
/// draws its own UI on it — so a line written from a background task lands in the middle of
/// someone else's rendering, and there is no reason for the operator reading it later to be the
/// same person watching the screen now. The file is where the session's own record belongs;
/// `tail -f <session-dir>/spawn.trace.log` reads it without touching the agent's canvas.
pub(crate) fn spawn_trace_quietly(session_dir: &Path, message: &str) {
    let trace = session_dir.join("spawn.trace.log");
    let _ = append_line(&trace, message);
}

/// Agent kind for the in-jail CLI: `claude` (default) or `cursor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    Claude,
    Cursor,
}

impl AgentKind {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "claude" => Ok(Self::Claude),
            "cursor" => Ok(Self::Cursor),
            other => Err(format!(
                "unrecognized --agent-kind {other:?}; expected \"claude\" or \"cursor\""
            )),
        }
    }
}

/// Parameters for a local sandboxed Claude or Cursor session.
pub struct SpawnParams {
    pub agent_kind: AgentKind,
    pub repo: PathBuf,
    pub session_id: String,
    pub model: String,
    pub permission_mode: String,
    pub claude_binary: Option<String>,
    /// Path to the `agent` binary when `agent_kind` is Cursor (default: `agent` on PATH).
    pub cursor_binary: Option<String>,
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
    /// Persistent jail `$HOME` for Cursor (`agent`) when `agent_kind` is Cursor.
    pub cursor_home_dir: PathBuf,
    /// Where the checkout lives relative to the jail, and which side of it the agent is on.
    pub codebase_mode: CodebaseMode,
    /// Already-resolved specialized-agent defs to wire into the in-jail `tddy-tools --mcp` process
    /// (see `crate::config::resolve_session_agents`). Empty means no subagent is wired.
    pub specialized_defs: Vec<tddy_discovery::agent_def::SpecializedAgentDef>,
    /// Extra args forwarded verbatim to the in-jail `claude` invocation (relayed to
    /// `tddy-sandbox-runner` as repeated `--claude-arg` tokens).
    pub claude_args: Vec<String>,
    /// `RUST_LOG` for the in-jail `tddy-tools --mcp` server (relayed as `--mcp-log-level`); `None`
    /// lets the runner pick its default.
    pub mcp_log_level: Option<String>,
}

/// Build the (name, replaced-tools) pairs for a session's resolved specialized-agent defs — each
/// its own name + its own YAML-declared `replaces`, normalized.
pub(crate) fn specialized_agent_replacement_pairs(
    defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
) -> Vec<(String, Vec<String>)> {
    defs.iter()
        .map(|def| {
            (
                def.name.clone(),
                tddy_discovery::subagent::normalize_replaced_tools(&def.replaces),
            )
        })
        .collect()
}

/// Builds the `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON`/(single-agent) `TDDY_SUBAGENT_REPLACES` jail
/// env overlay for the in-jail `tddy-tools --mcp` process from already-resolved specialized-agent
/// defs. Empty when no agent is configured.
pub(crate) fn subagent_env_overlay(
    defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
) -> std::collections::BTreeMap<String, String> {
    let mut env = std::collections::BTreeMap::new();
    if defs.is_empty() {
        return env;
    }
    let names = defs
        .iter()
        .map(|d| d.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    env.insert("TDDY_SUBAGENT".to_string(), names);
    if let Ok(defs_json) = serde_json::to_string(defs) {
        env.insert("TDDY_SUBAGENTS_JSON".to_string(), defs_json);
    }
    if defs.len() == 1 {
        let (_, replaced) = &specialized_agent_replacement_pairs(defs)[0];
        if !replaced.is_empty() {
            env.insert("TDDY_SUBAGENT_REPLACES".to_string(), replaced.join(","));
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

/// Build the list of read-write mounts passed to `spawn_sandbox_runner`.
///
/// `Managed` is the one mode that leaves the checkout outside: the agent reaches it only via
/// `mcp__tddy-tools__*` calls the host relays, so mounting it would hand back the direct route the
/// mode exists to remove. `Mounted` and `Sandboxed` both mount it read-write for opposite reasons
/// — in `Mounted` the agent is in the jail and needs the tree it works on, in `Sandboxed` the tree
/// is what the jail is *for*.
pub(crate) fn build_sandbox_mounts(
    mode: CodebaseMode,
    repo: &Path,
    scratch_home: &Path,
) -> Vec<tddy_sandbox::MountSpec> {
    let mut mounts = Vec::new();
    if mode != CodebaseMode::Managed {
        mounts.push(tddy_sandbox::MountSpec::read_write(repo));
    }
    mounts.push(tddy_sandbox::MountSpec::read_write(scratch_home));
    mounts
}

/// What a `--workspace-tools` jail is spawned with: the no-agent runner form a `sandboxed` session
/// uses, where the jail serves the host's tool calls against the mounted checkout instead of
/// hosting an agent of its own.
pub(crate) struct WorkspaceToolsRunnerArgs {
    pub sandbox_runner_path: String,
    pub session_id: String,
    /// The checkout, mounted read-write at this same path inside the jail.
    pub repo: PathBuf,
    pub context_dir: PathBuf,
    pub tool_ipc_socket: PathBuf,
    pub tddy_tools_path: String,
    pub ready_marker: PathBuf,
    pub grpc_socket: PathBuf,
    /// The app attaches over its existing loopback-gRPC transport, not over `--stdio`.
    pub grpc_listen_port: u16,
    /// A jail that runs the build needs the CONNECT relay the agent used to use from the other
    /// side of it.
    pub egress_shim_port: u16,
}

/// Build the argv for the no-agent jail a `sandboxed` session spawns.
///
/// Deliberately not a branch inside the agent-hosting argv builder: the two share no flag that
/// matters. There is no model, no permission mode, no agent binary and no pass-through agent args,
/// because there is no agent in this jail to give them to.
pub(crate) fn build_workspace_tools_runner_argv(args: WorkspaceToolsRunnerArgs) -> Vec<String> {
    let WorkspaceToolsRunnerArgs {
        sandbox_runner_path,
        session_id,
        repo,
        context_dir,
        tool_ipc_socket,
        tddy_tools_path,
        ready_marker,
        grpc_socket,
        grpc_listen_port,
        egress_shim_port,
    } = args;

    vec![
        sandbox_runner_path,
        "--session-id".into(),
        session_id,
        "--context-dir".into(),
        context_dir.to_string_lossy().into_owned(),
        "--tool-ipc-socket".into(),
        tool_ipc_socket.to_string_lossy().into_owned(),
        "--tddy-tools-path".into(),
        tddy_tools_path,
        "--ready-marker".into(),
        ready_marker.to_string_lossy().into_owned(),
        "--grpc-socket".into(),
        grpc_socket.to_string_lossy().into_owned(),
        // What makes this the no-agent form: the runner serves the host's `in_jail_tool_request`s
        // against the checkout as mounted here, and spawns no PTY and no in-jail MCP server.
        "--workspace-tools".into(),
        repo.to_string_lossy().into_owned(),
        // The app attaches over loopback gRPC, the transport it already drives every other session
        // with — the daemon's `--stdio` flavour of this same jail has no place here, because the
        // app does not own this process's stdio: the host agent does.
        "--grpc-listen-port".into(),
        grpc_listen_port.to_string(),
        // A jail that runs `cargo build` needs the network the daemon's workspace jail never did,
        // and reaches it through the CONNECT tunnels the host relay fulfills.
        "--egress-shim-port".into(),
        egress_shim_port.to_string(),
    ]
}

pub(crate) fn seed_cursor_credentials(cursor_home_dir: &Path) -> Result<()> {
    let dest_dir = cursor_home_dir.join(".cursor");
    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("create persistent cursor home {}", dest_dir.display()))?;
    let Some(host_home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    let host_cursor = host_home.join(".cursor");
    for name in ["auth.json", "cli-config.json"] {
        let src = host_cursor.join(name);
        let dest = dest_dir.join(name);
        if dest.exists() || !src.is_file() {
            continue;
        }
        std::fs::copy(&src, &dest).with_context(|| {
            format!(
                "seed cursor credentials {} -> {}",
                src.display(),
                dest.display()
            )
        })?;
        #[cfg(unix)]
        if name == "auth.json" {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
        }
    }
    Ok(())
}

/// A sandboxed Claude or Cursor session ready for host `SessionChannel` attach.
pub struct SpawnedSandbox {
    pub handle: SandboxHandle,
    pub session_id: String,
    pub worktree_path: PathBuf,
    pub ready_marker: PathBuf,
    pub egress_dir: PathBuf,
    pub session_dir: PathBuf,
}

pub(crate) fn canonicalize_exec_path(path: &str) -> String {
    if path.contains('/') {
        std::fs::canonicalize(path)
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string())
    } else {
        path.to_string()
    }
}

fn resolve_cursor_binary(configured: Option<&str>) -> Result<String> {
    let name = configured
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("agent");
    if name.contains('/') {
        let path = Path::new(name);
        anyhow::ensure!(
            path.is_file() || path.is_symlink(),
            "cursor agent binary not found at {}",
            path.display()
        );
        return Ok(canonicalize_exec_path(name));
    }
    let which_out = std::process::Command::new("which")
        .arg(name)
        .output()
        .context("run which to locate agent")?;
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
        "cursor agent binary {name:?} not found on host PATH.\n\
         Pass an absolute path, e.g.: --cursor-binary $(which agent)"
    );
}

/// Resolve the `claude` binary to exec: an explicit path as given, or the first `claude` on the
/// host PATH. Never a bare name — the jail's PATH is not this host's, and a session that cannot
/// name its agent binary should fail here rather than inside the jail.
pub fn resolve_claude_binary(configured: Option<&str>) -> Result<String> {
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
    let scratch_home_dir = match params.agent_kind {
        AgentKind::Claude => &params.claude_home_dir,
        AgentKind::Cursor => &params.cursor_home_dir,
    };
    match params.agent_kind {
        AgentKind::Claude => seed_claude_credentials(scratch_home_dir)?,
        AgentKind::Cursor => seed_cursor_credentials(scratch_home_dir)?,
    }

    let sandbox_root = std::fs::canonicalize(&sandbox_root).unwrap_or(sandbox_root);
    let egress_dir = std::fs::canonicalize(&egress_dir).unwrap_or(egress_dir);
    let scratch_dir = sandbox_root.join(".work");
    let scratch_home =
        std::fs::canonicalize(scratch_home_dir).unwrap_or_else(|_| scratch_home_dir.clone());
    let scratch_tmp = scratch_dir.join("tmp");
    let context_dir = sandbox_root.join("context");

    spawn_trace(
        &session_dir,
        &format!("preparing context from {} …", repo.display()),
    );
    let repo_for_context = repo.clone();
    // The jail's cwd carries what *this* agent reads for guidance and nothing else: `--agent-kind`
    // is already spelled the way tddy-core's per-backend table is keyed, so no mapping is needed
    // here (unlike the daemon, which dispatches on session types).
    let context_globs = tddy_core::backend::context_globs_for_agent(match params.agent_kind {
        AgentKind::Claude => "claude",
        AgentKind::Cursor => "cursor",
    });
    let specialized_defs = params.specialized_defs;
    let replacement_pairs = specialized_agent_replacement_pairs(&specialized_defs);
    let ctx: SandboxContextDir = tokio::task::spawn_blocking(move || {
        let replacement_refs: Vec<Vec<&str>> = replacement_pairs
            .iter()
            .map(|(_, tools)| tools.iter().map(String::as_str).collect())
            .collect();
        let replacements: Vec<SubagentReplacement<'_>> = replacement_pairs
            .iter()
            .zip(replacement_refs.iter())
            .map(|((name, _), refs)| SubagentReplacement {
                name,
                replaced: refs,
            })
            .collect();
        SandboxContextDir::create_with_subagent(&repo_for_context, &replacements, context_globs)
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
    let cursor_binary = resolve_cursor_binary(params.cursor_binary.as_deref())?;
    if params.agent_kind == AgentKind::Claude {
        seed_claude_local_install(scratch_home_dir, &claude_binary)?;
    } else {
        #[cfg(unix)]
        tddy_sandbox_recipes::seed_cursor_local_install(scratch_home_dir, &cursor_binary)?;
    }

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
        params.codebase_mode == CodebaseMode::Managed,
        &repo,
        &context_dir,
    );

    // Read the host's own controlling terminal size now, before `bridge::run_terminal_bridge`
    // hands stdin over to the jail — so the jail's PTY opens at the right size from the very
    // first frame instead of starting at a hardcoded default and waiting on a live resize (which
    // never fires if the user's terminal never actually changes size after attach) to correct it.
    let (initial_rows, initial_cols) = crate::bridge::terminal_size_or_default();

    let mut runner_argv = vec![
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
        "--model".into(),
        params.model.clone(),
        "--grpc-listen-port".into(),
        grpc_listen_port.to_string(),
        "--egress-shim-port".into(),
        egress_shim_port.to_string(),
        "--initial-cols".into(),
        initial_cols.to_string(),
        "--initial-rows".into(),
        initial_rows.to_string(),
    ];
    match params.agent_kind {
        AgentKind::Claude => {
            runner_argv.push("--claude-binary".into());
            runner_argv.push(claude_binary.clone());
            runner_argv.push("--permission-mode".into());
            runner_argv.push(perm.to_string());
            for claude_arg in &params.claude_args {
                runner_argv.push("--claude-arg".into());
                runner_argv.push(claude_arg.clone());
            }
        }
        AgentKind::Cursor => {
            runner_argv.push("--agent-kind".into());
            runner_argv.push("cursor".into());
            runner_argv.push("--agent-binary".into());
            runner_argv.push(cursor_binary.clone());
            for agent_arg in &params.claude_args {
                runner_argv.push("--agent-arg".into());
                runner_argv.push(agent_arg.clone());
            }
        }
    }
    if let Some(level) = &params.mcp_log_level {
        runner_argv.push("--mcp-log-level".into());
        runner_argv.push(level.clone());
    }

    let mut env = build_sandbox_runner_env(
        &scratch_home,
        &scratch_tmp,
        &params.session_id,
        &tool_ipc_socket,
        &egress_dir,
    );
    env.extend(subagent_env_overlay(&specialized_defs));

    // Expose the language-agnostic `Lsp*` MCP tools in the jail only when a language server is
    // available for this repo on the host.
    if tddy_core::toolcall::lsp::lsp_executor()
        .map(|ex| ex.is_available(&params.repo))
        .unwrap_or(false)
    {
        env.insert("TDDY_LSP_TOOLS".to_string(), "rust".to_string());
    }

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
        mounts: build_sandbox_mounts(params.codebase_mode, &repo, &scratch_home),
        // Preserve prior behavior (build_sandbox_plan used to hardcode $HOME): the recipe's
        // per-session credential copy stays enabled for the app path.
        host_home: std::env::var_os("HOME").map(PathBuf::from),
        // Standalone app path has no daemon config; empty config lets the cgroups backend derive
        // the delegated base at runtime (ignored by the macOS/QEMU backends).
        cgroup: tddy_sandbox::CgroupConfig::default(),
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

    // ─── The session's own record, once the terminal is somebody else's ─────────

    /// A `sandboxed` session hands the controlling terminal to `claude`, so what its background
    /// tasks have to say cannot go to stderr — but it still has to be *somewhere*, or the one
    /// event that explains a wedged tool socket exists nowhere at all.
    #[test]
    fn a_quiet_trace_still_lands_in_the_sessions_trace_log() {
        // Given
        let session_dir = tempfile::tempdir().expect("temp session dir");

        // When
        spawn_trace_quietly(
            session_dir.path(),
            "the host tool IPC socket could not accept",
        );

        // Then
        let trace = std::fs::read_to_string(session_dir.path().join("spawn.trace.log"))
            .expect("a quiet trace must still write the session's trace log");
        assert!(
            trace.contains("the host tool IPC socket could not accept"),
            "the trace log must hold what was not printed; it held: {trace}"
        );
    }

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

    // ─── Codebase-mode placement ────────────────────────────────────────────────
    //
    // Feature: docs/ft/coder/sandboxed-codebase-mode.md (criteria 2, 3)
    // Changeset: docs/dev/1-WIP/2026-09-05-sandboxed-codebase-mode.md

    /// Mounted mode mounts the repo and the persistent jail home, in that order — the agent works
    /// on the real project tree from inside the jail.
    #[test]
    fn build_sandbox_mounts_mounts_the_repo_and_scratch_home_in_mounted_mode() {
        // Given
        let repo = PathBuf::from("/tmp/repo");
        let scratch_home = PathBuf::from("/tmp/scratch-home");

        // When
        let mounts = build_sandbox_mounts(CodebaseMode::Mounted, &repo, &scratch_home);

        // Then
        assert_eq!(
            mounts.iter().map(|m| m.host.clone()).collect::<Vec<_>>(),
            vec![repo, scratch_home],
            "expected exactly [repo, scratch_home] in that order"
        );
    }

    /// Managed mode mounts only the persistent jail home — the repo is reached via
    /// `mcp__tddy-tools__*` calls relayed by the host, never mounted.
    #[test]
    fn build_sandbox_mounts_mounts_only_the_scratch_home_in_managed_mode() {
        // Given
        let repo = PathBuf::from("/tmp/repo");
        let scratch_home = PathBuf::from("/tmp/scratch-home");

        // When
        let mounts = build_sandbox_mounts(CodebaseMode::Managed, &repo, &scratch_home);

        // Then
        assert_eq!(
            mounts.iter().map(|m| m.host.clone()).collect::<Vec<_>>(),
            vec![scratch_home],
            "expected exactly [scratch_home] alone"
        );
    }

    /// Sandboxed mode is the placement where the *code* is what the jail holds, so the repo is
    /// mounted read-write — the opposite of managed mode, which shares its "the agent has no
    /// direct route to the checkout" property but reaches it the other way round.
    #[test]
    fn build_sandbox_mounts_mounts_the_repo_and_scratch_home_in_sandboxed_mode() {
        // Given
        let repo = PathBuf::from("/tmp/repo");
        let scratch_home = PathBuf::from("/tmp/scratch-home");

        // When
        let mounts = build_sandbox_mounts(CodebaseMode::Sandboxed, &repo, &scratch_home);

        // Then
        assert_eq!(
            mounts.iter().map(|m| m.host.clone()).collect::<Vec<_>>(),
            vec![repo, scratch_home],
            "the checkout must be inside the jail in sandboxed mode"
        );
    }

    // ─── The no-agent jail a sandboxed session spawns ───────────────────────────

    fn a_workspace_tools_runner_argv() -> Vec<String> {
        build_workspace_tools_runner_argv(WorkspaceToolsRunnerArgs {
            sandbox_runner_path: "/opt/tddy/tddy-sandbox-runner".to_string(),
            session_id: "sandboxed-codebase-session".to_string(),
            repo: PathBuf::from("/tmp/repo"),
            context_dir: PathBuf::from("/tmp/session/sandbox/context"),
            tool_ipc_socket: PathBuf::from("/tmp/tool_ipc.sock"),
            tddy_tools_path: "/opt/tddy/tddy-tools".to_string(),
            ready_marker: PathBuf::from("/tmp/session/sandbox/sandbox.ready"),
            grpc_socket: PathBuf::from("/tmp/session/sandbox/sandbox.grpc.sock"),
            grpc_listen_port: 45_501,
            egress_shim_port: 45_502,
        })
    }

    /// The value following `flag` in an argv, if the flag is present.
    fn value_after(argv: &[String], flag: &str) -> Option<String> {
        argv.iter()
            .position(|arg| arg == flag)
            .and_then(|i| argv.get(i + 1))
            .cloned()
    }

    /// The jail is spawned in the no-agent form, pointed at the checkout it serves tool calls
    /// against.
    #[test]
    fn a_sandboxed_codebase_jail_is_spawned_in_the_no_agent_workspace_tools_form() {
        // Given / When
        let argv = a_workspace_tools_runner_argv();

        // Then
        assert_eq!(
            value_after(&argv, "--workspace-tools").as_deref(),
            Some("/tmp/repo"),
            "the jail must serve tool calls against the checkout; argv was: {argv:?}"
        );
    }

    /// There is no agent in this jail, so nothing in its argv may describe one. A `--model` or
    /// `--claude-binary` reaching it would mean the wrong runner mode was spawned.
    #[test]
    fn a_sandboxed_codebase_jail_is_spawned_without_an_agent_binary_model_or_permission_mode() {
        // Given / When
        let argv = a_workspace_tools_runner_argv();

        // Then
        for agent_flag in [
            "--claude-binary",
            "--model",
            "--permission-mode",
            "--claude-arg",
            "--agent-kind",
            "--agent-binary",
        ] {
            assert!(
                !argv.iter().any(|arg| arg == agent_flag),
                "a jail with no agent must not carry {agent_flag}; argv was: {argv:?}"
            );
        }
    }

    /// The build runs in here, and a build fetches dependencies. The jail has no network of its
    /// own, so it is given the shim port whose CONNECT tunnels the host relay fulfills.
    #[test]
    fn a_sandboxed_codebase_jail_carries_an_egress_shim_port_for_its_build() {
        // Given / When
        let argv = a_workspace_tools_runner_argv();

        // Then
        assert_eq!(
            value_after(&argv, "--egress-shim-port").as_deref(),
            Some("45502"),
            "a jail that runs the build needs the host CONNECT relay; argv was: {argv:?}"
        );
    }

    /// The app attaches over the loopback-gRPC transport it already uses for every other session,
    /// not over the `--stdio` transport the daemon drives its workspace jails with.
    #[test]
    fn a_sandboxed_codebase_jail_is_reachable_over_the_apps_loopback_grpc_transport() {
        // Given / When
        let argv = a_workspace_tools_runner_argv();

        // Then
        assert_eq!(
            value_after(&argv, "--grpc-listen-port").as_deref(),
            Some("45501"),
            "argv was: {argv:?}"
        );
        assert!(
            !argv.iter().any(|arg| arg == "--stdio"),
            "the app owns no piped stdio for this jail; argv was: {argv:?}"
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

    // ─── subagent_env_overlay ────────────────────────────────────────────────────
    //
    // Feature: docs/ft/coder/specialized-subagents.md, docs/ft/coder/managed-codebase-subagents.md
    // § Tool replacement

    fn a_def(name: &str, replaces: &[&str]) -> tddy_discovery::agent_def::SpecializedAgentDef {
        tddy_discovery::agent_def::SpecializedAgentDef {
            name: name.to_string(),
            label: None,
            model: "some-model".to_string(),
            base_url: "http://localhost:30000".to_string(),
            // These tests cover tool replacement in the env overlay, which reads no credential.
            api_key: None,
            system_prompt: None,
            system_prompt_path: None,
            tools: vec![tddy_discovery::agent_def::SubagentTool::Read],
            max_turns: 10,
            replaces: replaces.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// With no defs given, the env overlay is empty — nothing is threaded into the in-jail
    /// `tddy-tools --mcp` process.
    #[test]
    fn subagent_env_overlay_is_empty_when_no_defs_are_given() {
        // Given / When
        let overlay = subagent_env_overlay(&[]);

        // Then
        assert!(
            overlay.is_empty(),
            "overlay must be empty with no defs given; got: {overlay:?}"
        );
    }

    /// A single resolved def carries `TDDY_SUBAGENT` (its name) and `TDDY_SUBAGENTS_JSON` (the
    /// serialized def).
    #[test]
    fn subagent_env_overlay_carries_name_and_json_for_a_single_def() {
        // Given
        let defs = vec![a_def("explorer", &["Grep", "Glob"])];

        // When
        let overlay = subagent_env_overlay(&defs);

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT").map(String::as_str),
            Some("explorer")
        );
        let defs_json = overlay
            .get("TDDY_SUBAGENTS_JSON")
            .expect("TDDY_SUBAGENTS_JSON must be present");
        assert!(
            defs_json.contains("explorer"),
            "TDDY_SUBAGENTS_JSON must serialize the def; got: {defs_json}"
        );
    }

    /// Multiple resolved defs carry a comma-joined `TDDY_SUBAGENT` name list and no
    /// `TDDY_SUBAGENT_REPLACES` (that key is single-agent-only).
    #[test]
    fn subagent_env_overlay_carries_comma_joined_names_for_multiple_defs() {
        // Given
        let defs = vec![
            a_def("explorer", &["Grep", "Glob"]),
            a_def("my-linter", &["ReadLints"]),
        ];

        // When
        let overlay = subagent_env_overlay(&defs);

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT").map(String::as_str),
            Some("explorer,my-linter")
        );
        assert!(
            !overlay.contains_key("TDDY_SUBAGENT_REPLACES"),
            "TDDY_SUBAGENT_REPLACES is single-agent-only; got: {overlay:?}"
        );
    }

    /// With a single def, `TDDY_SUBAGENT_REPLACES` carries that def's own YAML-declared `replaces`
    /// set — there is no caller-facing override.
    #[test]
    fn subagent_env_overlay_single_agent_uses_declared_default() {
        // Given
        let defs = vec![a_def("explorer", &["Grep", "Glob"])];

        // When
        let overlay = subagent_env_overlay(&defs);

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT_REPLACES").map(String::as_str),
            Some("Grep,Glob")
        );
    }
}
