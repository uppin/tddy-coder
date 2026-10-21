//! OpenCode (`opencode-cli`) session start and resume for the launch topic.
//!
//! The same shape as a Cursor session — a daemon-cut worktree, a PTY relayed over the daemon's own
//! connection — minus what OpenCode has no counterpart of: no hooks are installed, no chat id is
//! minted (a resume is `--continue` in the worktree, and a session owns its worktree), and the
//! session is never sandboxed.
//!
//! A chosen provider account is applied as a route ([`start_cli_route`]): its generated
//! `opencode.json` lives in the session directory, its key reaches the child's environment only.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::cli_harness::{cli_harness, CliHarness, CliLaunch};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::provider_route::RouteSources;
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_service::proto::session::{ResumeSessionResponse, StartSessionResponse};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_worktree_service::branch_intent::{BranchIntentPolicy, BranchIntentRequest};

use crate::cli_route_launch::{resume_cli_route, start_cli_route, CliRouteLaunch};
use crate::cli_session_worktree::{
    provision_cli_worktree, semantic_index_env, CliWorktree, CliWorktreeStart,
};
use crate::session_acting_identity::SessionAccountAccess;
use crate::stack_parent::SpawnStackParent;

/// The session type this module serves.
const OPENCODE_CLI: &str = "opencode-cli";

/// Everything one OpenCode start reads.
pub(crate) struct OpenCodeCliStart<'a> {
    pub config: &'a DaemonConfig,
    pub tddy_data_dir: &'a Path,
    pub cli_manager: &'a Arc<CliSessionManager>,
    pub os_user: &'a str,
    pub session_id: &'a str,
    /// Authorizes the `workspace` starts this session's seeded agents need on their own hosts.
    pub session_token: &'a str,
    pub sessions_base: PathBuf,
    /// The model the operator chose; empty leaves the choice to OpenCode.
    pub model: &'a str,
    pub project_id: &'a str,
    pub repo_path: &'a str,
    pub branch_worktree_intent: &'a str,
    pub new_branch_name: &'a str,
    pub selected_integration_base_ref: &'a str,
    pub selected_branch_to_work_on: &'a str,
    pub stack_parent: SpawnStackParent<'a>,
    pub initial_prompt: &'a str,
    /// The session's starting agent roster, already resolved by the caller.
    pub agents: &'a mut [tddy_core::SessionAgentRecord],
    pub semantic_index: bool,
    pub create_remote_branch: bool,
    pub task_registry: &'a tddy_task::TaskRegistry,
    pub agent_clones: &'a dyn tddy_session_agents::seed_codebase::SeededAgentClones,
    /// What the start reads its owner's vault with: the provider account and the commit identity.
    pub account_access: &'a SessionAccountAccess,
    pub route_sources: &'a RouteSources,
    pub progress: &'a AttachmentProgressSink,
}

fn opencode_harness() -> Result<&'static CliHarness, Status> {
    cli_harness(OPENCODE_CLI)
        .ok_or_else(|| Status::internal("the opencode-cli harness is not registered"))
}

/// The binary OpenCode is run as: the environment, then `cli_harnesses.opencode-cli.binary_path`
/// in the daemon config, then the registry's default.
fn opencode_binary(config: &DaemonConfig, harness: &CliHarness) -> String {
    config.cli_harness_binary(
        harness.session_type,
        harness.default_binary,
        harness.binary_env,
    )
}

