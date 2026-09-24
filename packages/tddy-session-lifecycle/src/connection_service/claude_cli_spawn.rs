use super::managed_launch;

use tddy_projects::project_storage;
use tddy_spawn::spawner;
use uuid::Uuid;

use tddy_core::Changeset;

use crate::{
    branch_intent::BranchIntentPolicy,
    connection_service::{hooks_and_urls, service_util, stack_parent},
};

use crate::branch_intent::BranchIntentRequest;

use crate::branch_intent::resolve_branch_workflow;

use crate::branch_intent::ResolvedBranchWorkflow;

use tddy_core::output::SESSIONS_SUBDIR;

use crate::user_sessions_path::projects_path_for_user;

use tddy_rpc::Status;

use tddy_service::proto::session::StartSessionResponse;

use tddy_rpc::Response;

use tddy_task::TaskRegistry;

use std::path::PathBuf;

use crate::cli_session_manager::CliSessionManager;

use std::sync::Arc;

use std::path::Path;

use crate::config::DaemonConfig;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn spawn_claude_cli_session_inner(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    claude_cli_manager: &Arc<CliSessionManager>,
    os_user: &str,
    session_id: &str,
    sessions_base: PathBuf,
    model: &str,
    project_id: &str,
    branch_worktree_intent: &str,
    new_branch_name: &str,
    selected_integration_base_ref: &str,
    selected_branch_to_work_on: &str,
    initial_prompt: &str,
    permission_mode: &str,
    dangerously_skip_permissions: bool,
    stack_parent: stack_parent::SpawnStackParent<'_>,
    managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
    child_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ChildSpawnHandler>>,
    conversation_spawn_handler: Option<Arc<dyn tddy_core::toolcall::ConversationSpawnHandler>>,
    // When true, index the worktree into the session dir before launch (blocking; aborts the start
    // on failure) and point the `SemanticSearch` tool at that per-session index DB.
    semantic_index: bool,
    // When true (and the intent is new_branch_from_base), push the freshly created branch to origin
    // at session start; a push failure fails the start.
    create_remote_branch: bool,
    ssh_config_host: &str,
    task_registry: &TaskRegistry,
) -> Result<Response<StartSessionResponse>, Status> {
    if model.trim().is_empty() {
        return Err(Status::invalid_argument(
            "model is required for claude-cli sessions",
        ));
    }

    // Require a valid, registered project — claude-cli always runs in a real worktree.
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return Err(Status::invalid_argument(
            "project_id is required for claude-cli sessions",
        ));
    }
    let projects_dir = projects_path_for_user(os_user, Some(tddy_data_dir))
        .ok_or_else(|| Status::internal("could not resolve projects path"))?;
    let project = project_storage::find_project(&projects_dir, project_id)
        .map_err(|e| Status::internal(e.to_string()))?
        .ok_or_else(|| Status::not_found("project not found"))?;
    let repo_root = PathBuf::from(&project.main_repo_path);
    if !repo_root.exists() {
        return Err(Status::invalid_argument(
            "project main repo path does not exist",
        ));
    }

    // Create session directory under sessions_base/sessions/<id>/.
    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;

    // Build branch intent and write a minimal changeset so the worktree setup fn can read it. A
    // legacy project (no stored default branch) leaves the base `None` so worktree setup resolves
    // the default live (`origin/master` → `origin/main` → `origin/HEAD`) — the same order the
    // project resolver uses.
    let ResolvedBranchWorkflow {
        intent,
        workflow: cs_workflow,
    } = resolve_branch_workflow(
        session_id,
        &BranchIntentRequest {
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
        },
        BranchIntentPolicy::claude_cli(),
        project.main_branch_ref.as_deref(),
    )?;
    let mut cs = Changeset {
        workflow: Some(cs_workflow),
        orchestrator_session_id: stack_parent.session_id().map(str::to_string),
        recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
        ..Changeset::default()
    };
    // A managed session seeds the recipe's start goal so `changeset.yaml` reflects the workflow
    // position immediately; the per-session controller advances it from there on `transition`.
    if let Some(recipe) = &managed_recipe {
        tddy_core::changeset::update_state(
            &mut cs,
            tddy_core::workflow::ids::WorkflowState::new(recipe.start_goal().as_str()),
        );
    }
    tddy_core::write_changeset(&session_dir, &cs)
        .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

    let chain_base_ref = stack_parent
        .chain_base_ref(
            project_id,
            &sessions_base,
            &repo_root,
            new_branch_name,
            selected_integration_base_ref,
        )
        .await?;
    let worktree_base_ref =
        tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);

    // Create the real git worktree (blocking: involves git fetch + git worktree add), or materialize
    // on an SSH target when `ssh_config_host` is set.
    let ssh_alias = ssh_config_host.trim();
    let timeout = config.spawn_worker_request_timeout();
    let worktree_path = if ssh_alias.is_empty() {
        let repo_root_clone = repo_root.clone();
        let session_dir_clone = session_dir.clone();
        service_util::spawn_blocking_with_timeout(
            timeout,
            "start_claude_cli_session: create worktree",
            move || {
                tddy_core::setup_worktree_for_session_with_optional_chain_base(
                    &repo_root_clone,
                    &session_dir_clone,
                    worktree_base_ref.as_deref(),
                )
                .map_err(|e| anyhow::anyhow!("worktree setup failed: {}", e))
            },
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
        &session_dir,
        &worktree_path,
        timeout,
    )
    .await?;

    // The child's branch now exists (and, when requested, is on origin), so a pr-stack
    // orchestrator's planned node can record it — which is what lets this node's descendants be
    // spawned at all, since they base onto `<remote>/<branch>`. A spawn naming a node links that
    // node on whichever daemon owns the orchestrator; one naming none keeps the branch-derived
    // local write, so a session resuming the branch a node already owns re-links to that node.
    let remote =
        project_storage::effective_remote_name_for_project(&projects_dir, project_id, &repo_root)
            .map_err(|e| Status::internal(e.to_string()))?;
    // Resolved once: the same branch is what the node records and what this session publishes about
    // itself on its participant. Read back from the changeset the worktree setup just wrote rather
    // than taken from the request — the branch may carry a collision suffix, and a node recording a
    // name nobody created leaves every descendant basing onto a ref that does not exist.
    let spawned_branch = hooks_and_urls::spawned_branch_of_session(
        &session_dir,
        hooks_and_urls::effective_spawn_branch(
            branch_worktree_intent,
            new_branch_name,
            selected_branch_to_work_on,
            &remote,
        ),
    );
    // A link that fails does **not** fail the spawn (D36): it lands after the worktree, the branch
    // and the session already exist, so failing here would leave an orphan session on this host and
    // still no branch on the orchestrator's — strictly worse than a node the operator can re-link by
    // restarting it. The live association still travels in participant metadata (D37).
    stack_parent
        .link_spawned_branch_without_failing_the_spawn(&sessions_base, &spawned_branch, session_id)
        .await;

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
        &worktree_path,
        &tddy_core::HookCommandParams {
            tddy_tools_path: &tddy_tools_path,
            daemon_url: &daemon_url,
            session_id,
            os_user,
            hook_token: &hook_token,
        },
    );

    // Spawn the claude CLI process in a PTY inside the real worktree. Resolve `claude` through the
    // shared host resolver — the same one the sandboxed path uses — so an explicit config path is
    // honored and a bare name is resolved to a real host install instead of relying on the daemon's
    // minimal systemd `PATH`.
    let manager = Arc::clone(claude_cli_manager);
    let session_id_owned = session_id.to_string();
    let model_owned = model.to_string();
    let binary_owned = hooks_and_urls::resolve_start_session_claude_binary(config);
    let worktree_clone = worktree_path.clone();

    let initial_prompt_opt = {
        let p = initial_prompt.trim();
        if p.is_empty() {
            None
        } else {
            Some(p.to_string())
        }
    };
    let permission_mode_opt = {
        let m = permission_mode.trim();
        if m.is_empty() {
            None
        } else {
            Some(m.to_string())
        }
    };

    // Managed-workflow wiring: build the per-session controller + toolcall listener, write the
    // recipe's orchestration prompt to a file `claude` appends to its system prompt, and inject
    // a per-session TDDY_SOCKET (+ a PATH that resolves tddy-tools) so the agent's host-side
    // `tddy-tools transition` reaches this session's controller.
    let mut managed: Option<crate::session_toolcall::ManagedWorkflow> = None;
    let mut append_system_prompt_file: Option<PathBuf> = None;
    let mut env_extra: Vec<(String, String)> = Vec::new();
    if let Some(recipe) = managed_recipe.clone() {
        let launch = managed_launch::prepare_managed_workflow_inner(
            tddy_data_dir,
            session_id,
            recipe,
            &session_dir,
            &worktree_path,
            &session_dir,
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
            &worktree_path,
            &session_dir,
        )
        .await?;
        let (key, value) = tddy_semantic_index::semantic_index::semantic_index_env(&session_dir);
        env_extra.push((key, value));
    }

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

    let pid = handle.pid;

    // Write .session.yaml.
    let now = chrono::Utc::now().to_rfc3339();
    let meta = tddy_core::SessionMetadata {
        session_id: session_id.to_string(),
        project_id: project_id.to_string(),
        created_at: now.clone(),
        updated_at: now,
        status: "active".to_string(),
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: Some(pid),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some(model.to_string()),
        cursor_chat_id: None,
        activity_status: None,
        hook_token: Some(hook_token),
        sandbox: None,
        agent: None,
        recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
        ssh_config_host: if ssh_alias.is_empty() {
            None
        } else {
            Some(ssh_alias.to_string())
        },
    };
    tddy_core::write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {}", e)))?;

    // What this session tells the fleet about itself, recorded now and published when its terminal
    // is first bridged into LiveKit. The stack association is the load-bearing part: a PR-Stack view
    // on another host has no other way to learn that this session is the planned node's child
    // (D37), and it is knowledge this call has and a later reader does not — no session directory
    // records which planned node was materialized — so it is kept rather than re-derived.
    //
    // Recording it is local work over values already in hand. Putting a participant in the room is
    // not: it is a network round-trip to a server this daemon does not control, and a session is
    // made of a checkout and a process, both of which already exist by now. That join belongs to
    // the moment a LiveKit consumer arrives, which is the same moment the session's room is opened
    // — see `SessionRoomRegistry::ensure_open`. The desktop reaches its own host over IPC and
    // drives this terminal without a bridge at all.
    claude_cli_manager
        .expose_terminal_to_livekit(
            session_id,
            hooks_and_urls::claude_cli_participant_metadata(
                &hooks_and_urls::StartingClaudeCliSession {
                    session_id,
                    model,
                    recipe: managed_recipe
                        .as_ref()
                        .map(|r| r.name())
                        .unwrap_or_default(),
                    worktree_path: &worktree_path,
                    branch: &spawned_branch,
                    stack_parent: &stack_parent,
                },
            ),
        )
        .await;

    // Where that terminal will be served once it is bridged. Derived from the session id and the
    // deployment config rather than read off a connection, so it is the same answer whether a
    // consumer has arrived yet or not — and deriving it contacts nothing.
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

    log::info!(
        target: "tddy_daemon::connection_service",
        "started claude-cli session {} pid={} worktree={} user={}",
        session_id,
        pid,
        worktree_path.display(),
        os_user
    );

    Ok(Response::new(StartSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: lk_room,
        livekit_url: lk_url,
        livekit_server_identity: lk_server_identity,
        branch_conflict: None,
    }))
}
