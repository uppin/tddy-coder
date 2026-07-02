//! Spawn `tddy-sandbox-runner` inside Seatbelt without a host `tddy-daemon`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tddy_daemon::sandbox_session::{
    build_sandbox_runner_env, copy_dir_all, pick_free_loopback_port, resolve_sandbox_runner_path,
    resolve_tddy_tools_path, spawn_sandbox_runner, wait_for_sandbox_ready, SandboxRunnerSpawn,
};
use tddy_sandbox::{append_line, SandboxContextDir, SandboxHandle, SubagentReplacement};

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

/// Spawn-time specialized-agent configuration (array model — see
/// docs/ft/coder/specialized-subagents.md). `specialized_agents` empty means no subagent is wired
/// into the session. The deprecated `--discovery-subagent` single-name alias is folded into
/// `specialized_agents` by [`resolve_specialized_agent_names`] before this config is built — never
/// both set at once.
#[derive(Default, Clone)]
pub(crate) struct SubagentSpawnConfig {
    pub specialized_agents: Vec<String>,
    /// Directory to resolve named agents from (`<tddyhome>/agents`), in addition to the builtins.
    pub agents_dir: PathBuf,
    /// Single-agent-only override: only valid when exactly one specialized agent is selected.
    pub fastcontext_url: Option<String>,
    /// Single-agent-only override: only valid when exactly one specialized agent is selected.
    pub fastcontext_model: Option<String>,
    /// Single-agent-only override: only valid when exactly one specialized agent is selected.
    pub fastcontext_max_turns: Option<u32>,
    /// `--subagent-replaces` override (comma-separated tool names), single-agent-only. `None`
    /// means "use the declared default(s)" — resolved via
    /// [`tddy_discovery::subagent::resolve_replaced_tools_for_defs`].
    pub replaces: Option<String>,
}

/// Resolve the effective specialized-agent name list from `--specialized-agent` (repeatable) and
/// the deprecated `--discovery-subagent` single-name alias. Mirrors [`resolve_codebase_mode`]'s
/// contract: the alias is folded in when the new flag is absent, and giving both at once is a
/// contradiction rejected outright rather than silently resolved by precedence.
pub(crate) fn resolve_specialized_agent_names(
    specialized_agent: &[String],
    discovery_subagent: Option<&str>,
) -> Result<Vec<String>, String> {
    let discovery_subagent = discovery_subagent.map(str::trim).filter(|s| !s.is_empty());
    match (specialized_agent.is_empty(), discovery_subagent) {
        (false, Some(_)) => Err(
            "conflicting subagent selection: --specialized-agent was given together with \
             --discovery-subagent (a deprecated alias for a single specialized agent)"
                .to_string(),
        ),
        (false, None) => Ok(specialized_agent.to_vec()),
        (true, Some(name)) => Ok(vec![name.to_string()]),
        (true, None) => Ok(Vec::new()),
    }
}

/// Resolve `config.specialized_agents` against builtins + `config.agents_dir`, baking any
/// single-agent `--fastcontext-*` override onto the matched def. An unresolvable name is an
/// error. Giving a single-agent override alongside more than one selected agent is also an
/// error — there is no well-defined agent to apply it to.
pub(crate) fn resolve_specialized_agents(
    config: &SubagentSpawnConfig,
) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>> {
    if config.specialized_agents.is_empty() {
        return Ok(Vec::new());
    }
    let resolved = tddy_discovery::agent_def::resolve_agent_defs(&config.agents_dir);
    let mut selected = Vec::with_capacity(config.specialized_agents.len());
    for name in &config.specialized_agents {
        let def = resolved.iter().find(|d| &d.name == name).ok_or_else(|| {
            anyhow::anyhow!(
                "specialized agent '{name}' not found (not a builtin and not present under {})",
                config.agents_dir.display()
            )
        })?;
        selected.push(def.clone());
    }

    let has_single_agent_overrides = config.fastcontext_url.is_some()
        || config.fastcontext_model.is_some()
        || config.fastcontext_max_turns.is_some();
    if has_single_agent_overrides {
        anyhow::ensure!(
            selected.len() == 1,
            "--fastcontext-url/--fastcontext-model/--fastcontext-max-turns only apply when \
             exactly one specialized agent is selected; got {}",
            selected.len()
        );
        let def = &mut selected[0];
        if let Some(ref url) = config.fastcontext_url {
            def.base_url = url.clone();
        }
        if let Some(ref model) = config.fastcontext_model {
            def.model = model.clone();
        }
        if let Some(max_turns) = config.fastcontext_max_turns {
            def.max_turns = max_turns;
        }
    }
    Ok(selected)
}

