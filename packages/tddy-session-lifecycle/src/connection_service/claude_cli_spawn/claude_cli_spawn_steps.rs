use crate::{
    cli_session_manager::CliSessionManager,
    connection_service::{hooks_and_urls, managed_launch, service_util},
};

use tddy_projects::project_storage;
use tddy_spawn::spawner;
use tddy_task::TaskRegistry;

use std::sync::Arc;

use uuid::Uuid;

use tddy_rpc::Status;

use std::path::PathBuf;

use std::path::Path;

use crate::config::DaemonConfig;

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

pub(super) fn install_claude_cli_hooks(
    config: &DaemonConfig,
    os_user: &str,
    session_id: &str,
    worktree_path: &Path,
) -> (String, String) {
    let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
        config
            .claude_cli
            .as_ref()
            .and_then(|c| c.tddy_tools_path.as_deref()),
    );

    let daemon_url = hooks_and_urls::claude_hook_daemon_url(config);

    // Generate a per-session hook token and write .claude/settings.local.json into the
    // worktree. Claude Code reads this file on startup and wires the six lifecycle hooks.
    // Write failure is warn-and-continue so it never blocks the session from starting.
    let hook_token = Uuid::new_v4().to_string();
    hooks_and_urls::write_claude_hooks_settings(
        worktree_path,
        &tddy_core::HookCommandParams {
            tddy_tools_path: &tddy_tools_path,
            daemon_url: &daemon_url,
            session_id,
            os_user,
            hook_token: &hook_token,
        },
    );
    (tddy_tools_path, hook_token)
}

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
    pub(super) task_registry: &'a TaskRegistry,
    pub(super) session_dir: &'a Path,
    pub(super) worktree_path: &'a Path,
    pub(super) tddy_tools_path: String,
}

pub(super) async fn managed_claude_cli_launch(
    launch: ManagedClaudeCliLaunch<'_>,
) -> Result<
    (
        Option<crate::session_toolcall::ManagedWorkflow>,
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
        task_registry,
        session_dir,
        worktree_path,
        tddy_tools_path,
    } = launch;
    let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
    let mut append_system_prompt_file: Option<PathBuf> = None;
    let mut env_extra: Vec<(String, String)> = Vec::new();
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
        )?;
        append_system_prompt_file = Some(launch.prompt_file);
        env_extra = launch.env;
        managed = Some(launch.workflow);
    }
    // Semantic index: build the per-session vector index over the worktree before launching the
    // agent (blocking until terminal). A missing embedder or a failed index aborts the start — no
    // unindexed fallback. On success, point the `SemanticSearch` tool at the session's index DB.
    if semantic_index {
        service_util::index_session_worktree(
            tddy_data_dir,
            task_registry,
            session_id,
            worktree_path,
            session_dir,
        )
        .await?;
        let (key, value) = tddy_semantic_index::semantic_index::semantic_index_env(session_dir);
        env_extra.push((key, value));
    }
    Ok((managed, append_system_prompt_file, env_extra))
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
    pub(super) managed: Option<crate::session_toolcall::ManagedWorkflow>,
    pub(super) append_system_prompt_file: Option<PathBuf>,
    pub(super) env_extra: Vec<(String, String)>,
}

pub(super) async fn spawn_claude_cli_process(
    launch: ClaudeCliProcess<'_>,
) -> Result<Arc<crate::claude_cli_session::PtyHandle>, Status> {
    let ClaudeCliProcess {
        os_user,
        session_id,
        dangerously_skip_permissions,
        manager,
        session_id_owned,
        model_owned,
        binary_owned,
        worktree_clone,
        initial_prompt_opt,
        permission_mode_opt,
        managed,
        append_system_prompt_file,
        env_extra,
    } = launch;
    let handle = manager
        .start_with_options(
            &session_id_owned,
            worktree_clone,
            &model_owned,
            &binary_owned,
            initial_prompt_opt.as_deref(),
            permission_mode_opt.as_deref(),
            dangerously_skip_permissions,
            false,
            append_system_prompt_file.as_deref(),
            Vec::new(),
            env_extra,
            Some(os_user),
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn claude-cli: {}", e)))?;
    if let Some(mw) = managed {
        manager.attach_managed_workflow(session_id, mw).await;
    }
    Ok(handle)
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
