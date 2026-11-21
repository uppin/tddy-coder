//! The smaller steps of a CLI harness start (split out of `opencode_cli_spawn.rs`): pointing a
//! target's hooks at its log, placing the slot files and the session pointer (shared with the
//! resume), the node launcher, and the start's answer.
//!
//! TODO(restructure-path-attr): moved here by hand, without running `tddy-tools restructure` on
//! this move — on the inference that the refusal it gave `remote_hook_follower.rs`'s split in this
//! crate (`#[path]` test modules, "plan is malformed") would hit it too. See
//! `docs/dev/todo/2026-11-21-restructure-ignores-path-attribute-modules.md`.

use std::sync::Arc;

use tddy_rpc::{Response, Status};
use tddy_service::proto::session::StartSessionResponse;

use crate::cli_agent_steps::host_launch_slots::write_slot_files_on_target;
use crate::cli_agent_steps::host_launch_slots::write_slot_files_through_shell;
use crate::cli_session_pointer::{place_session_pointer, PointerHost};
use crate::cursor_from_spec::node_launcher_of;

pub(super) fn log_hooks_on_the_target(
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
pub(super) async fn place_slot_files_and_pointer(
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

pub(super) fn node_launcher_for(
    agent: &crate::spec_launch::HostAgent,
    target_shell: &Option<Arc<dyn tddy_remote_shell::RemoteShell>>,
) -> Option<crate::cursor_from_spec::NodeLauncher> {
    match target_shell {
        Some(_) => None,
        None => node_launcher_of(&agent.tool.binary),
    }
}

pub(super) fn report_the_started_session(
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