/// Start an OpenCode session: cut its worktree, apply its provider route, spawn `opencode` in a
/// PTY and record `.session.yaml`.
pub(crate) async fn start_opencode_cli_session(
    start: OpenCodeCliStart<'_>,
) -> Result<Response<StartSessionResponse>, Status> {
    let OpenCodeCliStart {
        config,
        tddy_data_dir,
        cli_manager,
        os_user,
        session_id,
        session_token,
        sessions_base,
        model,
        project_id,
        repo_path,
        branch_worktree_intent,
        new_branch_name,
        selected_integration_base_ref,
        selected_branch_to_work_on,
        stack_parent,
        initial_prompt,
        agents,
        semantic_index,
        create_remote_branch,
        task_registry,
        agent_clones,
        account_access,
        route_sources,
        progress,
    } = start;
    let harness = opencode_harness()?;
    let project_id = project_id.trim();
    let repo_path = repo_path.trim();

    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;

    // Resolved before anything is cut or spawned: an account that is missing, locked or cannot
    // serve OpenCode fails the start, and OpenCode never silently runs on its own setup instead.
    let route = start_cli_route(account_access, harness, route_sources, model, &session_dir)?;

    let intent = tddy_session_split::service_util::write_initial_changeset(
        session_id,
        &BranchIntentRequest {
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
        },
        BranchIntentPolicy::opencode_cli(),
        None,
        &session_dir,
        stack_parent.session_id(),
        None,
    )?;

    let CliWorktree {
        path: worktree_path,
        project,
    } = provision_cli_worktree(
        &CliWorktreeStart {
            session_type: OPENCODE_CLI,
            cut_label: "start_opencode_cli_session: create worktree",
            session_id,
            session_dir: &session_dir,
            sessions_base: &sessions_base,
            tddy_data_dir,
            os_user,
            project_id,
            repo_path,
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
            create_remote_branch,
            intent,
            timeout: config.spawn_worker_request_timeout(),
        },
        &stack_parent,
        progress,
    )
    .await?;

    // The clones this session's seeded agents read, claimed now: the worktree they mirror exists.
    let seeded_clones = agent_clones
        .claim_for_seed(
            session_id,
            &tddy_session_agents::seed_codebase::SeedCodebase::of_a_starting_session(
                session_dir.clone(),
                worktree_path.clone(),
                project_id,
                // An unsandboxed co-located session enforces no withdrawal.
                false,
            ),
            session_token,
            agents,
        )
        .await?;

    let mut env = semantic_index_env(
        tddy_data_dir,
        session_id,
        semantic_index,
        task_registry,
        &session_dir,
        &worktree_path,
        progress,
    )
    .await?;
    // The agent's commits carry the project's account identity; a refusal starts the session under
    // the checkout's own, with the reason logged.
    env.extend(
        account_access
            .session_identity_in(session_id, os_user, &session_dir, project.as_ref())
            .git_environment,
    );

    progress.begin_phase(StartStep::Agent);
    let binary = opencode_binary(config, harness);
    // A routed launch leaves the model to the route, which names it on the generated provider.
    let launch_model = if route.is_some() { "" } else { model };
    let launched = launch(
        cli_manager,
        harness,
        session_id,
        &worktree_path,
        &CliLaunch {
            binary: &binary,
            model: launch_model,
            initial_prompt: Some(initial_prompt),
            ..CliLaunch::default()
        },
        route.unwrap_or_default(),
        env,
    )
    .await?;
    progress.end_phase(StartStep::Agent);

    let pid = launched.pid;
    let meta = SessionMetadata {
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: Some(pid),
        model: Some(model.to_string()).filter(|model| !model.is_empty()),
        agents_rev: tddy_session_agents::agent_roster::started_roster_rev(agents),
        agents: agents.to_vec(),
        ..tddy_session_split::service_util::starting_session_metadata(
            session_id,
            project_id,
            OPENCODE_CLI,
        )
    };
    write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
    // The roster naming them is on disk now, so the clones belong to the session rather than to the
    // start that claimed them.
    seeded_clones.keep();

    log::info!(
        target: "tddy_daemon::connection_service",
        "started opencode-cli session {session_id} pid={pid} worktree={} user={os_user}",
        worktree_path.display()
    );

    Ok(Response::new(StartSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
        branch_conflict: None,
    }))
}

/// Resume an OpenCode session: `opencode --continue` in its worktree, with its provider route
/// re-resolved from the vault as it stands now. Refused when the worktree is gone — there is
/// nothing for `--continue` to continue.
pub(crate) async fn resume_opencode_cli_session(
    cli_manager: &Arc<CliSessionManager>,
    config: &DaemonConfig,
    account_access: &SessionAccountAccess,
    session_id: &str,
    session_dir: &Path,
    meta: SessionMetadata,
    // The project's account identity pairs the resumed agent's commits carry (empty keeps the
    // checkout's own).
    git_environment: Vec<(String, String)>,
) -> Result<Response<ResumeSessionResponse>, Status> {
    let harness = opencode_harness()?;
    let worktree_path = meta
        .repo_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| session_dir.to_path_buf());
    if !worktree_path.exists() {
        return Err(Status::failed_precondition(
            "worktree no longer exists; cannot resume opencode-cli session",
        ));
    }

    let route = resume_cli_route(account_access, harness, session_dir)?;
    let binary = opencode_binary(config, harness);
    let launched = launch(
        cli_manager,
        harness,
        session_id,
        &worktree_path,
        &CliLaunch {
            binary: &binary,
            resume: true,
            ..CliLaunch::default()
        },
        route,
        git_environment,
    )
    .await?;

    let mut updated = meta;
    updated.pid = Some(launched.pid);
    updated.status = "active".to_string();
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    write_session_metadata(session_dir, &updated)
        .map_err(|e| Status::internal(format!("failed to update session metadata: {e}")))?;

    Ok(Response::new(ResumeSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
    }))
}

/// Spawn `harness` with `launch`, followed by the route's arguments and with its environment added.
async fn launch(
    cli_manager: &Arc<CliSessionManager>,
    harness: &CliHarness,
    session_id: &str,
    worktree_path: &Path,
    launch: &CliLaunch<'_>,
    route: CliRouteLaunch,
    mut env: Vec<(String, String)>,
) -> Result<Arc<tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle>, Status> {
    env.extend(route.env);
    cli_manager
        .launch_cli_harness(
            harness,
            session_id,
            worktree_path.to_path_buf(),
            launch,
            route.args,
            env,
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn {}: {e}", harness.session_type)))
}
