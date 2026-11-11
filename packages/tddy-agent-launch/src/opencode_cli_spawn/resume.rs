//! Resuming a CLI harness session: its re-derived wiring, its worktree and the agent restarted on
//! it.

use crate::cli_agent_steps::conversation::resumed_or_adopted_conversation;
use crate::cli_agent_steps::conversation::ConversationStart;
use crate::cli_agent_steps::{
    render_host_launch_slots, write_slot_files_on_target, HostLaunchSlots, ResumedWiring,
};
use crate::cli_resume_token::{resume_token_recorder, settled_resume_token};
use crate::cli_route_launch::resume_cli_route;
use crate::cli_session_pointer::{
    place_session_pointer, removing_session_pointer_on_exit, PointerHost,
};
use crate::cursor_from_spec::node_launcher_of;
use crate::harness_from_spec::{
    harness_template_params, resolve_session_agent, resume_token_adapter, CliSessionAgent,
    CliSessionType, HarnessLaunch,
};
use crate::opencode_cli_spawn::harness_launch::{
    launch, HarnessRun, HarnessSession, HarnessSpecLaunch,
};
use crate::session_acting_identity::SessionAccountAccess;
use crate::usage_context::{record_usage_context, UsageSubject};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::user_paths::home_dir_for_user;
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::{ResumeSessionRequest, ResumeSessionResponse};

