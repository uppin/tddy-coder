//! Start and resume of a registered CLI harness that is cut a worktree of its own — OpenCode
//! (`opencode-cli`) and Muse Code (`muse-cli`) — for the launch topic. The harness is the caller's
//! registry entry ([`CliHarness`]); nothing here is particular to one of them.
//!
//! The same shape as a Cursor session — a daemon-cut worktree, a PTY relayed over the daemon's own
//! connection — minus what these harnesses have no counterpart of: no hooks are installed, no chat
//! id is minted (a resume is `--continue` in the worktree, and a session owns its worktree), and the
//! session is never sandboxed.
//!
//! A chosen provider account is applied as a route ([`start_cli_route`]): any file it generates
//! (OpenCode's `opencode.json`) lives in the session directory, its key reaches the child's
//! environment only.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::cli_harness::{CliHarness, CliLaunch};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::provider_route::RouteSources;
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_service::proto::session::{
    ResumeSessionRequest, ResumeSessionResponse, StartSessionResponse,
};
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_worktree_service::branch_intent::{BranchIntentPolicy, BranchIntentRequest};

use crate::cli_resume_token::{resume_token_recorder, settled_resume_token};
use crate::cli_route_launch::{
    launch_model_for, resume_cli_route, start_cli_route, CliRouteLaunch,
};
use crate::cli_session_worktree::{
    provision_cli_worktree, semantic_index_env, CliWorktree, CliWorktreeStart,
};
use crate::session_acting_identity::SessionAccountAccess;
use crate::stack_parent::SpawnStackParent;
use tddy_cli_sessions::cli_session_manager::{ExitHook, HarnessLaunchExtras};

/// Everything one CLI harness start reads.
pub(crate) struct CliHarnessStart<'a> {
    /// The registered harness the session runs.
    pub harness: &'static CliHarness,
    pub config: &'a DaemonConfig,
    pub tddy_data_dir: &'a Path,
    pub cli_manager: &'a Arc<CliSessionManager>,
    pub os_user: &'a str,
    pub session_id: &'a str,
    /// Authorizes the `workspace` starts this session's seeded agents need on their own hosts.
    pub session_token: &'a str,
    pub sessions_base: PathBuf,
    /// The model the operator chose; empty leaves the choice to the harness.
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

/// The binary `harness` is run as: its environment variable, then
/// `cli_harnesses.<session type>.binary_path` in the daemon config, then the registry's default.
/// A bare default name is resolved on the host — `~/.local/bin/<name>`, then `$PATH` — because the
/// daemon runs under systemd with a minimal `PATH` that never holds the user-local install dirs
/// (and the impersonated child would otherwise fail the same way). A name that resolves nowhere is
/// left bare, so the spawner's error is the familiar "not found on PATH".
pub(crate) fn harness_binary(config: &DaemonConfig, harness: &CliHarness) -> String {
    let binary = config.cli_harness_binary(
        harness.session_type,
        harness.default_binary,
        harness.binary_env,
    );
    tddy_daemon_kernel::config::resolve_harness_binary_name(
        binary,
        tddy_daemon_kernel::config::find_real_harness_on_host,
    )
}

