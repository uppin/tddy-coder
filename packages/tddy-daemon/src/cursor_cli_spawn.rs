//! Cursor Agent CLI session spawn/resume helpers for `ConnectionServiceImpl`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::{
    build_cursor_hooks_settings, write_session_metadata, Changeset, HookCommandParams,
    SessionMetadata,
};
use tddy_rpc::{Response, Status};
use tddy_service::proto::connection::{ResumeSessionResponse, StartSessionResponse};
use uuid::Uuid;

use crate::branch_intent::{
    resolve_branch_workflow, BranchIntentPolicy, BranchIntentRequest, ResolvedBranchWorkflow,
};
use crate::cli_session_manager::CliSessionManager;
use crate::config::{resolve_cursor_binary_path, DaemonConfig};
use crate::connection_service::{
    session_worktree_source, spawn_blocking_with_timeout, WorktreeSource,
};
use crate::project_storage;
use crate::user_sessions_path::projects_path_for_user;

/// Write `.cursor/hooks.json` under `worktree_path` for a cursor-cli session.
///
/// Returns the generated per-session hook token (also embedded in hook commands).
pub fn install_cursor_hooks_in_worktree(
    config: &DaemonConfig,
    worktree_path: &Path,
    session_id: &str,
    os_user: &str,
) -> String {
    let tddy_tools_path = crate::sandbox_session::resolve_tddy_tools_path(
        crate::config::resolve_cursor_cli_tddy_tools_path(config).as_deref(),
    );

    // `cursor_cli.daemon_url`, then `claude_cli.daemon_url`, then this daemon's own web listener —
    // the same last resort every hook URL falls back to.
    let daemon_url = crate::config::resolve_cursor_cli_daemon_url(config)
        .unwrap_or_else(|| crate::connection_service::local_daemon_hook_url(config));

    let hook_token = Uuid::new_v4().to_string();
    let hooks_settings = build_cursor_hooks_settings(&HookCommandParams {
        tddy_tools_path: &tddy_tools_path,
        daemon_url: &daemon_url,
        session_id,
        os_user,
        hook_token: &hook_token,
    });
    let cursor_dir = worktree_path.join(".cursor");
    if let Err(e) = std::fs::create_dir_all(&cursor_dir).and_then(|_| {
        serde_json::to_string_pretty(&hooks_settings)
            .map_err(|e| std::io::Error::other(e.to_string()))
            .and_then(|json| std::fs::write(cursor_dir.join("hooks.json"), json))
    }) {
        log::warn!(
            "session {session_id}: failed to write .cursor/hooks.json — hooks will not fire: {e}"
        );
    }
    hook_token
}

/// The chat id printed by `cursor-agent create-chat`, or a description of what came out instead.
///
/// `create-chat` prints a **bare** chat id on stdout and exits 0 ("Create a new empty chat and
/// return its ID" — cursor-agent 2026.07.23). Anything else — no output, a banner, a prompt, an
/// error message — is not a chat id, and pinning it to the session would send every later resume
/// into a chat that does not exist.
pub fn parse_created_chat_id(stdout: &str) -> Result<String, String> {
    let chat_id = stdout.trim();
    if chat_id.is_empty() {
        return Err("printed no chat id on stdout".to_string());
    }
    if chat_id.split_whitespace().count() > 1 {
        return Err(format!(
            "printed {chat_id:?} on stdout, which is not a bare chat id"
        ));
    }
    Ok(chat_id.to_string())
}

