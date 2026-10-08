//! Cursor Agent CLI session spawn/resume helpers for the launch topic (`LaunchSessions`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::StartSessionResponse;

use crate::cli_session_manager::CliSessionManager;
use crate::connection_service::session_acting_identity::SessionAccountAccess;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use crate::connection_service::hooks_and_urls::effective_spawn_branch;
use crate::connection_service::worktree_source::session_worktree_source;
use crate::connection_service::hooks_and_urls::spawned_branch_of_session;
use crate::connection_service::worktree_source::WorktreeSource;
use tddy_daemon_kernel::config::{resolve_cursor_binary_path, DaemonConfig};
use tddy_projects::project_storage;
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_worktree_service::branch_intent::{BranchIntentPolicy, BranchIntentRequest};

mod chat;
pub use chat::*;

/// [`spawn_cursor_cli_session_reporting`] for a caller with nobody watching the start's steps.
#[allow(clippy::too_many_arguments)]
pub async fn spawn_cursor_cli_session_inner(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    cli_manager: &Arc<CliSessionManager>,
    os_user: &str,
    session_id: &str,
    // Authorizes the `workspace` starts this session's seeded agents need on their own hosts; the
    // peer sees the same token the client presented here.
    session_token: &str,
    sessions_base: PathBuf,
    model: &str,
    project_id: &str,
    branch_worktree_intent: &str,
    new_branch_name: &str,
    selected_integration_base_ref: &str,
    selected_branch_to_work_on: &str,
    // Client-supplied local checkout to run against directly (StartSessionRequest.repo_path).
    // When non-empty it wins over `project_id`: the session's worktree IS this path (no git
    // worktree is created and it is never removed on session end). Empty → resolve from
    // `project_id` as before.
    repo_path: &str,
    // The spawn's PR-stack parent, and the daemon that resolves what the child bases off — this one
    // when it owns the parent, the daemon named in the request when it does not.
    stack_parent: crate::connection_service::stack_parent::SpawnStackParent<'_>,
    initial_prompt: &str,
    managed_codebase: bool,
    // The session's starting agent roster, already resolved against the request's
    // `specialized_agents` by the caller (docs/ft/daemon/session-agent-roster.md). Resolved there
    // rather than here because qualifying an id needs this daemon's def sources *and* its registry
    // assistants, and this free function is handed neither.
    agents: &mut [tddy_core::SessionAgentRecord],
    managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
    // When true, index the worktree before launch (blocking; aborts on failure) and point the
    // `SemanticSearch` tool at the per-session index via `TDDY_SEMANTIC_INDEX_DB`.
    semantic_index: bool,
    // When true (new_branch_from_base only), push the new branch to origin at session start.
    create_remote_branch: bool,
    task_registry: &tddy_task::TaskRegistry,
    // This daemon in its capacity as the claimant of the clones the roster's peer-owned agents
    // read. A parameter rather than something built here because this is a free function, and
    // naming the concrete daemon type would drag it through every caller of a function that
    // otherwise mentions nothing of the kind.
    agent_clones: &dyn tddy_session_agents::seed_codebase::SeededAgentClones,
) -> Result<Response<StartSessionResponse>, Status> {
    spawn_cursor_cli_session_reporting(
        config,
        tddy_data_dir,
        cli_manager,
        os_user,
        session_id,
        session_token,
        sessions_base,
        model,
        project_id,
        branch_worktree_intent,
        new_branch_name,
        selected_integration_base_ref,
        selected_branch_to_work_on,
        repo_path,
        stack_parent,
        initial_prompt,
        managed_codebase,
        agents,
        managed_recipe,
        semantic_index,
        create_remote_branch,
        task_registry,
        agent_clones,
        // A caller of this entry point holds no vault to read an account with, so the session
        // starts under the checkout's own identity.
        &SessionAccountAccess::none(),
        &AttachmentProgressSink::discarding(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn_cursor_cli_session_reporting(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    cli_manager: &Arc<CliSessionManager>,
    os_user: &str,
    session_id: &str,
    // Authorizes the `workspace` starts this session's seeded agents need on their own hosts; the
    // peer sees the same token the client presented here.
    session_token: &str,
    sessions_base: PathBuf,
    model: &str,
    project_id: &str,
    branch_worktree_intent: &str,
    new_branch_name: &str,
    selected_integration_base_ref: &str,
    selected_branch_to_work_on: &str,
    // Client-supplied local checkout to run against directly (StartSessionRequest.repo_path).
    // When non-empty it wins over `project_id`: the session's worktree IS this path (no git
    // worktree is created and it is never removed on session end). Empty → resolve from
    // `project_id` as before.
    repo_path: &str,
    // The spawn's PR-stack parent, and the daemon that resolves what the child bases off — this one
    // when it owns the parent, the daemon named in the request when it does not.
    stack_parent: crate::connection_service::stack_parent::SpawnStackParent<'_>,
    initial_prompt: &str,
    managed_codebase: bool,
    // The session's starting agent roster, already resolved against the request's
    // `specialized_agents` by the caller (docs/ft/daemon/session-agent-roster.md). Resolved there
    // rather than here because qualifying an id needs this daemon's def sources *and* its registry
    // assistants, and this free function is handed neither.
    agents: &mut [tddy_core::SessionAgentRecord],
    managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
    // When true, index the worktree before launch (blocking; aborts on failure) and point the
    // `SemanticSearch` tool at the per-session index via `TDDY_SEMANTIC_INDEX_DB`.
    semantic_index: bool,
    // When true (new_branch_from_base only), push the new branch to origin at session start.
    create_remote_branch: bool,
    task_registry: &tddy_task::TaskRegistry,
    // This daemon in its capacity as the claimant of the clones the roster's peer-owned agents
    // read. A parameter rather than something built here because this is a free function, and
    // naming the concrete daemon type would drag it through every caller of a function that
    // otherwise mentions nothing of the kind.
    agent_clones: &dyn tddy_session_agents::seed_codebase::SeededAgentClones,
    // What the start reads its owner's vault with, for the account identity the agent's commits
    // carry. A cursor-cli session runs no toolcall listener, so there is no token to answer.
    account_access: &SessionAccountAccess,
    // Where the start's phases (worktree, semantic index, agent) are announced.
    progress: &AttachmentProgressSink,
) -> Result<Response<StartSessionResponse>, Status> {
    if model.trim().is_empty() {
        return Err(Status::invalid_argument(
            "model is required for cursor-cli sessions",
        ));
    }
    let project_id = project_id.trim();
    let repo_path = repo_path.trim();

    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

    // No project default branch is consulted here: this path may run against a client-supplied
    // `repo_path` with no registered project at all, and it never read one before the extraction.
    let intent = tddy_session_split::service_util::write_initial_changeset(
        session_id,
        &BranchIntentRequest {
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
        },
        BranchIntentPolicy::cursor_cli(),
        None,
        &session_dir,
        stack_parent.session_id(),
        managed_recipe.as_deref(),
    )?;

    let timeout = config.spawn_worker_request_timeout();
    let (worktree_path, project_accounts) = match session_worktree_source(repo_path, project_id) {
        WorktreeSource::Project(pid) => {
            if pid.is_empty() {
                return Err(Status::invalid_argument(
                    "project_id is required for cursor-cli sessions",
                ));
            }
            let (projects_dir, project) =
                tddy_session_split::service_util::find_registered_project(tddy_data_dir, os_user, &pid)?;
            let repo_root = tddy_session_split::service_util::project_repo_root(&project)?;
            let project_accounts = project.accounts.clone();
            let chain_base_ref = stack_parent
                .chain_base_ref(
                    &pid,
                    &sessions_base,
                    &repo_root,
                    new_branch_name,
                    selected_integration_base_ref,
                )
                .await?;
            progress.begin_phase(StartStep::Worktree);
            let wt = cut_cursor_cli_worktree(
                selected_integration_base_ref,
                create_remote_branch,
                &session_dir,
                intent,
                timeout,
                &repo_root,
                chain_base_ref,
            )
            .await?;
            progress.end_phase(StartStep::Worktree);
            // The child's branch now exists, so the planned node it materializes can record it —
            // which is what makes that node's descendants spawnable at all, since they base onto
            // `<remote>/<branch>` (`Stack::base_ref_for_spawn`). Until this call a cursor-cli child
            // of a pr-stack orchestrator recorded neither a session nor a branch on its node **on
            // any host**, so the stack wedged behind it: the read half of the same association was
            // threaded through here and used, while the write half was never made.
            //
            // Keyed on the branch the session's changeset records rather than on the requested name
            // — it may carry a collision suffix — and a failed link is logged, not raised (D36).
            let spawned_branch = spawned_cursor_cli_branch(
                branch_worktree_intent,
                new_branch_name,
                selected_branch_to_work_on,
                &session_dir,
                pid,
                projects_dir,
                repo_root,
            )?;
            stack_parent
                .link_spawned_branch_without_failing_the_spawn(
                    &sessions_base,
                    &spawned_branch,
                    session_id,
                )
                .await;
            (wt, Some(project_accounts))
        }
        WorktreeSource::RepoPath(path) => {
            let canonical = std::fs::canonicalize(&path).map_err(|e| {
                Status::invalid_argument(format!(
                    "repo_path {} is not accessible: {e}",
                    path.display()
                ))
            })?;
            if !canonical.is_dir() {
                return Err(Status::invalid_argument(format!(
                    "repo_path {} is not a directory",
                    canonical.display()
                )));
            }
            log::info!(
                target: "tddy_daemon::cursor_cli_spawn",
                "spawn_cursor_cli_session_inner {session_id}: using client-supplied repo_path {} directly as worktree (not daemon-managed; not removed on session end)",
                canonical.display()
            );
            // A client-supplied checkout belongs to no project, so no account is assigned to it.
            (canonical, None)
        }
    };

    // The clones this session's seeded agents read, claimed now: the worktree they mirror exists,
    // and the peers that build them must be at work before the agent that prompts them is launched.
    // Where an agent runs decides how the session is split across hosts, never whether it can be
    // seeded — the same placements a split start takes, this one takes.
    let seeded_clones = agent_clones
        .claim_for_seed(
            session_id,
            &tddy_session_agents::seed_codebase::SeedCodebase::of_a_starting_session(
                session_dir.clone(),
                worktree_path.clone(),
                project_id,
                // Nothing stands between this agent and its own file tools: an unsandboxed
                // co-located session enforces no withdrawal (`session_enforces_a_withdrawal`).
                false,
            ),
            session_token,
            agents,
        )
        .await?;

    let hook_token =
        chat::install_cursor_hooks_in_worktree(config, &worktree_path, session_id, os_user);

    let (binary_path, initial_prompt_opt) = prepare_cursor_cli_launch(
        config,
        initial_prompt,
        managed_codebase,
        &managed_recipe,
        &worktree_path,
    );

    // Semantic index: index the worktree into the session dir before launch (blocking; a missing
    // embedder or a failed index aborts the start — no unindexed fallback), and point the
    // `SemanticSearch` tool at the per-session index DB via the process env.
    let mut session_env = cursor_cli_semantic_env(
        tddy_data_dir,
        session_id,
        semantic_index,
        task_registry,
        &session_dir,
        &worktree_path,
        progress,
    )
    .await?;
    // The agent's commits carry the project's account identity; a refusal starts the session
    // under the checkout's own, with the reason logged.
    session_env.extend(
        account_access
            .session_identity(session_id, project_accounts.as_deref())
            .git_environment,
    );

    // The Cursor chat this session owns for its whole lifetime: minted here, persisted in
    // `.session.yaml` below, and passed as `--resume <id>` on every later spawn so a resume
    // continues this chat instead of opening a new one.
    progress.begin_phase(StartStep::Agent);
    let (cursor_chat_id, handle) = spawn_cursor_cli_process(
        cli_manager,
        session_id,
        model,
        &worktree_path,
        binary_path,
        initial_prompt_opt,
        session_env,
    )
    .await?;
    progress.end_phase(StartStep::Agent);

    let pid = handle.pid;
    write_cursor_cli_session_metadata(CursorCliSessionRecord {
        session_id,
        model,
        agents,
        managed_recipe,
        project_id,
        session_dir,
        worktree_path: &worktree_path,
        hook_token,
        cursor_chat_id,
        pid,
    })?;
    // The roster naming them is on disk now, so the clones belong to the session rather than to the
    // start that claimed them.
    seeded_clones.keep();

    log::info!(
        target: "tddy_daemon::connection_service",
        "started cursor-cli session {} pid={} worktree={} user={}",
        session_id,
        pid,
        worktree_path.display(),
        os_user
    );

    Ok(Response::new(StartSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
        branch_conflict: None,
    }))
}

async fn cut_cursor_cli_worktree(
    selected_integration_base_ref: &str,
    create_remote_branch: bool,
    session_dir: &Path,
    intent: tddy_core::BranchWorktreeIntent,
    timeout: std::time::Duration,
    repo_root: &Path,
    chain_base_ref: Option<String>,
) -> Result<PathBuf, Status> {
    let worktree_base_ref =
        tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);
    let wt = tddy_session_split::service_util::create_session_worktree(
        timeout,
        "start_cursor_cli_session: create worktree",
        repo_root,
        session_dir,
        worktree_base_ref,
    )
    .await?;
    tddy_session_split::service_util::push_new_branch_to_origin_if_requested(
        create_remote_branch,
        intent,
        session_dir,
        &wt,
        timeout,
    )
    .await?;
    Ok(wt)
}

fn spawned_cursor_cli_branch(
    branch_worktree_intent: &str,
    new_branch_name: &str,
    selected_branch_to_work_on: &str,
    session_dir: &Path,
    pid: String,
    projects_dir: PathBuf,
    repo_root: PathBuf,
) -> Result<String, Status> {
    let remote =
        project_storage::effective_remote_name_for_project(&projects_dir, &pid, &repo_root)
            .map_err(|e| Status::internal(e.to_string()))?;
    let spawned_branch = spawned_branch_of_session(
        session_dir,
        effective_spawn_branch(
            branch_worktree_intent,
            new_branch_name,
            selected_branch_to_work_on,
            &remote,
        ),
    );
    Ok(spawned_branch)
}

fn prepare_cursor_cli_launch(
    config: &DaemonConfig,
    initial_prompt: &str,
    managed_codebase: bool,
    managed_recipe: &Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    worktree_path: &Path,
) -> (String, Option<String>) {
    let binary_path = resolve_cursor_binary_path(config);
    let initial_prompt_opt = tddy_daemon_kernel::trim_to_option(initial_prompt);
    if managed_recipe.is_some() {
        let rules_dir = worktree_path.join(".cursor").join("rules");
        let _ = std::fs::create_dir_all(&rules_dir);
        if let Some(recipe) = managed_recipe {
            let _ = std::fs::write(
                rules_dir.join("tddy-managed-workflow.mdc"),
                format!("Managed workflow recipe: {}\n", recipe.name()),
            );
        }
    }
    let _ = managed_codebase;
    (binary_path, initial_prompt_opt)
}

async fn cursor_cli_semantic_env(
    tddy_data_dir: &Path,
    session_id: &str,
    semantic_index: bool,
    task_registry: &tddy_task::TaskRegistry,
    session_dir: &Path,
    worktree_path: &Path,
    progress: &AttachmentProgressSink,
) -> Result<Vec<(String, String)>, Status> {
    let mut session_env: Vec<(String, String)> = Vec::new();
    if semantic_index {
        progress.begin_phase(StartStep::SemanticIndex);
        tddy_session_split::service_util::index_session_worktree(
            tddy_data_dir,
            task_registry,
            session_id,
            worktree_path,
            session_dir,
        )
        .await?;
        progress.end_phase(StartStep::SemanticIndex);
        session_env.push(tddy_semantic_index::semantic_index::semantic_index_env(
            session_dir,
        ));
    }
    Ok(session_env)
}

async fn spawn_cursor_cli_process(
    cli_manager: &Arc<CliSessionManager>,
    session_id: &str,
    model: &str,
    worktree_path: &Path,
    binary_path: String,
    initial_prompt_opt: Option<String>,
    session_env: Vec<(String, String)>,
) -> Result<(String, Arc<crate::claude_cli_session::PtyHandle>), Status> {
    let cursor_chat_id = chat::mint_cursor_chat_id(&binary_path, worktree_path)
        .await
        .map_err(|e| {
            Status::internal(format!(
                "failed to create the Cursor chat for session {session_id}: {e}"
            ))
        })?;
    let handle = cli_manager
        .start_cursor(
            session_id,
            worktree_path.to_path_buf(),
            model,
            &binary_path,
            Some(&cursor_chat_id),
            initial_prompt_opt.as_deref(),
            session_env,
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn cursor-cli: {}", e)))?;
    Ok((cursor_chat_id, handle))
}

/// What a started cursor-cli session's `.session.yaml` records.
struct CursorCliSessionRecord<'a> {
    session_id: &'a str,
    model: &'a str,
    agents: &'a mut [tddy_core::SessionAgentRecord],
    managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
    project_id: &'a str,
    session_dir: PathBuf,
    worktree_path: &'a Path,
    hook_token: String,
    cursor_chat_id: String,
    pid: u32,
}

fn write_cursor_cli_session_metadata(launch: CursorCliSessionRecord<'_>) -> Result<(), Status> {
    let CursorCliSessionRecord {
        session_id,
        model,
        agents,
        managed_recipe,
        project_id,
        session_dir,
        worktree_path,
        hook_token,
        cursor_chat_id,
        pid,
    } = launch;
    let meta = SessionMetadata {
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: Some(pid),
        model: Some(model.to_string()),
        cursor_chat_id: Some(cursor_chat_id),
        hook_token: Some(hook_token),
        recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
        agents_rev: tddy_session_agents::agent_roster::started_roster_rev(agents),
        agents: agents.to_vec(),
        ..tddy_session_split::service_util::starting_session_metadata(session_id, project_id, "cursor-cli")
    };
    write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {}", e)))?;
    Ok(())
}

mod resume;
pub use resume::*;