/// Build the (name, replaced-tools) pairs for a session's resolved specialized-agent defs.
/// `replaces_override` (from `--subagent-replaces`) is single-agent-only: it only applies (and is
/// only meaningful) when exactly one def is given, overriding that def's own declared `replaces`
/// outright rather than merging with it.
pub(crate) fn specialized_agent_replacement_pairs(
    defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
    replaces_override: Option<&str>,
) -> Vec<(String, Vec<String>)> {
    if defs.len() == 1 {
        let replaced = match replaces_override.map(str::trim).filter(|s| !s.is_empty()) {
            Some(csv) => {
                let tokens: Vec<String> =
                    csv.split(',').map(str::trim).map(str::to_string).collect();
                tddy_discovery::subagent::normalize_replaced_tools(&tokens)
            }
            None => tddy_discovery::subagent::normalize_replaced_tools(&defs[0].replaces),
        };
        return vec![(defs[0].name.clone(), replaced)];
    }
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
    replaces_override: Option<&str>,
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
        let (_, replaced) = &specialized_agent_replacement_pairs(defs, replaces_override)[0];
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
    let specialized_defs = resolve_specialized_agents(&params.subagent)?;
    let replacement_pairs =
        specialized_agent_replacement_pairs(&specialized_defs, params.subagent.replaces.as_deref());
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
        SandboxContextDir::create_with_subagent(&repo_for_context, &replacements)
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
    env.extend(subagent_env_overlay(
        &specialized_defs,
        params.subagent.replaces.as_deref(),
    ));

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

    // ─── resolve_specialized_agent_names ────────────────────────────────────────
    //
    // Feature: docs/ft/coder/specialized-subagents.md (tddy-sandbox-app migration)

    /// With neither `--specialized-agent` nor `--discovery-subagent` given, no agent is selected.
    #[test]
    fn resolve_specialized_agent_names_returns_empty_when_neither_flag_is_given() {
        // Given / When
        let names = resolve_specialized_agent_names(&[], None)
            .expect("no flags given must resolve without error");

        // Then
        assert_eq!(names, Vec::<String>::new());
    }

    /// Repeated `--specialized-agent` flags produce the array verbatim, in the given order.
    #[test]
    fn resolve_specialized_agent_names_returns_the_array_when_specialized_agent_repeated() {
        // Given
        let given = vec!["fastcontext".to_string(), "my-linter".to_string()];

        // When
        let names = resolve_specialized_agent_names(&given, None)
            .expect("repeated --specialized-agent must resolve without error");

        // Then
        assert_eq!(names, given);
    }

    /// With no `--specialized-agent` given, the deprecated `--discovery-subagent` single name is
    /// folded into a one-element array.
    #[test]
    fn resolve_specialized_agent_names_folds_discovery_subagent_alias_into_single_entry_array() {
        // Given / When
        let names = resolve_specialized_agent_names(&[], Some("fastcontext"))
            .expect("the deprecated alias alone must resolve without error");

        // Then
        assert_eq!(names, vec!["fastcontext".to_string()]);
    }

    /// Giving both `--specialized-agent` and `--discovery-subagent` at once is a contradiction —
    /// rejected outright, not silently resolved by one taking precedence.
    #[test]
    fn resolve_specialized_agent_names_errors_when_both_flags_are_given() {
        // Given / When
        let result =
            resolve_specialized_agent_names(&["fastcontext".to_string()], Some("fastcontext"));

        // Then
        assert!(
            result.is_err(),
            "--specialized-agent + --discovery-subagent together must be rejected"
        );
    }

    // ─── resolve_specialized_agents ─────────────────────────────────────────────

    fn config_with_names(names: &[&str]) -> SubagentSpawnConfig {
        SubagentSpawnConfig {
            specialized_agents: names.iter().map(|s| s.to_string()).collect(),
            agents_dir: PathBuf::from("/nonexistent-agents-dir-for-tests"),
            fastcontext_url: None,
            fastcontext_model: None,
            fastcontext_max_turns: None,
            replaces: None,
        }
    }

    /// An empty `specialized_agents` list resolves to no defs, not an error.
    #[test]
    fn resolve_specialized_agents_returns_empty_for_no_names() {
        // Given
        let config = config_with_names(&[]);

        // When
        let defs = resolve_specialized_agents(&config).expect("empty names must not error");

        // Then
        assert!(defs.is_empty());
    }

    /// The always-available builtin `fastcontext` resolves without any `--agents-dir` override.
    #[test]
    fn resolve_specialized_agents_resolves_the_builtin_fastcontext_name() {
        // Given
        let config = config_with_names(&["fastcontext"]);

        // When
        let defs = resolve_specialized_agents(&config)
            .expect("fastcontext must resolve via the builtin def");

        // Then
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "fastcontext");
    }

    /// A name that resolves against neither the builtins nor `agents_dir` is a typed error.
    #[test]
    fn resolve_specialized_agents_errors_on_unknown_name() {
        // Given
        let config = config_with_names(&["ghost-agent"]);

        // When
        let result = resolve_specialized_agents(&config);

        // Then
        let err = result.expect_err("an unresolvable name must be rejected");
        assert!(
            err.to_string().contains("ghost-agent"),
            "the error must name the unresolvable agent; got: {err}"
        );
    }

    /// A single-agent `--fastcontext-*` override is baked onto the one matched def.
    #[test]
    fn resolve_specialized_agents_bakes_single_agent_overrides_onto_the_matched_def() {
        // Given
        let mut config = config_with_names(&["fastcontext"]);
        config.fastcontext_url = Some("http://localhost:9999".to_string());
        config.fastcontext_model = Some("custom-model".to_string());
        config.fastcontext_max_turns = Some(3);

        // When
        let defs = resolve_specialized_agents(&config).expect("override must resolve cleanly");

        // Then
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].base_url, "http://localhost:9999");
        assert_eq!(defs[0].model, "custom-model");
        assert_eq!(defs[0].max_turns, 3);
    }

    /// A single-agent override alongside more than one selected agent is rejected — there is no
    /// well-defined agent to apply it to.
    #[test]
    fn resolve_specialized_agents_errors_when_overrides_given_with_more_than_one_agent() {
        // Given — "fastcontext" twice just to get 2 resolvable entries without needing a second
        // real agent def on disk
        let mut config = config_with_names(&["fastcontext", "fastcontext"]);
        config.fastcontext_url = Some("http://localhost:9999".to_string());

        // When
        let result = resolve_specialized_agents(&config);

        // Then
        assert!(
            result.is_err(),
            "a single-agent override with 2 selected agents must be rejected"
        );
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
        let overlay = subagent_env_overlay(&[], None);

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
        let defs = vec![a_def("fastcontext", &["Grep", "Glob"])];

        // When
        let overlay = subagent_env_overlay(&defs, None);

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT").map(String::as_str),
            Some("fastcontext")
        );
        let defs_json = overlay
            .get("TDDY_SUBAGENTS_JSON")
            .expect("TDDY_SUBAGENTS_JSON must be present");
        assert!(
            defs_json.contains("fastcontext"),
            "TDDY_SUBAGENTS_JSON must serialize the def; got: {defs_json}"
        );
    }

    /// Multiple resolved defs carry a comma-joined `TDDY_SUBAGENT` name list and no
    /// `TDDY_SUBAGENT_REPLACES` (that key is single-agent-only).
    #[test]
    fn subagent_env_overlay_carries_comma_joined_names_for_multiple_defs() {
        // Given
        let defs = vec![
            a_def("fastcontext", &["Grep", "Glob"]),
            a_def("my-linter", &["ReadLints"]),
        ];

        // When
        let overlay = subagent_env_overlay(&defs, None);

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT").map(String::as_str),
            Some("fastcontext,my-linter")
        );
        assert!(
            !overlay.contains_key("TDDY_SUBAGENT_REPLACES"),
            "TDDY_SUBAGENT_REPLACES is single-agent-only; got: {overlay:?}"
        );
    }

    /// With a single def and no `--subagent-replaces` override, `TDDY_SUBAGENT_REPLACES` carries
    /// that def's own declared `replaces` set.
    #[test]
    fn subagent_env_overlay_single_agent_uses_declared_default_when_no_override_given() {
        // Given
        let defs = vec![a_def("fastcontext", &["Grep", "Glob"])];

        // When
        let overlay = subagent_env_overlay(&defs, None);

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT_REPLACES").map(String::as_str),
            Some("Grep,Glob")
        );
    }

    /// With a single def, an explicit `--subagent-replaces` override wins outright over that
    /// def's declared default, with tokens normalized to canonical exec-tool casing.
    #[test]
    fn subagent_env_overlay_single_agent_override_wins_over_declared_default() {
        // Given
        let defs = vec![a_def("fastcontext", &["Grep", "Glob"])];

        // When
        let overlay = subagent_env_overlay(&defs, Some("read"));

        // Then
        assert_eq!(
            overlay.get("TDDY_SUBAGENT_REPLACES").map(String::as_str),
            Some("Read")
        );
    }
}
