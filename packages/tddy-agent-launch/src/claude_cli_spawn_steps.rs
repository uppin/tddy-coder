use crate::claude_cli_tmux::ClaudeCliTmuxLaunch;
use crate::hooks_and_urls;
use crate::managed_launch;
use crate::project_home_launch::{spec_role_launch, RoleLaunch, RoleWiring};
use crate::spec_launch::{launch_claude_cli, ClaudeCliLaunch, HostCodingAgents};
use tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_session_split::service_util;

use tddy_projects::project_storage;
use tddy_spawn::spawner;
use tddy_task::TaskRegistry;

use std::sync::Arc;

use uuid::Uuid;

use tddy_rpc::Status;

use std::path::PathBuf;

use std::path::Path;

use tddy_daemon_kernel::config::DaemonConfig;

use tddy_daemon_kernel::daemon_hook_urls;
use tddy_service::proto::session::start_phase::Step as StartStep;

/// What cutting a claude-cli session's worktree reads: the checkout, the base, and where it goes.
pub(super) struct ClaudeCliWorktreeCut<'a> {
    pub(super) config: &'a DaemonConfig,
    pub(super) session_id: &'a str,
    pub(super) create_remote_branch: bool,
    pub(super) project: project_storage::ProjectData,
    pub(super) repo_root: &'a Path,
    pub(super) session_dir: &'a Path,
    pub(super) intent: tddy_core::BranchWorktreeIntent,
    pub(super) worktree_base_ref: Option<String>,
    pub(super) ssh_alias: &'a str,
}

pub(super) async fn cut_claude_cli_worktree(
    launch: ClaudeCliWorktreeCut<'_>,
) -> Result<PathBuf, Status> {
    let ClaudeCliWorktreeCut {
        config,
        session_id,
        create_remote_branch,
        project,
        repo_root,
        session_dir,
        intent,
        worktree_base_ref,
        ssh_alias,
    } = launch;
    let timeout = config.spawn_worker_request_timeout();
    let worktree_path = if ssh_alias.is_empty() {
        service_util::create_session_worktree(
            timeout,
            "start_claude_cli_session: create worktree",
            repo_root,
            session_dir,
            worktree_base_ref,
        )
        .await?
    } else {
        let git_url = project.git_url.clone();
        let session_id_owned = session_id.to_string();
        let alias_owned = ssh_alias.to_string();
        let remote_path = service_util::spawn_blocking_with_timeout(
            timeout,
            "start_claude_cli_session: create remote worktree",
            move || {
                tddy_core::setup_worktree_for_session_over_ssh(
                    &alias_owned,
                    &git_url,
                    &session_id_owned,
                )
                .map_err(|e| anyhow::anyhow!("remote worktree setup failed: {}", e))
            },
        )
        .await?;
        PathBuf::from(remote_path)
    };
    service_util::push_new_branch_to_origin_if_requested(
        create_remote_branch,
        intent,
        session_dir,
        &worktree_path,
        timeout,
    )
    .await?;
    Ok(worktree_path)
}

/// The session's checkout: the one the caller named in `repo_path` (a project's home worktree),
/// adopted as it is, or — when that is empty — a session worktree cut for it.
pub(super) async fn claude_cli_checkout(
    repo_path: &str,
    cut: ClaudeCliWorktreeCut<'_>,
) -> Result<PathBuf, Status> {
    match repo_path.trim() {
        "" => cut_claude_cli_worktree(cut).await,
        adopted => adopt_checkout(adopted),
    }
}

/// A checkout the caller named, run in directly: canonicalized, and required to be a directory. It
/// is not daemon-managed, so nothing removes it when the session ends.
fn adopt_checkout(path: &str) -> Result<PathBuf, Status> {
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        Status::invalid_argument(format!("repo_path {path} is not accessible: {e}"))
    })?;
    if !canonical.is_dir() {
        return Err(Status::invalid_argument(format!(
            "repo_path {} is not a directory",
            canonical.display()
        )));
    }
    Ok(canonical)
}

