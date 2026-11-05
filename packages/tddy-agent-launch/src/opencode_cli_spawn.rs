//! Start and resume of a CLI coding agent that is cut a worktree of its own — OpenCode
//! (`opencode-cli`), Muse Code (`muse-cli`) and Warp (`warp-cli`) — for the launch topic. The agent is
//! its spec (`tddy-coding-agent-specs`): its argv is the spec's `run:` template, rendered by
//! [`launch_host_pty`] over [`harness_template_params`]; its route is the spec's `route` section; its
//! resume token is read by the spec's `resume` adapter. Nothing here is particular to one of them.
//!
//! The same shape as a Cursor session — a daemon-cut worktree, a PTY relayed over the daemon's own
//! connection — minus what these agents have no counterpart of: no hooks are installed, no chat id is
//! minted (a resume is `--continue` in the worktree, and a session owns its worktree), and the session
//! is never sandboxed here (the jail is `svc_start_sandboxed_cli_harness_session`).
//!
//! A chosen provider account is applied as a route ([`start_cli_route`]): any file it generates
//! (OpenCode's `opencode.json`) lives in the session directory, its key reaches the child's
//! environment only.
//!
//! Feature docs: `docs/ft/daemon/{opencode,muse,warp}-cli-session.md` (`#agent-tools 8/12`).

mod harness_launch;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
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
use tddy_worktree_service::branch_intent::BranchIntentRequest;

use crate::cli_agent_steps::{
    refuse_a_role_the_agent_cannot_hold, render_host_launch_slots, resumed_or_adopted_conversation,
    start_conversation, wire_session_agent, write_slot_files_on_target, ConversationStart,
    HostLaunchSlots, ManagedWiring, ResumedWiring, SessionWiringInputs,
};
use crate::cli_resume_token::{resume_token_recorder, settled_resume_token};
use crate::cli_route_launch::{resume_cli_route, routed_launch_model, start_cli_route};
use crate::cli_session_worktree::{
    provision_cli_worktree, semantic_index_env, CliWorktree, CliWorktreeStart,
};
use crate::cursor_from_spec::node_launcher_of;
use crate::harness_from_spec::{
    harness_template_params, resolve_session_agent, resume_token_adapter, CliSessionAgent,
    CliSessionType, HarnessLaunch,
};
use crate::project_home_launch::StartRole;
use crate::session_acting_identity::SessionAccountAccess;
use crate::spec_launch::HostAgent;
use crate::stack_parent::SpawnStackParent;
use harness_launch::{launch, HarnessRun, HarnessSession, HarnessSpecLaunch};

/// Everything one CLI harness start reads.
pub(crate) struct CliHarnessStart<'a> {
    /// The CLI session type the session runs, with the spec that launches it.
    pub kind: CliSessionType,
    /// The coding agent the session launches, resolved on this host before anything is cut.
    pub agent: &'a HostAgent,
    /// What the daemon's config adds to the agent's environment (its renderer).
    pub renderer_env: Vec<(String, String)>,
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
    /// Run the harness inside a tmux session as the session's OS user
    /// (`StartSessionRequest.run_in_tmux`, `#tmux-agent` 3/3).
    pub run_in_tmux: bool,
    /// `StartSessionRequest.permission_mode`; blank leaves the spec's own default.
    pub permission_mode: &'a str,
    pub dangerously_skip_permissions: bool,
    /// The ssh `Host` alias the checkout lives on; empty for a checkout on this host.
    pub ssh_config_host: &'a str,
    /// Whether the session was started over a managed codebase; recorded on the session.
    pub managed_codebase: bool,
    /// The managed workflow the session is driven by, for a spec declaring `managed_workflow`.
    pub managed: Option<ManagedWiring>,
    /// The session's standing role, recorded before its agent is spawned.
    pub role: StartRole<'a>,
}

