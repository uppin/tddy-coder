use tddy_core::write_session_metadata;

use crate::cursor_cli_spawn::{chat, no_coding_agents, resolve_cursor_agent, LaunchedCursor};
use crate::cursor_from_spec::node_launcher_of;
use crate::harness_from_spec::{harness_template_params, HarnessLaunch};
use crate::spec_launch::{
    launch_host_pty, plan_host_pty, HostAgent, HostCodingAgents, HostPtySession,
};
use tddy_actions::tool::TemplateParams;
use tddy_coding_agent_specs::slots::RenderedSlots;

use std::path::PathBuf;

use tddy_rpc::Status;

use tddy_service::proto::session::ResumeSessionResponse;

use tddy_rpc::Response;

use tddy_core::SessionMetadata;

use std::path::Path;

use tddy_daemon_kernel::config::DaemonConfig;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;

use std::sync::Arc;

// TODO(cursor-cli): bundle the resume args (sweep)
#[allow(clippy::too_many_arguments)]
pub async fn resume_cursor_cli_session(
    cli_manager: &Arc<CliSessionManager>,
    config: &DaemonConfig,
    // The session's OS user: a tmux-run cursor resumes as this user.
    os_user: &str,
    session_id: &str,
    session_dir: &Path,
    meta: SessionMetadata,
    // What the resumed agent's environment is given: the project's account identity pairs its
    // commits carry (none keeps the checkout's own) and its notify environment.
    git_environment: Vec<(String, String)>,
    // The coding agents this host launches from: the resume relaunches the `cursor` spec into the
    // recorded chat. A host without them refuses.
    coding_agents: Option<&HostCodingAgents>,
) -> Result<Response<ResumeSessionResponse>, Status> {
    let agents = coding_agents.ok_or_else(no_coding_agents)?;
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

    let agent = resolve_cursor_agent(agents, config)?;
    let binary_path = agent.tool.binary.to_string_lossy().into_owned();
    let chat_id = chat_to_resume(&meta, session_id, &binary_path, &worktree_path).await?;

    let launcher = node_launcher_of(&agent.tool.binary);
    let params = harness_template_params(&HarnessLaunch {
        model: &model,
        chat_id: Some(&chat_id),
        node_launcher: launcher.as_ref(),
        ..HarnessLaunch::default()
    });
    let launched = relaunch_cursor(
        cli_manager,
        &agent,
        &params,
        CursorRelaunch {
            os_user,
            session_id,
            session_dir,
            worktree_path: &worktree_path,
            model: &model,
            env: git_environment,
            in_tmux: meta.tmux_session.is_some(),
        },
    )
    .await?;
    let (pid, mut updated) = (launched.pid, meta);
    updated.tmux_session = launched.tmux_session;
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

/// What a resumed cursor-cli agent is relaunched with.
struct CursorRelaunch<'a> {
    os_user: &'a str,
    session_id: &'a str,
    session_dir: &'a Path,
    worktree_path: &'a Path,
    model: &'a str,
    env: Vec<(String, String)>,
    /// The session ran its agent in tmux.
    in_tmux: bool,
}

/// Relaunch a resumed cursor-cli agent from the `cursor` spec (`params` resume the recorded chat):
/// a tmux-run one is re-attached while its tmux session is alive, else started fresh, as the
/// session's OS user (`#tmux-agent` 3/3); any other is the PTY child again. A resume keeps the
/// hooks file its start wrote, and registers no MCP server on the host.
async fn relaunch_cursor(
    cli_manager: &Arc<CliSessionManager>,
    agent: &HostAgent,
    params: &TemplateParams,
    relaunch: CursorRelaunch<'_>,
) -> Result<LaunchedCursor, Status> {
    let failed = |e: crate::spec_launch::SpecLaunchError| {
        Status::internal(format!("failed to resume cursor-cli: {e}"))
    };
    if relaunch.in_tmux {
        let plan = plan_host_pty(agent, params, &RenderedSlots::default()).map_err(failed)?;
        let launch = crate::cli_harness_tmux::HarnessTmuxLaunch::cursor(
            relaunch.os_user,
            relaunch.session_id,
            relaunch.worktree_path,
            relaunch.session_dir,
            plan.argv,
            plan.env.into_iter().chain(relaunch.env).collect(),
        );
        let launched = crate::cli_harness_tmux::resume_harness_in_tmux(cli_manager, launch).await?;
        return Ok(LaunchedCursor::in_tmux(launched));
    }
    let handle = launch_host_pty(
        agent,
        params,
        &RenderedSlots::default(),
        relaunch.worktree_path,
        HostPtySession {
            manager: cli_manager,
            session_id: relaunch.session_id,
            session_type: "cursor-cli",
            model: relaunch.model,
            env: relaunch.env,
            os_user: None,
        },
        None,
    )
    .await
    .map_err(failed)?;
    Ok(LaunchedCursor {
        pid: handle.pid,
        tmux_session: None,
    })
}

/// The chat a resume reattaches to. A session started before chat ids were recorded has no way back
/// to its original chat, so its first resume adopts a fresh one, which the resume pins — every later
/// resume then reattaches instead of starting over again.
async fn chat_to_resume(
    meta: &SessionMetadata,
    session_id: &str,
    binary_path: &str,
    worktree_path: &Path,
) -> Result<String, Status> {
    if let Some(id) = meta
        .cursor_chat_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return Ok(id.to_string());
    }
    let adopted = chat::mint_cursor_chat_id(binary_path, worktree_path)
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
    Ok(adopted)
}