pub(super) fn spawned_claude_cli_branch(
    branch_worktree_intent: &str,
    new_branch_name: &str,
    selected_branch_to_work_on: &str,
    project_id: &str,
    projects_dir: PathBuf,
    repo_root: PathBuf,
    session_dir: &Path,
) -> Result<String, Status> {
    let remote =
        project_storage::effective_remote_name_for_project(&projects_dir, project_id, &repo_root)
            .map_err(|e| Status::internal(e.to_string()))?;
    // Resolved once: the same branch is what the node records and what this session publishes about
    // itself on its participant. Read back from the changeset the worktree setup just wrote rather
    // than taken from the request — the branch may carry a collision suffix, and a node recording a
    // name nobody created leaves every descendant basing onto a ref that does not exist.
    let spawned_branch = hooks_and_urls::spawned_branch_of_session(
        session_dir,
        hooks_and_urls::effective_spawn_branch(
            branch_worktree_intent,
            new_branch_name,
            selected_branch_to_work_on,
            &remote,
        ),
    );
    Ok(spawned_branch)
}

/// The lifecycle hooks a claude-cli start wires its agent to: the relay that reports them, where it
/// reports, and the per-session token the daemon checks.
pub(super) struct ClaudeCliHooks {
    pub(super) tddy_tools_path: String,
    daemon_url: String,
    daemon_socket: Option<String>,
    pub(super) hook_token: String,
    answer_wait_secs: u64,
}

impl ClaudeCliHooks {
    /// The hooks' command parameters for `session_id`, run by `os_user`.
    pub(super) fn params<'a>(
        &'a self,
        session_id: &'a str,
        os_user: &'a str,
    ) -> tddy_core::HookCommandParams<'a> {
        tddy_core::HookCommandParams {
            tddy_tools_path: &self.tddy_tools_path,
            daemon_url: &self.daemon_url,
            session_id,
            os_user,
            hook_token: &self.hook_token,
            daemon_socket: self.daemon_socket.as_deref(),
            answer_wait_secs: self.answer_wait_secs,
        }
    }
}

/// A claude-cli start's hooks, with a fresh per-session token.
///
/// `tddy_data_dir` is the data dir the daemon *resolved* — `TDDY_DATA_DIR` included, which a
/// configuration file need not agree with (`./desktop-dev` sets it absolute beside a relative
/// `tmp/.tddy`) — because an embedded daemon's agent-tool socket path is digested from it.
pub(super) fn claude_cli_hooks(config: &DaemonConfig, tddy_data_dir: &Path) -> ClaudeCliHooks {
    let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
        config
            .claude_cli
            .as_ref()
            .and_then(|c| c.tddy_tools_path.as_deref()),
    );
    ClaudeCliHooks {
        tddy_tools_path,
        daemon_url: daemon_hook_urls::claude_hook_daemon_url(config),
        // An embedded daemon serves no HTTP RPC; its hooks report over this socket.
        daemon_socket: tddy_daemon_kernel::embedded_host_agent_tool_socket(tddy_data_dir),
        hook_token: Uuid::new_v4().to_string(),
        answer_wait_secs: config.claude_cli_answer_wait_secs(),
    }
}

/// A claude-cli start's hooks ([`claude_cli_hooks`]), their `.claude/settings.local.json` written
/// into the worktree — the hand-built launch's writer; a spec-driven launch renders the file through
/// the `claude` spec's `hooks` slot instead. Claude Code reads this file on startup and wires the
/// six lifecycle hooks. Write failure is warn-and-continue so it never blocks the session from
/// starting.
pub(super) fn install_claude_cli_hooks(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    os_user: &str,
    session_id: &str,
    worktree_path: &Path,
) -> ClaudeCliHooks {
    let hooks = claude_cli_hooks(config, tddy_data_dir);
    tddy_session_split::service_util::write_claude_hooks_settings(
        worktree_path,
        &hooks.params(session_id, os_user),
    );
    hooks
}

/// The notify wiring a claude-cli start and resume apply — see [`crate::notify_env`].
pub use crate::notify_env::{
    agent_notify_launch, resumed_agent_notify_launch, AgentNotifyLaunch, AgentNotifyWiring,
};