impl crate::launch_ports::LaunchSessions {
    /// The resume of a CLI harness session: a split session's re-derived wiring, the session's
    /// commit identity and its owner's vault, then [`resume_cli_harness_session`].
    pub(crate) async fn resume_cli_harness(
        &self,
        kind: CliSessionType,
        os_user: &str,
        req: &ResumeSessionRequest,
        sessions_base: &Path,
        session_dir: &Path,
        mut metadata: SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        // Nothing of a split session's tool transport survives a stop, and its roster lives beside
        // its codebase: both are re-derived from the persisted pairing (with a fresh join token),
        // and a codebase host that cannot be read refuses the resume. `None` for a co-located one.
        let split = self
            .split_sessions
            .resume_split_wiring(
                &metadata,
                os_user,
                sessions_base,
                session_dir,
                &req.session_id,
                &req.session_token,
            )
            .await?;
        if metadata.tmux_session.is_some() {
            // A tmux-run agent that exited unwatched left its token on its kept pane.
            crate::cli_harness_tmux::settle_unwatched_agent_exit(
                &self.cli_manager,
                Some(&*self.coding_agents),
                os_user,
                &req.session_id,
                kind.name,
                session_dir,
            )
            .await?;
        }
        // The previous run's recorder may still be writing the token it printed as it exited.
        metadata.harness_resume_token =
            settled_resume_token(&self.cli_manager, &req.session_id, session_dir).await?;
        let session_agent = resolve_session_agent(&self.coding_agents, kind.clone())?;
        let identity = self.host.session_identity(
            os_user,
            &req.session_id,
            &metadata.project_id,
            &req.session_token,
        );
        // A split agent runs in its context directory, standing in for the checkout it does not have.
        let worktree = split
            .as_ref()
            .map(|split| split.context_dir.clone())
            .or_else(|| metadata.repo_path.as_ref().map(PathBuf::from))
            .unwrap_or_else(|| session_dir.to_path_buf());
        // A worktree that is gone leaves nothing to resume: refused before anything is re-wired.
        refuse_a_resume_without_its_worktree(kind.name, &worktree)?;
        let wiring = self
            .resumed_session_wiring(
                &session_agent.agent,
                &metadata,
                session_dir,
                &worktree,
                os_user,
                &req.session_token,
                identity.github_credential,
            )
            .await?;
        resume_cli_harness_session(
            &session_agent,
            &self.cli_manager,
            &self.host.session_account_access(&req.session_token),
            ResumedSession {
                session_id: &req.session_id,
                session_dir,
                meta: metadata,
                worktree,
                // A split agent has no checkout: its commits are made by the tools on the codebase
                // daemon, never by this process's environment.
                git_environment: if split.is_some() {
                    Vec::new()
                } else {
                    identity.git_environment
                },
                split,
                os_user,
                wiring,
                tddy_data_dir: &self.tddy_data_dir,
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
    /// Where the agent resumes: its worktree, or a split agent's context directory.
    worktree: PathBuf,
    /// The project's account identity pairs the resumed agent's commits carry (empty keeps the
    /// checkout's own, and a split agent's).
    git_environment: Vec<(String, String)>,
    /// A split session's re-derived wiring: its pre-rendered flags (the roster's withdrawals, the
    /// `tddy-tools` registration) and its `TDDY_REMOTE_*` transport, as its split spawn gives them.
    split: Option<tddy_session_split::split_session::SplitAgentWiring>,
    /// The OS user the session runs as, for the child's `HOME`/`PATH` overrides.
    os_user: &'a str,
    /// What the resumed agent is given again: a role's restriction, the managed workflow, notify.
    wiring: ResumedWiring,
    /// Where the main terminal's recording is kept.
    tddy_data_dir: &'a Path,
}

/// A session whose worktree is gone has nothing for its agent to continue.
fn refuse_a_resume_without_its_worktree(
    session_type: &str,
    worktree_path: &Path,
) -> Result<(), Status> {
    if worktree_path.exists() {
        return Ok(());
    }
    Err(Status::failed_precondition(format!(
        "worktree no longer exists; cannot resume {session_type} session"
    )))
}

/// Resume a CLI harness session: the agent's spec rendered with `resume` (`--continue`, or its
/// recorded resume token) in its worktree, with its provider route re-resolved from the vault as it
/// stands now. Refused when the worktree is gone — there is nothing for `--continue` to continue.
async fn resume_cli_harness_session(
    session_agent: &CliSessionAgent,
    cli_manager: &Arc<CliSessionManager>,
    account_access: &SessionAccountAccess,
    session: ResumedSession<'_>,
) -> Result<Response<ResumeSessionResponse>, Status> {
    let CliSessionAgent { kind, agent, .. } = session_agent;
    let kind = kind.clone();
    let ResumedSession {
        session_id,
        session_dir,
        meta,
        worktree: worktree_path,
        git_environment,
        split,
        os_user,
        wiring,
        tddy_data_dir,
    } = session;
    let SplitLaunchParts {
        args: split_args,
        env: split_env,
        pointer: session_pointer,
    } = split_launch_parts(split, &wiring)?;

    let route = resume_cli_route(account_access, &agent.spec, session_dir)?;
    let (on_exit, token_arm) = resume_token_recorder(
        session_dir.to_path_buf(),
        resume_token_adapter(&agent.spec)?,
    );
    let spec = &agent.spec;
    let conversation = resumed_or_adopted_conversation(
        ConversationStart {
            spec,
            binary: &agent.tool.binary,
            env_base: &agent.env_base,
            worktree: &worktree_path,
            env: &git_environment,
            session_id,
        },
        &meta,
    )
    .await?;
    let remote_host = meta
        .ssh_config_host
        .as_deref()
        .filter(|alias| !alias.is_empty());
    let mut slots = render_host_launch_slots(
        agent,
        &worktree_path,
        session_dir,
        HostLaunchSlots {
            toolset: wiring.toolset,
            system_prompt_file: wiring.system_prompt_file.as_deref(),
            ..HostLaunchSlots::default()
        },
    )?;
    slots.argv_extra.extend(split_args);
    slots.argv_extra.extend(wiring.extra_args);
    slots.argv_extra.extend(route.args);
    write_slot_files_on_target(&mut slots, remote_host)?;
    let host = PointerHost::of(remote_host);
    let pointer = place_session_pointer(session_pointer.as_ref(), &worktree_path, &host)?;
    let on_exit = removing_session_pointer_on_exit(on_exit, pointer, meta.tmux_session.is_some());
    let mut env = git_environment;
    env.extend(split_env);
    env.extend(wiring.env);
    env.extend(route.env);
    let launcher = node_launcher_of(&agent.tool.binary);
    let params = harness_template_params(&conversation.bound(HarnessLaunch {
        model: meta.model.as_deref().unwrap_or_default(),
        resume: true,
        node_launcher: launcher.as_ref(),
        ..HarnessLaunch::default()
    }));
    record_usage_context(
        session_dir,
        &spec.id,
        home_dir_for_user(os_user),
        &UsageSubject::new(
            conversation.usage_id(),
            meta.model.as_deref().unwrap_or_default(),
        ),
        &worktree_path,
    )?;
    let launched = launch(
        cli_manager,
        HarnessSession::new(session_id, &worktree_path, session_dir, os_user),
        HarnessSpecLaunch {
            agent,
            params: &params,
            slots: &slots,
            kind: kind.name,
            model: "",
            env,
            on_exit,
        },
        HarnessRun::resume(meta.tmux_session.is_some()),
    )
    .await?;
    if let Some(workflow) = wiring.workflow {
        cli_manager
            .attach_managed_workflow(session_id, workflow)
            .await;
    }
    crate::claude_cli_spawn::record_main_terminal(cli_manager, tddy_data_dir, session_id).await;

    let mut updated = meta;
    updated.cursor_chat_id = conversation
        .recorded_chat_id()
        .map(str::to_string)
        .or(updated.cursor_chat_id);
    updated.pid = Some(launched.pid);
    updated.tmux_session = launched.tmux_session;
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

/// What a split session adds to a resume's launch.
struct SplitLaunchParts {
    args: Vec<String>,
    env: Vec<(String, String)>,
    pointer: Option<tddy_core::session_pointer::SessionPointer>,
}

/// A split session's extra flags, its env and its pointer: the pointer is the one its wiring was
/// re-derived with; any other session's is the one its own wiring carries. Which one is decided by
/// the session's kind, never by absence: a split session whose wiring carries no pointer is refused.
fn split_launch_parts(
    split: Option<tddy_session_split::split_session::SplitAgentWiring>,
    wiring: &ResumedWiring,
) -> Result<SplitLaunchParts, Status> {
    match split {
        Some(split) => Ok(SplitLaunchParts {
            args: split.extra_args,
            env: split.env,
            pointer: Some(split.session_pointer.ok_or_else(|| {
                Status::internal("a split session's wiring carries no session pointer")
            })?),
        }),
        None => Ok(SplitLaunchParts {
            args: Vec::new(),
            env: Vec::new(),
            pointer: wiring.session_pointer.clone(),
        }),
    }
}