// TODO(harness-clis): orphaned from the production launch; only the legacy jail's
// `resolve_cli_jail_binaries` still names it. Removal is deferred: docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md
/// The binary a CLI harness is run as before its spec launched it: its environment variable, then
/// `cli_harnesses.<session type>.binary_path` in the daemon config, then the registry's default. A
/// bare default name is resolved on the host — `~/.local/bin/<name>`, then `$PATH`.
#[allow(dead_code)]
pub(crate) fn harness_binary(
    config: &DaemonConfig,
    harness: &tddy_core::cli_harness::CliHarness,
) -> String {
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

/// Start a CLI harness session: cut its worktree, apply its provider route, spawn the agent from its
/// spec in a PTY and record `.session.yaml`.
pub(crate) async fn start_cli_harness_session(
    start: CliHarnessStart<'_>,
) -> Result<Response<StartSessionResponse>, Status> {
    let CliHarnessStart {
        kind,
        agent,
        renderer_env,
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
        run_in_tmux,
        permission_mode,
        dangerously_skip_permissions,
        ssh_config_host,
        managed_codebase,
        managed,
        role,
    } = start;
    let project_id = project_id.trim();
    let repo_path = repo_path.trim();
    let ssh_alias = ssh_config_host.trim();
    let spec = &agent.spec;
    let role_name = role.name;
    refuse_a_role_the_agent_cannot_hold(spec, role_name)?;

    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;

    // Resolved before anything is cut or spawned: an account that is missing, locked or cannot
    // serve the agent fails the start, and the agent never silently runs on its own setup instead.
    let route = start_cli_route(
        account_access,
        &agent.spec,
        route_sources,
        model,
        &session_dir,
    )?;

    let intent = tddy_session_split::service_util::write_initial_changeset(
        session_id,
        &BranchIntentRequest {
            branch_worktree_intent,
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
        },
        crate::harness_from_spec::cli_branch_intent(kind.clone(), &agent.spec),
        None,
        &session_dir,
        stack_parent.session_id(),
        managed.as_ref().map(|managed| managed.recipe.as_ref()),
    )?;

    let CliWorktree {
        path: worktree_path,
        project,
    } = provision_cli_worktree(
        &CliWorktreeStart {
            session_type: kind.name,
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
            ssh_alias,
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
    let identity =
        account_access.session_identity_in(session_id, os_user, &session_dir, project.as_ref());
    env.extend(identity.git_environment);
    env.extend(renderer_env);
    let recipe_name = managed
        .as_ref()
        .map(|managed| managed.recipe.name().to_string());
    let mut wiring = wire_session_agent(SessionWiringInputs {
        spec,
        config,
        tddy_data_dir,
        os_user,
        session_id,
        session_dir: &session_dir,
        worktree: &worktree_path,
        managed,
        role,
        github_credential_handler: identity.github_credential,
    })?;

    let launch_model = routed_launch_model(
        &agent.spec,
        route.as_ref().map(|route| route.args.as_slice()),
        model,
    );
    let (on_exit, token_arm) =
        resume_token_recorder(session_dir.clone(), resume_token_adapter(&agent.spec)?);
    // The role is on disk before the agent exists, so the daemon never reads a home session as
    // ordinary.
    crate::claude_cli_spawn_steps::record_role_of_session_type_before_spawn(
        &session_dir,
        session_id,
        project_id,
        role_name,
        kind.name,
        &worktree_path,
        managed_codebase,
    )?;
    let conversation = start_conversation(ConversationStart {
        spec,
        binary: &agent.tool.binary,
        worktree: &worktree_path,
        env: &env,
        session_id,
    })
    .await?;
    progress.begin_phase(StartStep::Agent);
    let route = route.unwrap_or_default();
    let remote_host = Some(ssh_alias).filter(|alias| !alias.is_empty());
    let toolset = std::mem::take(&mut wiring.toolset);
    let role_args = std::mem::take(&mut wiring.extra_args);
    let hook_params = wiring.hook_params(spec, session_id, os_user);
    let mut slots = render_host_launch_slots(
        agent,
        &worktree_path,
        &session_dir,
        HostLaunchSlots {
            toolset,
            hooks: hook_params,
            system_prompt_file: wiring.system_prompt_file.as_deref(),
            home: None,
        },
    )?;
    // The role's flags, then the route's arguments, follow the spec's own argv; the route's
    // environment follows the session's.
    slots.argv_extra.extend(role_args);
    slots.argv_extra.extend(route.args);
    write_slot_files_on_target(&mut slots, remote_host)?;
    env.extend(std::mem::take(&mut wiring.env));
    env.extend(route.env);
    let launcher = node_launcher_of(&agent.tool.binary);
    let params = harness_template_params(&conversation.bound(HarnessLaunch {
        model: launch_model,
        prompt: Some(initial_prompt),
        permission_mode: Some(permission_mode),
        dangerously_skip_permissions,
        node_launcher: launcher.as_ref(),
        ..HarnessLaunch::default()
    }));
    let launched = launch(
        cli_manager,
        HarnessSession::new(session_id, &worktree_path, &session_dir, os_user),
        HarnessSpecLaunch {
            agent,
            params: &params,
            slots: &slots,
            kind: kind.name,
            model: launch_model,
            env,
            on_exit,
        },
        HarnessRun::start(run_in_tmux),
    )
    .await?;
    progress.end_phase(StartStep::Agent);
    if let Some(workflow) = wiring.workflow {
        cli_manager
            .attach_managed_workflow(session_id, workflow)
            .await;
    }
    crate::claude_cli_spawn::record_main_terminal(cli_manager, tddy_data_dir, session_id).await;

    let pid = launched.pid;
    let meta = SessionMetadata {
        tmux_session: launched.tmux_session,
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: Some(pid),
        model: Some(model.to_string()).filter(|model| !model.is_empty()),
        agents_rev: tddy_session_agents::agent_roster::started_roster_rev(agents),
        agents: agents.to_vec(),
        hook_token: wiring.hooks.map(|hooks| hooks.hook_token),
        recipe: recipe_name.clone(),
        ssh_config_host: remote_host.map(str::to_string),
        managed_codebase,
        session_role: Some(role_name.to_string()).filter(|role| !role.is_empty()),
        cursor_chat_id: conversation.recorded_chat_id().map(str::to_string),
        ..tddy_session_split::service_util::starting_session_metadata(
            session_id, project_id, kind.name,
        )
    };
    write_session_metadata(&session_dir, &meta)
        .map_err(|e| Status::internal(format!("failed to write session metadata: {e}")))?;
    // Armed only once the metadata is on disk: the recorder adds its token to it.
    token_arm.arm();
    // The roster naming them is on disk now, so the clones belong to the session rather than to the
    // start that claimed them.
    seeded_clones.keep();
    // What the session tells the fleet about itself, published when its terminal is first bridged
    // into LiveKit; the stack association is the load-bearing part (D37).
    let branch = crate::hooks_and_urls::spawned_branch_of_session(
        &session_dir,
        Some(selected_branch_to_work_on)
            .filter(|branch| !branch.is_empty())
            .unwrap_or(new_branch_name),
    );
    cli_manager
        .expose_terminal_to_livekit(
            session_id,
            crate::hooks_and_urls::claude_cli_participant_metadata(
                &crate::hooks_and_urls::StartingClaudeCliSession {
                    agent: &spec.id,
                    session_id,
                    model,
                    recipe: recipe_name.as_deref().unwrap_or_default(),
                    worktree_path: &worktree_path,
                    branch: &branch,
                    stack_parent: &stack_parent,
                },
            ),
        )
        .await;

    log::info!(
        target: "tddy_daemon::connection_service",
        "started {} session {session_id} pid={pid} worktree={} user={os_user}",
        kind.name,
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
        kind: CliSessionType,
        os_user: &str,
        req: &ResumeSessionRequest,
        session_dir: &Path,
        mut metadata: SessionMetadata,
    ) -> Result<Response<ResumeSessionResponse>, Status> {
        if metadata.tmux_session.is_some() {
            // A tmux-run agent that exited unwatched left its token on its kept pane.
            crate::cli_harness_tmux::settle_unwatched_agent_exit(
                &self.claude_cli_manager,
                self.coding_agents.as_deref(),
                os_user,
                &req.session_id,
                kind.name,
                session_dir,
            )
            .await?;
        }
        // The previous run's recorder may still be writing the token it printed as it exited.
        metadata.harness_resume_token =
            settled_resume_token(&self.claude_cli_manager, &req.session_id, session_dir).await?;
        let session_agent =
            resolve_session_agent(self.coding_agents.as_ref(), &self.config, kind.clone())?;
        let identity = self.host.session_identity(
            os_user,
            &req.session_id,
            &metadata.project_id,
            &req.session_token,
        );
        let worktree = metadata
            .repo_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| session_dir.to_path_buf());
        // A worktree that is gone leaves nothing to resume: refused before anything is re-wired.
        refuse_a_resume_without_its_worktree(kind.name, &worktree)?;
        let wiring = self
            .resumed_session_wiring(
                &session_agent.agent.spec,
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
            &self.claude_cli_manager,
            &self.host.session_account_access(&req.session_token),
            ResumedSession {
                session_id: &req.session_id,
                session_dir,
                meta: metadata,
                git_environment: identity.git_environment,
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
    /// The project's account identity pairs the resumed agent's commits carry (empty keeps the
    /// checkout's own).
    git_environment: Vec<(String, String)>,
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
    let CliSessionAgent {
        kind,
        agent,
        renderer_env,
        ..
    } = session_agent;
    let kind = kind.clone();
    let ResumedSession {
        session_id,
        session_dir,
        meta,
        git_environment,
        os_user,
        wiring,
        tddy_data_dir,
    } = session;
    let worktree_path = meta
        .repo_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| session_dir.to_path_buf());
    refuse_a_resume_without_its_worktree(kind.name, &worktree_path)?;

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
    slots.argv_extra.extend(wiring.extra_args);
    slots.argv_extra.extend(route.args);
    write_slot_files_on_target(&mut slots, remote_host)?;
    let mut env = git_environment;
    env.extend(renderer_env.clone());
    env.extend(wiring.env);
    env.extend(route.env);
    let launcher = node_launcher_of(&agent.tool.binary);
    let params = harness_template_params(&conversation.bound(HarnessLaunch {
        model: meta.model.as_deref().unwrap_or_default(),
        resume: true,
        node_launcher: launcher.as_ref(),
        ..HarnessLaunch::default()
    }));
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