/// What wiring a claude-cli session's managed workflow and semantic index reads.
pub(super) struct ManagedClaudeCliLaunch<'a> {
    pub(super) tddy_data_dir: &'a Path,
    pub(super) session_id: &'a str,
    pub(super) managed_recipe:
        &'a Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    pub(super) child_spawn_handler:
        Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler + 'static>>,
    pub(super) conversation_spawn_handler:
        Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler + 'static>>,
    pub(super) semantic_index: bool,
    pub(super) progress: &'a tddy_session_files::attachment_progress::AttachmentProgressSink,
    pub(super) task_registry: &'a TaskRegistry,
    pub(super) session_dir: &'a Path,
    pub(super) worktree_path: &'a Path,
    pub(super) tddy_tools_path: String,
    /// The `GIT_*` pairs of the account the project resolved to; empty when it resolved none.
    pub(super) git_environment: Vec<(String, String)>,
    /// Answers the agent's tools' `github-token` over the session's toolcall socket.
    pub(super) github_credential_handler:
        Option<Arc<dyn tddy_core::toolcall::GithubCredentialHandler + 'static>>,
}

pub(super) async fn managed_claude_cli_launch(
    launch: ManagedClaudeCliLaunch<'_>,
) -> Result<
    (
        Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
        Option<PathBuf>,
        Vec<(String, String)>,
    ),
    Status,
> {
    let ManagedClaudeCliLaunch {
        tddy_data_dir,
        session_id,
        managed_recipe,
        child_spawn_handler,
        conversation_spawn_handler,
        semantic_index,
        progress,
        task_registry,
        session_dir,
        worktree_path,
        tddy_tools_path,
        git_environment,
        github_credential_handler,
    } = launch;
    let mut managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow> = None;
    let mut append_system_prompt_file: Option<PathBuf> = None;
    let mut env_extra = git_environment;
    if let Some(recipe) = managed_recipe.clone() {
        let launch = managed_launch::prepare_managed_workflow_inner(
            tddy_data_dir,
            session_id,
            recipe,
            session_dir,
            worktree_path,
            session_dir,
            &tddy_tools_path,
            None,
            child_spawn_handler.clone(),
            conversation_spawn_handler.clone(),
            github_credential_handler,
        )?;
        append_system_prompt_file = Some(launch.prompt_file);
        env_extra.extend(launch.env);
        managed = Some(launch.workflow);
    }
    // Semantic index: build the per-session vector index over the worktree before launching the
    // agent (blocking until terminal). A missing embedder or a failed index aborts the start — no
    // unindexed fallback. On success, point the `SemanticSearch` tool at the session's index DB.
    if semantic_index {
        progress.begin_phase(StartStep::SemanticIndex);
        service_util::index_session_worktree(
            tddy_data_dir,
            task_registry,
            session_id,
            worktree_path,
            session_dir,
        )
        .await?;
        progress.end_phase(StartStep::SemanticIndex);
        let (key, value) = tddy_semantic_index::semantic_index::semantic_index_env(session_dir);
        env_extra.push((key, value));
    }
    Ok((managed, append_system_prompt_file, env_extra))
}

/// What the session's role pre-approves its `claude` to call, as `--allowedTools` arguments. A
/// project-home session is given exactly [`tddy_tool_engine::read_only::project_home_allowed_tools`];
/// every other role, and no role at all, adds nothing, so an ordinary session's argv is unchanged.
pub(crate) fn role_allowed_tools_args(session_role: Option<&str>) -> Vec<String> {
    if session_role != Some(tddy_core::session_metadata::PROJECT_HOME_ROLE) {
        return Vec::new();
    }
    tddy_tool_engine::read_only::project_home_allowed_tools()
        .into_iter()
        .flat_map(|tool| ["--allowedTools".to_string(), tool])
        .collect()
}

/// Record a session's standing role in its `.session.yaml` before its agent is spawned, so there is
/// no moment in which the agent runs and the daemon reads the session as ordinary. The record is
/// complete as far as a spawn-less session can be, and the spawn's own write replaces it with the pid.
/// A session without a role has nothing to record early.
pub(super) fn record_role_before_spawn(
    session_dir: &Path,
    session_id: &str,
    project_id: &str,
    session_role: &str,
    worktree_path: &Path,
    managed_codebase: bool,
) -> Result<(), Status> {
    record_role_of_session_type_before_spawn(
        session_dir,
        session_id,
        project_id,
        session_role,
        "claude-cli",
        worktree_path,
        managed_codebase,
    )
}

