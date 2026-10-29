use crate::project_home_answer_tools::{ANSWER_SESSION_PERMISSION, ANSWER_SESSION_QUESTION};
use crate::project_home_disk_refresh::REFRESH_WORKTREE_DISK_USAGE;
use crate::project_home_git_tools::is_project_home_git_tool;
use crate::project_home_inbox_tools::{READ_PROJECT_NOTIFICATIONS, WAIT_PROJECT_NOTIFICATIONS};
use crate::project_home_terminal_tools::{
    CLAIM_SESSION_TERMINAL, READ_SESSION_TERMINAL, SEND_SESSION_KEYS,
};
use crate::project_home_terminate::TERMINATE_SESSION;
use crate::project_home_tools::DIFF_SESSION_WORKTREE;
use crate::project_home_tools::READ_SESSION_WORKTREE;
use crate::project_home_worktree_tools::{GET_WORKTREE, LIST_WORKTREES};
use crate::session_follow::{READ_SESSION_ACTIVITY, READ_SESSION_TRANSCRIPT};
use crate::start_options::DESCRIBE_SESSION_START_OPTIONS;
use serde_json::{json, Value};

/// The argument every project-home target tool takes to name the host its target runs on. Empty or
/// absent is this host.
pub const TARGET_DAEMON_INSTANCE_ID: &str = "daemon_instance_id";

/// The host `args_json` names in [`TARGET_DAEMON_INSTANCE_ID`], or `None` for this host (the
/// argument absent or empty, or `args_json` not an object).
pub fn target_daemon_instance_id(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let host = args.get(TARGET_DAEMON_INSTANCE_ID)?.as_str()?.trim();
    (!host.is_empty()).then(|| host.to_string())
}

/// Whether `name` is a project-home tool that acts on a host, and so takes
/// [`TARGET_DAEMON_INSTANCE_ID`]: read, diff, the two worktree tools, the seven git tools, the
/// terminal tools (`#home-inbox` 2/8, 5/8), `DescribeSessionStartOptions`, which describes the host
/// it names (`#start-parity` 5/9), and the two notification tools, which read the host they name
/// (`#home-inbox` 7/8).
pub fn is_project_home_target_tool(name: &str) -> bool {
    matches!(
        name,
        READ_SESSION_WORKTREE
            | DIFF_SESSION_WORKTREE
            | LIST_WORKTREES
            | GET_WORKTREE
            | DESCRIBE_SESSION_START_OPTIONS
            | READ_SESSION_ACTIVITY
            | READ_SESSION_TRANSCRIPT
            | TERMINATE_SESSION
            | REFRESH_WORKTREE_DISK_USAGE
            | READ_SESSION_TERMINAL
            | CLAIM_SESSION_TERMINAL
            | SEND_SESSION_KEYS
            | ANSWER_SESSION_PERMISSION
            | ANSWER_SESSION_QUESTION
            | WAIT_PROJECT_NOTIFICATIONS
            | READ_PROJECT_NOTIFICATIONS
    ) || is_project_home_git_tool(name)
}

/// The [`TARGET_DAEMON_INSTANCE_ID`] property every target tool's schema carries.
pub(crate) fn target_host_property() -> Value {
    json!({
        "type": "string",
        "description": "The host the session runs on, as ListProjectSessions reports it in its daemon_instance_id; empty for this host."
    })
}
