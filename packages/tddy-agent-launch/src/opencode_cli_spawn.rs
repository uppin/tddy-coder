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
pub mod restore_tabs;
mod resume;
pub(crate) mod session_start;

use std::sync::Arc;

use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::user_paths::home_dir_for_user;
use tddy_rpc::{Response, Status};
use tddy_service::proto::session::start_phase::Step as StartStep;
use tddy_service::proto::session::StartSessionResponse;
use tddy_worktree_service::branch_intent::BranchIntentRequest;

use crate::cli_agent_steps::conversation::refuse_a_role_the_agent_cannot_hold;
use crate::cli_agent_steps::conversation::start_conversation;
use crate::cli_agent_steps::conversation::ConversationStart;
use crate::cli_agent_steps::conversation::ConversationTarget;
use crate::cli_agent_steps::host_launch_slots::render_host_launch_slots;
use crate::cli_agent_steps::host_launch_slots::write_slot_files_on_target;
use crate::cli_agent_steps::host_launch_slots::write_slot_files_through_shell;
use crate::cli_agent_steps::host_launch_slots::HostLaunchSlots;
use crate::cli_agent_steps::{wire_session_agent, SessionWiringInputs};
use crate::cli_resume_token::resume_token_recorder;
use crate::cli_route_launch::{routed_launch_model, start_cli_route};
use crate::cli_session_pointer::{
    place_session_pointer, removing_session_pointer_on_exit, PointerHost,
};
use crate::cli_session_worktree::{
    provision_cli_worktree, semantic_index_env, CliWorktree, CliWorktreeStart,
};
use crate::cursor_from_spec::node_launcher_of;
use crate::effort_launch::effort_launch;
use crate::harness_from_spec::{harness_template_params, resume_token_adapter, HarnessLaunch};
use crate::opencode_cli_spawn::session_start::shell_for_session_target;
use crate::opencode_cli_spawn::session_start::CliSessionStart;
use crate::usage_context::{record_usage_context, UsageSubject};
use harness_launch::{launch, HarnessRemote, HarnessRun, HarnessSession, HarnessSpecLaunch};

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
        orchestrator,
        reports_to,
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
        effort,
        ssh_config_host,
        target,
        subject,
        coder_credentials,
        hook_delivery,
        hook_followers,
        managed_codebase,
        managed,
        role,
        session_tool_token,
    } = start;
    let project_id = project_id.trim();
    let repo_path = repo_path.trim();
    let ssh_alias = ssh_config_host.trim();
    // The checkout's host: the target the agent runs on, or the ssh exec host. The two are
    // mutually exclusive (refused at the start), so one alias names where the worktree is cut,
    // the slot files are written and the pointer is placed.
    let checkout_alias =
        crate::remote_agent_launch::target_config_host(target.as_ref()).unwrap_or(ssh_alias);
    // Resolved before anything is cut: a Coder target whose vault is locked, or whose account
    // the vault does not hold, refuses the start here rather than after a worktree exists.
    let target_shell = match &target {
        Some(target) => Some(shell_for_session_target(
            target,
            &subject,
            coder_credentials.as_ref(),
        )?),
        None => None,
    };
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
        orchestrator
            .or_else(|| stack_parent.as_orchestrator_link())
            .as_ref(),
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
            ssh_alias: checkout_alias,
            shell: target_shell.clone(),
        },
        &stack_parent,
        progress,
    )
    .await?;

    // The start's later calls on its target (the conversation, the slot files, the pointer, the
    // trust write, the launch) are listed on its record as the worktree step's are.
    let target_shell = target_shell.map(|shell| -> Arc<dyn tddy_remote_shell::RemoteShell> {
        Arc::new(crate::cli_session_worktree::recorded_by(shell, progress))
    });

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
        // A session whose agent runs on another host is exported none: the URL and the socket
        // name this daemon's host, which that shell cannot reach — detection would prefer the
        // dead route over the remote one.
        session_tool_token: session_tool_token.filter(|_| checkout_alias.is_empty()),
        reports_to: reports_to.as_ref(),
        on_target: target.is_some(),
    })?;

    // On a target the agent's hooks append to its hook log there, not to a daemon it cannot
    // reach: the launch creates that log and follows it.
    log_hooks_on_the_target(&target, &mut wiring);
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
        target: target_shell.as_ref().map(|shell| ConversationTarget {
            shell,
            command: &agent.tool.command,
        }),
    })
    .await?;
    progress.begin_phase(StartStep::Agent);
    let route = route.unwrap_or_default();
    let remote_host = Some(checkout_alias).filter(|alias| !alias.is_empty());
    let toolset = std::mem::take(&mut wiring.toolset);
    let role_args = std::mem::take(&mut wiring.extra_args);
    let first_prompt = crate::back_channel::first_prompt_with(
        wiring.first_prompt_guidance.as_deref(),
        initial_prompt,
    );
    let hook_params = wiring.hook_params(spec, session_id, os_user, &route.plain_env);
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
    let carried_effort = effort_launch(spec, effort);
    slots.argv_extra.extend(role_args);
    // An orchestrator and its children on this host message each other by peer name.
    if crate::back_channel::launches_under_peer_name(
        target.is_some(),
        reports_to.is_some(),
        role_name,
    ) {
        slots
            .argv_extra
            .extend(crate::back_channel::peer_name_args(spec, session_id));
    }
    slots.argv_extra.extend(carried_effort.args);
    slots.argv_extra.extend(route.args);
    let pointer = place_slot_files_and_pointer(
        spec,
        &worktree_path,
        &target_shell,
        wiring.session_pointer.as_ref(),
        remote_host,
        &mut slots,
    )
    .await?;
    let on_exit = removing_session_pointer_on_exit(on_exit, pointer, run_in_tmux);
    env.extend(std::mem::take(&mut wiring.env));
    env.extend(carried_effort.env);
    env.extend(route.env);
    // The node launcher is a path of this host: a target runs its own agent, found by its name.
    let launcher = node_launcher_for(agent, &target_shell);
    let params = harness_template_params(&conversation.bound(HarnessLaunch {
        model: launch_model,
        prompt: Some(&first_prompt),
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
            remote: target_shell.clone().map(|shell| HarnessRemote {
                shell,
                pointer: wiring.session_pointer.clone(),
                startup_window: config.spawn_startup_grace_period(),
                start_timeout: config.remote_agent_start_timeout(),
                hook_token: wiring.hooks.as_ref().map(|hooks| hooks.hook_token.clone()),
                hook_delivery: hook_delivery.clone(),
                hook_followers: hook_followers.clone(),
            }),
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
        // The agent runs: the session reads `Started` from its own first record, without
        // waiting for a hook. A Claude start's `SessionStart` hook re-records the same a
        // moment later; a Cursor start's never fires (every Cursor launch resumes its chat,
        // and a resumed launch sends no `sessionStart`), so without this the session would
        // read no status at all until its first prompt's `beforeSubmitPrompt` (`Running`).
        activity_status: Some(
            tddy_core::SessionActivityStatus::Started
                .as_wire()
                .to_string(),
        ),
        tmux_session: launched.tmux_session,
        repo_path: Some(worktree_path.to_string_lossy().to_string()),
        pid: Some(pid),
        model: Some(model.to_string()).filter(|model| !model.is_empty()),
        agents_rev: tddy_session_agents::agent_roster::started_roster_rev(agents),
        agents: agents.to_vec(),
        hook_token: wiring.hooks.map(|hooks| hooks.hook_token),
        recipe: recipe_name.clone(),
        ssh_config_host: Some(ssh_alias)
            .filter(|alias| !alias.is_empty())
            .map(str::to_string),
        target: target.clone(),
        managed_codebase,
        session_role: Some(role_name.to_string()).filter(|role| !role.is_empty()),
        effort: Some(effort.trim().to_string()).filter(|effort| !effort.is_empty()),
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

    report_the_started_session(kind, os_user, session_id, worktree_path, pid)
}

fn log_hooks_on_the_target(
    target: &Option<tddy_core::SessionTarget>,
    wiring: &mut crate::cli_agent_steps::SessionWiring,
) {
    if target.is_some() {
        if let Some(hooks) = &mut wiring.hooks {
            // Relative to the worktree the agent runs in: the hook appends there, on the
            // target, wherever that worktree is.
            hooks.remote_log = Some(tddy_core::REMOTE_HOOK_LOG.to_string());
        }
    }
}

/// Write a launch's slot files and its session pointer where its worktree is — this host, an ssh
/// alias, or the target's shell — once the worktree is ready for the session's own hooks file.
/// Shared by the start and the resume.
async fn place_slot_files_and_pointer(
    spec: &tddy_coding_agent_specs::CodingAgentSpec,
    worktree_path: &std::path::Path,
    target_shell: &Option<Arc<dyn tddy_remote_shell::RemoteShell>>,
    session_pointer: Option<&tddy_core::session_pointer::SessionPointer>,
    remote_host: Option<&str>,
    slots: &mut tddy_coding_agent_specs::slots::RenderedSlots,
) -> Result<Option<crate::cli_session_pointer::PlacedSessionPointer>, Status> {
    let host = match target_shell {
        Some(shell) => PointerHost::Remote {
            shell: Arc::clone(shell),
        },
        None => PointerHost::of(remote_host),
    };
    crate::session_hooks_worktree::prepare_worktree_for_session_hooks(
        spec,
        worktree_path,
        slots,
        &host,
    )
    .await?;
    match target_shell {
        Some(shell) => write_slot_files_through_shell(slots, shell.as_ref()).await?,
        None => write_slot_files_on_target(slots, remote_host).await?,
    }
    place_session_pointer(session_pointer, worktree_path, &host).await
}

fn node_launcher_for(
    agent: &crate::spec_launch::HostAgent,
    target_shell: &Option<Arc<dyn tddy_remote_shell::RemoteShell>>,
) -> Option<crate::cursor_from_spec::NodeLauncher> {
    match target_shell {
        Some(_) => None,
        None => node_launcher_of(&agent.tool.binary),
    }
}

fn report_the_started_session(
    kind: crate::harness_from_spec::CliSessionType,
    os_user: &str,
    session_id: &str,
    worktree_path: std::path::PathBuf,
    pid: u32,
) -> Result<Response<StartSessionResponse>, Status> {
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
