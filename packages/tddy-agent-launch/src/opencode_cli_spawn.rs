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
mod resume;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::provider_route::RouteSources;
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::user_paths::home_dir_for_user;
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_service::proto::session::StartSessionResponse;
use tddy_session_files::attachment_progress::AttachmentProgressSink;
use tddy_worktree_service::branch_intent::BranchIntentRequest;

use crate::cli_agent_steps::conversation::refuse_a_role_the_agent_cannot_hold;
use crate::cli_agent_steps::conversation::start_conversation;
use crate::cli_agent_steps::conversation::ConversationStart;
use crate::cli_agent_steps::{
    render_host_launch_slots, wire_session_agent, write_slot_files_on_target, HostLaunchSlots,
    ManagedWiring, SessionWiringInputs,
};
use crate::cli_resume_token::resume_token_recorder;
use crate::cli_route_launch::{routed_launch_model, start_cli_route};
use crate::cli_session_pointer::{
    place_session_pointer, removing_session_pointer_on_exit, PointerHost,
};
use crate::cli_session_worktree::{
    provision_cli_worktree, semantic_index_env, CliWorktree, CliWorktreeStart,
};
use crate::cursor_from_spec::node_launcher_of;
use crate::harness_from_spec::{
    harness_template_params, resume_token_adapter, CliSessionType, HarnessLaunch,
};
use crate::project_home_launch::StartRole;
use crate::session_acting_identity::SessionAccountAccess;
use crate::spec_launch::HostAgent;
use crate::stack_parent::SpawnStackParent;
use crate::usage_context::{record_usage_context, UsageSubject};
use harness_launch::{launch, HarnessRun, HarnessSession, HarnessSpecLaunch};

/// Everything one CLI harness start reads.
pub(crate) struct CliSessionStart<'a> {
    /// The CLI session type the session runs, with the spec that launches it.
    pub kind: CliSessionType,
    /// The coding agent the session launches, resolved on this host before anything is cut.
    pub agent: &'a HostAgent,
    pub config: &'a DaemonConfig,
    pub tddy_data_dir: &'a Path,
    /// The session's OS user's agent-tool socket, resolved before anything is cut
    /// ([`LaunchSessions::agent_tool_socket_of`]).
    pub agent_tool_socket: &'a Path,
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

pub(crate) async fn start_cli_harness_session(
    start: CliSessionStart<'_>,
) -> Result<Response<StartSessionResponse>, Status> {
    let CliSessionStart {
        kind,
        agent,
        config,
        tddy_data_dir,
        agent_tool_socket,
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
    let recipe_name = managed
        .as_ref()
        .map(|managed| managed.recipe.name().to_string());
    let mut wiring = wire_session_agent(SessionWiringInputs {
        spec,
        tddy_tools_path: &crate::cli_harness_jail::agent_tddy_tools(agent)?,
        config,
        tddy_data_dir,
        agent_tool_socket,
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
        env_base: &agent.env_base,
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
    let host = PointerHost::of(remote_host);
    let pointer = place_session_pointer(wiring.session_pointer.as_ref(), &worktree_path, &host)?;
    let on_exit = removing_session_pointer_on_exit(on_exit, pointer, run_in_tmux);
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
    record_usage_context(
        &session_dir,
        &spec.id,
        home_dir_for_user(os_user),
        &UsageSubject::new(conversation.usage_id(), model),
        &worktree_path,
    )?;
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