// TODO(harness-clis): launch from the session type's spec through `launch_host_pty`, rendered
// with `crate::harness_from_spec::harness_template_params`, instead of `CliHarness::argv`.
/// Start a CLI harness session: cut its worktree, apply its provider route, spawn the harness in
/// a PTY and record `.session.yaml`.
pub(crate) async fn start_cli_harness_session(
    start: CliHarnessStart<'_>,
) -> Result<Response<StartSessionResponse>, Status> {
    let CliHarnessStart {
        harness,
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
    let project_id = project_id.trim();
    let repo_path = repo_path.trim();

    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;

    // Resolved before anything is cut or spawned: an account that is missing, locked or cannot
    // serve the harness fails the start, and the harness never silently runs on its own setup
    // instead.
    let route = start_cli_route(account_access, harness, route_sources, model, &session_dir)?;

    let intent = tddy_session_split::service_util::write_initial_changeset(
        session_id,
        &BranchIntentRequest {
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
        },
        BranchIntentPolicy::cli_harness(harness.session_type),
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
            session_type: harness.session_type,
            cut_label: "start_cli_harness_session: create worktree",
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
    let binary = harness_binary(config, harness);
    let launch_model = launch_model_for(harness, route.is_some(), model);
    let (token_recorder, token_arm) = resume_token_recorder(session_dir.clone());
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
        launch_extras(route.unwrap_or_default(), env, token_recorder),
        Some(os_user),
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
            harness.session_type,
        )
    };
    write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
    // Armed only once the metadata is on disk: the recorder adds its token to it.
    token_arm.arm();
    // The roster naming them is on disk now, so the clones belong to the session rather than to the
    // start that claimed them.
    seeded_clones.keep();

    log::info!(
        target: "tddy_daemon::connection_service",
        "started {} session {session_id} pid={pid} worktree={} user={os_user}",
        harness.session_type,
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

impl crate::launch_ports::LaunchSessions {
    /// The resume of a CLI harness session: the session's commit identity and its owner's vault,
    /// then [`resume_cli_harness_session`].
    pub(crate) async fn resume_cli_harness(
        &self,
        harness: &'static CliHarness,
        os_user: &str,
        req: &ResumeSessionRequest,
        session_dir: &Path,
        mut metadata: SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        // The previous run's recorder may still be writing the token it printed as it exited.
        metadata.harness_resume_token =
            settled_resume_token(&self.claude_cli_manager, &req.session_id, session_dir).await?;
        let identity = self.host.session_identity(
            os_user,
            &req.session_id,
            &metadata.project_id,
            &req.session_token,
        );
        resume_cli_harness_session(
            harness,
            &self.claude_cli_manager,
            &self.config,
            &self.host.session_account_access(&req.session_token),
            ResumedSession {
                session_id: &req.session_id,
                session_dir,
                meta: metadata,
                git_environment: identity.git_environment,
                os_user,
            },
        )
        .await
    }
}

/// The stopped session a resume relaunches.
struct ResumedSession<'a> {
    session_id: &'a str,
    session_dir: &'a Path,
    meta: SessionMetadata,
    /// The project's account identity pairs the resumed agent's commits carry (empty keeps the
    /// checkout's own).
    git_environment: Vec<(String, String)>,
    /// The OS user the session runs as, for the child's `HOME`/`PATH` overrides.
    os_user: &'a str,
}

/// Resume a CLI harness session: the harness's resume argv (`--continue`) in its worktree, with its
/// provider route re-resolved from the vault as it stands now. Refused when the worktree is gone —
/// there is nothing for `--continue` to continue.
async fn resume_cli_harness_session(
    harness: &CliHarness,
    cli_manager: &Arc<CliSessionManager>,
    config: &DaemonConfig,
    account_access: &SessionAccountAccess,
    session: ResumedSession<'_>,
) -> Result<Response<ResumeSessionResponse>, Status> {
    let ResumedSession {
        session_id,
        session_dir,
        meta,
        git_environment,
        os_user,
    } = session;
    let worktree_path = meta
        .repo_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| session_dir.to_path_buf());
    if !worktree_path.exists() {
        return Err(Status::failed_precondition(format!(
            "worktree no longer exists; cannot resume {} session",
            harness.session_type
        )));
    }

    let route = resume_cli_route(account_access, harness, session_dir)?;
    let binary = harness_binary(config, harness);
    let (token_recorder, token_arm) = resume_token_recorder(session_dir.to_path_buf());
    let launched = launch(
        cli_manager,
        harness,
        session_id,
        &worktree_path,
        &CliLaunch {
            binary: &binary,
            conversation_id: meta.harness_resume_token.as_deref(),
            resume: true,
            ..CliLaunch::default()
        },
        launch_extras(route, git_environment, token_recorder),
        Some(os_user),
    )
    .await?;

    let mut updated = meta;
    updated.pid = Some(launched.pid);
    updated.status = "active".to_string();
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    write_session_metadata(session_dir, &updated)
        .map_err(|e| Status::internal(format!("failed to update session metadata: {e}")))?;
    // Armed only once the metadata is on disk, for the same reason as the start.
    token_arm.arm();

    Ok(Response::new(ResumeSessionResponse {
        session_id: session_id.to_string(),
        livekit_room: String::new(),
        livekit_url: String::new(),
        livekit_server_identity: String::new(),
    }))
}

/// What a launch adds to the harness's argv: the route's arguments, `env` with the route's
/// environment on top, and `on_exit` run once the process has exited.
fn launch_extras(
    route: CliRouteLaunch,
    mut env: Vec<(String, String)>,
    on_exit: ExitHook,
) -> HarnessLaunchExtras {
    env.extend(route.env);
    HarnessLaunchExtras {
        args: route.args,
        env,
        on_exit: Some(on_exit),
    }
}

/// Spawn `harness` with `launch`, followed by `extras`, as `os_user` — the session owner, whose
/// `HOME` and configured `PATH` extra the child must run under, or its user-local binary and
/// credentials are never found (a systemd daemon's own `PATH` is minimal).
async fn launch(
    cli_manager: &Arc<CliSessionManager>,
    harness: &CliHarness,
    session_id: &str,
    worktree_path: &Path,
    launch: &CliLaunch<'_>,
    extras: HarnessLaunchExtras,
    os_user: Option<&str>,
) -> Result<Arc<tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle>, Status> {
    cli_manager
        .launch_cli_harness(
            harness,
            session_id,
            worktree_path.to_path_buf(),
            launch,
            extras,
            os_user,
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn {}: {e}", harness.session_type)))
}