/// Mint the Cursor chat a session will own by running `<binary_path> create-chat` in `worktree_path`.
///
/// Only `create-chat` mints real chat ids, so a failure here is returned to the caller: starting the
/// agent without one would produce a session no resume can reattach to.
pub async fn mint_cursor_chat_id(
    binary_path: &str,
    worktree_path: &Path,
) -> Result<String, String> {
    let output = tokio::process::Command::new(binary_path)
        .arg("create-chat")
        .current_dir(worktree_path)
        .output()
        .await
        .map_err(|e| format!("`{binary_path} create-chat` could not be run: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{binary_path} create-chat` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_created_chat_id(&String::from_utf8_lossy(&output.stdout))
        .map_err(|e| format!("`{binary_path} create-chat` {e}"))
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_cursor_cli_session_inner(
    config: &DaemonConfig,
    tddy_data_dir: &Path,
    cli_manager: &Arc<CliSessionManager>,
    os_user: &str,
    session_id: &str,
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
    stack_parent: Option<&str>,
    initial_prompt: &str,
    managed_codebase: bool,
    specialized_agents: &[String],
    managed_recipe: Option<Arc<dyn tddy_core::backend::WorkflowRecipe>>,
    // When true, index the worktree before launch (blocking; aborts on failure) and point the
    // `SemanticSearch` tool at the per-session index via `TDDY_SEMANTIC_INDEX_DB`.
    semantic_index: bool,
    // When true (new_branch_from_base only), push the new branch to origin at session start.
    create_remote_branch: bool,
    task_registry: &tddy_task::TaskRegistry,
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
        BranchIntentPolicy::cursor_cli(),
        None,
    )?;
    let mut cs = Changeset {
        workflow: Some(cs_workflow),
        orchestrator_session_id: stack_parent.map(str::to_string),
        recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
        ..Changeset::default()
    };
    if let Some(recipe) = &managed_recipe {
        tddy_core::changeset::update_state(
            &mut cs,
            tddy_core::workflow::ids::WorkflowState::new(recipe.start_goal().as_str()),
        );
    }
    tddy_core::write_changeset(&session_dir, &cs)
        .map_err(|e| Status::internal(format!("failed to write changeset: {}", e)))?;

    let timeout = config.spawn_worker_request_timeout();
    let worktree_path = match session_worktree_source(repo_path, project_id) {
        WorktreeSource::Project(pid) => {
            if pid.is_empty() {
                return Err(Status::invalid_argument(
                    "project_id is required for cursor-cli sessions",
                ));
            }
            let projects_dir = projects_path_for_user(os_user, Some(tddy_data_dir))
                .ok_or_else(|| Status::internal("could not resolve projects path"))?;
            let project = project_storage::find_project(&projects_dir, &pid)
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found("project not found"))?;
            let repo_root = PathBuf::from(&project.main_repo_path);
            if !repo_root.exists() {
                return Err(Status::invalid_argument(
                    "project main repo path does not exist",
                ));
            }
            let chain_base_ref = tddy_core::resolve_chain_base_ref(
                &sessions_base,
                stack_parent,
                &repo_root,
                new_branch_name,
            )
            .map_err(Status::failed_precondition)?;
            let worktree_base_ref =
                tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);
            let repo_root_clone = repo_root.clone();
            let session_dir_clone = session_dir.clone();
            let wt = spawn_blocking_with_timeout(
                timeout,
                "start_cursor_cli_session: create worktree",
                move || {
                    tddy_core::setup_worktree_for_session_with_optional_chain_base(
                        &repo_root_clone,
                        &session_dir_clone,
                        worktree_base_ref.as_deref(),
                    )
                    .map_err(|e| anyhow::anyhow!("worktree setup failed: {}", e))
                },
            )
            .await?;
            crate::connection_service::push_new_branch_to_origin_if_requested(
                create_remote_branch,
                intent,
                &session_dir,
                &wt,
                timeout,
            )
            .await?;
            wt
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
            canonical
        }
    };

    let hook_token = install_cursor_hooks_in_worktree(config, &worktree_path, session_id, os_user);

    let binary_path = resolve_cursor_binary_path(config);
    let initial_prompt_opt = {
        let p = initial_prompt.trim();
        if p.is_empty() {
            None
        } else {
            Some(p.to_string())
        }
    };
    if managed_recipe.is_some() {
        let rules_dir = worktree_path.join(".cursor").join("rules");
        let _ = std::fs::create_dir_all(&rules_dir);
        if let Some(recipe) = &managed_recipe {
            let _ = std::fs::write(
                rules_dir.join("tddy-managed-workflow.mdc"),
                format!("Managed workflow recipe: {}\n", recipe.name()),
            );
        }
    }
    let _ = (managed_codebase, specialized_agents);

    // Semantic index: index the worktree into the session dir before launch (blocking; a missing
    // embedder or a failed index aborts the start — no unindexed fallback), and point the
    // `SemanticSearch` tool at the per-session index DB via the process env.
    let mut session_env: Vec<(String, String)> = Vec::new();
    if semantic_index {
        let embedder = tddy_semantic_index::production_embedder(tddy_data_dir).map_err(|e| {
            Status::failed_precondition(format!(
                "semantic index requested but no embedder is available: {e}"
            ))
        })?;
        crate::semantic_index::run_semantic_index_blocking(
            &worktree_path,
            &session_dir,
            embedder,
            task_registry,
            session_id,
        )
        .await
        .map_err(|e| Status::internal(format!("semantic index failed: {e}")))?;
        session_env.push(crate::semantic_index::semantic_index_env(&session_dir));
    }

    // The Cursor chat this session owns for its whole lifetime: minted here, persisted in
    // `.session.yaml` below, and passed as `--resume <id>` on every later spawn so a resume
    // continues this chat instead of opening a new one.
    let cursor_chat_id = mint_cursor_chat_id(&binary_path, &worktree_path)
        .await
        .map_err(|e| {
            Status::internal(format!(
                "failed to create the Cursor chat for session {session_id}: {e}"
            ))
        })?;

    let handle = cli_manager
        .start_cursor(
            session_id,
            worktree_path.clone(),
            model,
            &binary_path,
            Some(&cursor_chat_id),
            initial_prompt_opt.as_deref(),
            session_env,
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn cursor-cli: {}", e)))?;

    let pid = handle.pid;
    let now = chrono::Utc::now().to_rfc3339();
    let meta = SessionMetadata {
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
        session_type: Some("cursor-cli".to_string()),
        model: Some(model.to_string()),
        cursor_chat_id: Some(cursor_chat_id),
        activity_status: None,
        hook_token: Some(hook_token),
        sandbox: None,
        agent: None,
        recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),
        specialized_agents: specialized_agents.to_vec(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
    };
    write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {}", e)))?;

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

pub async fn resume_cursor_cli_session(
    cli_manager: &Arc<CliSessionManager>,
    config: &DaemonConfig,
    session_id: &str,
    session_dir: &Path,
    meta: SessionMetadata,
) -> Result<Response<ResumeSessionResponse>, Status> {
    let model = meta.model.clone().unwrap_or_default();
    let worktree_path = meta
        .repo_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| session_dir.to_path_buf());

    if !worktree_path.exists() {
        return Err(Status::failed_precondition(
            "worktree no longer exists; cannot resume cursor-cli session",
        ));
    }

    let binary_path = resolve_cursor_binary_path(config);
    // The chat to reattach to. A session started before chat ids were recorded has no way back to
    // its original chat, so its first resume adopts a fresh one and pins it below — every later
    // resume then reattaches instead of starting over again.
    let chat_id = match meta
        .cursor_chat_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        Some(id) => id.to_string(),
        None => {
            let adopted = mint_cursor_chat_id(&binary_path, &worktree_path)
                .await
                .map_err(|e| {
                    Status::internal(format!(
                        "failed to create the Cursor chat for session {session_id}: {e}"
                    ))
                })?;
            log::info!(
                target: "tddy_daemon::cursor_cli_spawn",
                "resume_cursor_cli_session {session_id}: no cursor_chat_id on record; adopted chat {adopted} for this and every later resume"
            );
            adopted
        }
    };

    let handle = cli_manager
        .resume_cursor(
            session_id,
            worktree_path.clone(),
            &model,
            &binary_path,
            Some(&chat_id),
        )
        .await
        .map_err(|e| Status::internal(format!("failed to resume cursor-cli: {}", e)))?;

    let pid = handle.pid;
    let mut updated = meta;
    updated.cursor_chat_id = Some(chat_id);
    updated.pid = Some(pid);
    updated.status = "active".to_string();
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    write_session_metadata(session_dir, &updated)
        .map_err(|e| Status::internal(format!("failed to update session metadata: {}", e)))?;

    Ok(Response::new(ResumeSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
    }))
}