/// [`record_role_before_spawn`] for a session of any CLI agent: `session_type` is the record's.
pub(super) fn record_role_of_session_type_before_spawn(
    session_dir: &Path,
    session_id: &str,
    project_id: &str,
    session_role: &str,
    session_type: &str,
    worktree_path: &Path,
    managed_codebase: bool,
) -> Result<(), Status> {
    if session_role.is_empty() {
        return Ok(());
    }
    let meta = tddy_core::SessionMetadata {
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        managed_codebase,
        session_role: Some(session_role.to_string()),
        ..service_util::starting_session_metadata(session_id, project_id, session_type)
    };
    tddy_core::write_session_metadata(session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to record the session's role: {e}")))
}

/// What the claude-cli process is spawned with, and the managed workflow it is handed.
pub(super) struct ClaudeCliProcess<'a> {
    pub(super) os_user: &'a str,
    pub(super) session_id: &'a str,
    pub(super) dangerously_skip_permissions: bool,
    pub(super) manager: Arc<CliSessionManager>,
    pub(super) session_id_owned: String,
    pub(super) model_owned: String,
    pub(super) binary_owned: String,
    pub(super) worktree_clone: PathBuf,
    pub(super) initial_prompt_opt: Option<String>,
    pub(super) permission_mode_opt: Option<String>,
    pub(super) managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    pub(super) append_system_prompt_file: Option<PathBuf>,
    pub(super) env_extra: Vec<(String, String)>,
    /// The coding agents declared as tools, when the host has them: the process is then launched
    /// from the `claude` spec instead of the hand-built argv. `config` names daemon.yaml's
    /// `claude_cli.binary_path`.
    pub(super) coding_agents: Option<(&'a HostCodingAgents, &'a DaemonConfig)>,
    /// The session's hooks: a spec-driven launch writes their settings through the `hooks` slot.
    pub(super) hooks: &'a ClaudeCliHooks,
    /// A main's launch, carried out only now that nothing before the spawn can refuse the start.
    pub(super) main: Option<crate::project_home_launch::MainLaunchPlan<'a>>,
    /// Run `claude` inside a tmux session instead of as the PTY child ([`crate::claude_cli_tmux`]).
    pub(super) run_in_tmux: bool,
    /// The session's directory: a tmux-run agent's pane environment file is written under it.
    pub(super) session_dir: PathBuf,
    /// The SSH target the worktree lives on; empty for a local checkout.
    pub(super) ssh_alias: String,
}

/// A started claude-cli agent: the pid the session records, and the tmux session it runs in when
/// it was started with `run_in_tmux`.
pub(super) struct SpawnedClaudeCli {
    /// The agent's pid: the PTY child's, or — for a tmux-run agent — the pane's process.
    pub(super) pid: u32,
    pub(super) tmux_session: Option<String>,
}

pub(super) async fn spawn_claude_cli_process(
    mut launch: ClaudeCliProcess<'_>,
) -> Result<SpawnedClaudeCli, Status> {
    let managed = launch.managed.take();
    let manager = Arc::clone(&launch.manager);
    let session_id = launch.session_id;
    let role = RoleWiring::of(launch.main.take())?;
    let spawned = match launch.coding_agents {
        // TODO(agent-tools-cascade): a tmux-run start (`run_in_tmux`, #tmux-agent) still builds the
        // hand-built argv in `claude_cli_tmux::start_claude_in_tmux` (and installs its hooks by
        // hand); confirm the spec-rendered argv (`spec_launch::launch_claude_cli`) should be the one
        // started in the agent's tmux pane.
        Some((agents, config)) if !launch.run_in_tmux => {
            let handle = launch
                .spawn_from_spec(agents, config, role)
                .await
                .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {}", e)))?;
            SpawnedClaudeCli {
                pid: handle.pid,
                tmux_session: None,
            }
        }
        _ => {
            let role_launch = role.hand_built()?;
            launch.spawn_hand_built(role_launch).await?
        }
    };
    if let Some(mw) = managed {
        manager.attach_managed_workflow(session_id, mw).await;
    }
    Ok(spawned)
}

