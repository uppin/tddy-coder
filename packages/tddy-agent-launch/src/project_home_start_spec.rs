//! The start request a project's main (project-home session) runs from: the New-session spec
//! `EnsureProjectHomeSession` carries, refused where main could not honour it, and with every field
//! that says where main runs overwritten by the daemon.
//!
//! Location (project, worktree, branch, managed mode, role) is the daemon's by definition of main,
//! so it is overwritten rather than refused. A placement and a role are refused: dropping them
//! silently would start something other than what the operator asked for.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home session.

use std::path::Path;

use tddy_core::session_metadata::PROJECT_HOME_ROLE;
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;

/// The one session type that can enforce managed-codebase mode, which is what makes main's
/// read-only guarantee real. Cursor CLI has no tool allowlist to hold it to
/// (`docs/dev/todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`); the other CLI
/// harnesses and tool sessions have no managed mode at all.
const READ_ONLY_SESSION_TYPE: &str = "claude-cli";

/// Refuses a spec asking for what main cannot be given: a standing role (granted by the daemon,
/// never requested — see `LaunchSessions::start_session_core`) or a placement other than the home
/// worktree on this host (a sandbox, another codebase host, an SSH exec target, an agent clone or
/// a split agent).
pub(crate) fn refuse_what_main_cannot_honour(spec: &StartSessionRequest) -> Result<(), Status> {
    if !spec.session_role.trim().is_empty() {
        return Err(Status::permission_denied(format!(
            "a session's role is granted by the daemon, not requested: main's start spec may not \
             set session_role (got {:?})",
            spec.session_role
        )));
    }
    // TODO(claude-jail): `launch_jailed` carries `session_role`; drop `sandbox` from the refused
    // placements once the sandboxed claude-cli start calls it
    // (docs/dev/todo/2026-10-24-sandboxed-main-needs-session-role.md).
    let placement = [
        (spec.sandbox, "sandbox"),
        (spec.sandboxed_codebase, "sandboxed_codebase"),
        (
            !spec.codebase_daemon_instance_id.trim().is_empty(),
            "codebase_daemon_instance_id",
        ),
        // An SSH exec target runs the agent on a host that does not hold the home worktree.
        (!spec.ssh_config_host.trim().is_empty(), "ssh_config_host"),
        (spec.agent_clone.is_some(), "agent_clone"),
        (spec.split_agent.is_some(), "split_agent"),
    ];
    if let Some((_, field)) = placement.iter().find(|(set, _)| *set) {
        return Err(Status::invalid_argument(format!(
            "main runs on its project's home worktree on this host: its start spec may not set \
             {field}"
        )));
    }
    Ok(())
}

/// Refuses a start whose session type cannot be held read-only. `req` is the start after its
/// harness profile was applied, so a profile cannot swap the type in after this check.
pub(crate) fn refuse_a_type_main_cannot_run(req: &StartSessionRequest) -> Result<(), Status> {
    let session_type = req.session_type.trim();
    if session_type == READ_ONLY_SESSION_TYPE {
        return Ok(());
    }
    let named = if session_type.is_empty() {
        "tool"
    } else {
        session_type
    };
    Err(Status::failed_precondition(format!(
        "a {named} session cannot enforce managed-codebase mode, so main could not be kept \
         read-only; only {READ_ONLY_SESSION_TYPE} can run main"
    )))
}

/// `spec` placed as `project_id`'s main on `home_worktree`: managed, with the project-home role,
/// on no branch and on this host. Everything else the spec chose (model, account, agent,
/// permission mode, initial prompt, specialized agents, semantic index, …) is kept.
pub(crate) fn on_home_worktree(
    spec: StartSessionRequest,
    project_id: &str,
    home_worktree: &Path,
) -> StartSessionRequest {
    StartSessionRequest {
        project_id: project_id.to_string(),
        repo_path: home_worktree.to_string_lossy().into_owned(),
        managed_codebase: true,
        session_role: PROJECT_HOME_ROLE.to_string(),
        daemon_instance_id: String::new(),
        branch_worktree_intent: String::new(),
        selected_branch_to_work_on: String::new(),
        new_branch_name: String::new(),
        selected_integration_base_ref: String::new(),
        create_remote_branch: false,
        on_branch_conflict: String::new(),
        stack_parent: String::new(),
        stack_parent_daemon_instance_id: String::new(),
        stack_node_id: String::new(),
        pr_stack_base_session_id: String::new(),
        ..spec
    }
}
