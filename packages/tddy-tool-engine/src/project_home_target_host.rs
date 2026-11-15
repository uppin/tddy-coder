use crate::project_home_answer_tools::{ANSWER_SESSION_PERMISSION, ANSWER_SESSION_QUESTION};
use crate::project_home_disk_refresh::REFRESH_WORKTREE_DISK_USAGE;
use crate::project_home_git_tools::is_project_home_git_tool;
use crate::project_home_inbox_tools::{READ_PROJECT_NOTIFICATIONS, WAIT_PROJECT_NOTIFICATIONS};
use crate::project_home_lifecycle_tools::{
    CLEAR_WORKTREE, DELETE_SESSION, DELETE_WORKTREE, RENAME_SESSION, RESTORE_WORKTREE,
    SET_SESSION_MARK,
};
use crate::project_home_terminal_tools::{
    CLAIM_SESSION_TERMINAL, READ_SESSION_TERMINAL, SEND_SESSION_KEYS,
};
use crate::project_home_terminate::TERMINATE_SESSION;
use crate::project_home_tools::DIFF_SESSION_WORKTREE;
use crate::project_home_tools::READ_SESSION_WORKTREE;
use crate::project_home_turn_tools::{RESUME_SESSION, SEND_SESSION_MESSAGE};
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
            | SEND_SESSION_MESSAGE
            | RESUME_SESSION
            | SET_SESSION_MARK
            | RENAME_SESSION
            | DELETE_SESSION
            | CLEAR_WORKTREE
            | DELETE_WORKTREE
            | RESTORE_WORKTREE
    ) || is_project_home_git_tool(name)
}

/// The [`TARGET_DAEMON_INSTANCE_ID`] property every target tool's schema carries.
pub(crate) fn target_host_property() -> Value {
    json!({
        "type": "string",
        "description": "The host the session runs on, as ListProjectSessions reports it in its daemon_instance_id; empty for this host."
    })
}

/// Whether `name` acts on a session's checkout: read, diff, restore and the git tools. Named with a
/// split session's agent half — which has no checkout — such a tool acts on its codebase half.
pub fn is_worktree_target_tool(name: &str) -> bool {
    matches!(
        name,
        READ_SESSION_WORKTREE | DIFF_SESSION_WORKTREE | RESTORE_WORKTREE
    ) || is_project_home_git_tool(name)
}

/// `args_json` aimed at `codebase` instead: its `session_id` and [`TARGET_DAEMON_INSTANCE_ID`]
/// replaced, every other argument kept.
pub fn retarget_args(
    args_json: &str,
    codebase: &tddy_worktree_service::session_target::CodebaseHalf,
) -> String {
    let Ok(Value::Object(mut args)) = serde_json::from_str::<Value>(args_json) else {
        return args_json.to_string();
    };
    args.insert(
        "session_id".to_string(),
        Value::String(codebase.session_id.clone()),
    );
    args.insert(
        TARGET_DAEMON_INSTANCE_ID.to_string(),
        Value::String(codebase.daemon_instance_id.clone()),
    );
    Value::Object(args).to_string()
}