/// Start claude as the session's main PTY child, as every claude-cli start did before tmux mode,
/// and return its pid.
async fn start_claude_in_pty(
    manager: &CliSessionManager,
    launch: ClaudeCliTmuxLaunch<'_>,
) -> Result<u32, Status> {
    let handle = manager
        .start_with_options(
            launch.session_id,
            launch.worktree_path,
            launch.model,
            launch.binary,
            launch.initial_prompt,
            launch.permission_mode,
            launch.dangerously_skip_permissions,
            launch.resume,
            launch.append_system_prompt_file,
            launch.extra_args,
            launch.env,
            Some(launch.os_user),
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {}", e)))?;
    Ok(handle.pid)
}

impl ClaudeCliProcess<'_> {
    /// Launched from the `claude` spec: the main's tool flags and the hooks file go through the
    /// spec's slots.
    async fn spawn_from_spec(
        self,
        agents: &HostCodingAgents,
        config: &DaemonConfig,
        role: RoleWiring,
    ) -> anyhow::Result<Arc<PtyHandle>> {
        let (extra_args, toolset) = spec_role_launch(role.main.as_ref());
        launch_claude_cli(ClaudeCliLaunch {
            remote_host: Some(self.ssh_alias.as_str()).filter(|alias| !alias.is_empty()),
            agents,
            config,
            manager: &self.manager,
            session_id: &self.session_id_owned,
            worktree: &self.worktree_clone,
            model: &self.model_owned,
            resume: false,
            initial_prompt: self.initial_prompt_opt.as_deref(),
            permission_mode: self.permission_mode_opt.as_deref(),
            dangerously_skip_permissions: self.dangerously_skip_permissions,
            append_system_prompt_file: self.append_system_prompt_file.as_deref(),
            toolset,
            hooks: Some(self.hooks.params(self.session_id, self.os_user)),
            extra_args,
            env: self.env_extra.into_iter().chain(role.env).collect(),
            os_user: Some(self.os_user),
        })
        .await
        .map_err(anyhow::Error::from)
    }

    /// The hand-built argv, for a host without coding agents or a tmux-run start; the hooks file
    /// is already written. A tmux-run agent runs in its own tmux session, the rest as the PTY child.
    async fn spawn_hand_built(self, role_launch: RoleLaunch) -> Result<SpawnedClaudeCli, Status> {
        let env = self.env_extra.into_iter().chain(role_launch.env).collect();
        let launch = ClaudeCliTmuxLaunch {
            os_user: self.os_user,
            session_id: &self.session_id_owned,
            worktree_path: self.worktree_clone,
            model: &self.model_owned,
            binary: &self.binary_owned,
            initial_prompt: self.initial_prompt_opt.as_deref(),
            permission_mode: self.permission_mode_opt.as_deref(),
            dangerously_skip_permissions: self.dangerously_skip_permissions,
            resume: false,
            append_system_prompt_file: self.append_system_prompt_file.as_deref(),
            extra_args: role_launch.extra_args,
            env,
            session_dir: self.session_dir,
        };
        if self.run_in_tmux {
            let launched =
                crate::claude_cli_tmux::start_claude_in_tmux(&self.manager, launch).await?;
            Ok(SpawnedClaudeCli {
                pid: launched.pane_pid,
                tmux_session: Some(launched.tmux_session),
            })
        } else {
            Ok(SpawnedClaudeCli {
                pid: start_claude_in_pty(&self.manager, launch).await?,
                tmux_session: None,
            })
        }
    }
}

pub(super) fn claude_cli_livekit_room(
    config: &DaemonConfig,
    session_id: &str,
) -> (String, String, String) {
    let (lk_room, lk_url, lk_server_identity) = match spawner::livekit_creds_from_config(config) {
        Some(lk) => (
            spawner::resolve_livekit_room_name(lk.common_room.as_deref(), session_id),
            lk.url.clone(),
            spawner::livekit_server_identity_for_session(
                lk.daemon_instance_id.as_deref(),
                session_id,
            ),
        ),
        None => (String::new(), String::new(), String::new()),
    };
    (lk_room, lk_url, lk_server_identity)
}

#[cfg(test)]
mod claude_cli_spawn_steps_tests;
