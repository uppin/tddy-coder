use crate::catalog::{tool_catalog, ToolDef};
use crate::project_home_answer_tools::{
    project_home_answer_tool_defs, ANSWER_SESSION_PERMISSION, ANSWER_SESSION_QUESTION,
};
use crate::project_home_disk_refresh::{
    refresh_worktree_disk_usage_tool_def, REFRESH_WORKTREE_DISK_USAGE,
};
use crate::project_home_git_tools::project_home_git_tool_defs;
use crate::project_home_inbox_tools::{
    project_home_inbox_tool_defs, READ_PROJECT_NOTIFICATIONS, WAIT_PROJECT_NOTIFICATIONS,
};
use crate::project_home_notify_tools::{notify_operator_tool_def, NOTIFY_OPERATOR};
use crate::project_home_terminal_tools::{
    project_home_terminal_tool_defs, CLAIM_SESSION_TERMINAL, READ_SESSION_TERMINAL,
    SEND_SESSION_KEYS,
};
use crate::project_home_terminate::{terminate_session_tool_def, TERMINATE_SESSION};
use crate::project_home_tools::DIFF_SESSION_WORKTREE;
use crate::project_home_tools::LIST_PROJECT_SESSIONS;
use crate::project_home_tools::READ_SESSION_WORKTREE;
use crate::project_home_worktree_tools::{worktree_tool_defs, GET_WORKTREE, LIST_WORKTREES};
use crate::read_only::is_project_home_allowed_tool;
use crate::session_follow::{
    session_follow_tool_defs, READ_SESSION_ACTIVITY, READ_SESSION_TRANSCRIPT,
};
use crate::start_ledger::{CANCEL_SESSION_START, GET_SESSION_START};
use crate::start_options::{start_options_tool_def, DESCRIBE_SESSION_START_OPTIONS};
use crate::start_session_args::{
    start_session_schema, start_session_schema_without_choices, StartChoices, START_SESSION,
};
use serde_json::{json, Value};

use crate::project_home_target_host::target_host_property;
use crate::project_home_target_host::TARGET_DAEMON_INSTANCE_ID;

/// Whether `name` is one of the project-home tools.
pub fn is_project_home_tool(name: &str) -> bool {
    matches!(
        name,
        LIST_PROJECT_SESSIONS
            | START_SESSION
            | GET_SESSION_START
            | CANCEL_SESSION_START
            | DESCRIBE_SESSION_START_OPTIONS
            | READ_SESSION_WORKTREE
            | DIFF_SESSION_WORKTREE
            | LIST_WORKTREES
            | GET_WORKTREE
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
            | NOTIFY_OPERATOR
    )
}

/// The exec tools a project-home session's agent is offered: exactly the tools
/// [`is_project_home_allowed_tool`] lets it run, in catalog order, then the four project-home tools,
/// then the seven project-home git tools (`#project-home` 7/7).
/// The same predicate the daemon's role guard applies at call time, so what is advertised and what
/// is executed cannot disagree.
///
/// `StartSession`'s schema here names no host or recipe choices
/// ([`start_session_schema_without_choices`]); [`session_offered_catalog`] is the daemon's answer,
/// with them.
pub fn project_home_offered_catalog() -> Vec<ToolDef> {
    project_home_catalog_with(start_session_schema_without_choices())
}

/// The exec tools a session's agent is offered, by whether it is a project-home session: the
/// project-home catalog for one, with `StartSession`'s schema offering `choices` — the daemon's
/// eligible hosts and supported recipes — and the daemon-wide catalog for any other. The one place
/// that answer is decided, for every host that answers `ListExecTools` (the token route and the
/// jail's bridge).
pub fn session_offered_catalog(is_project_home: bool, choices: &StartChoices) -> Vec<ToolDef> {
    if is_project_home {
        project_home_catalog_with(start_session_schema(choices))
    } else {
        tool_catalog()
    }
}

/// What a session's role entitles its agent to among the project-home tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionToolRole {
    /// The project-home session: every project-home tool, and nothing else that writes.
    ProjectHome,
    /// A session that can spawn conversations (grill-me): the daemon-wide catalog, the four start
    /// tools (`#start-parity` 9/9) and the two answer tools (`#home-inbox` 6/8, 8/9), for its own
    /// project alone.
    ConversationSpawner,
    /// Any other session: the daemon-wide catalog.
    Ordinary,
}

/// Whether `name` is one of the six tools a [`SessionToolRole::ConversationSpawner`] is offered and
/// served: the four start tools — `StartSession`, `GetSessionStart`, `CancelSessionStart` and
/// `DescribeSessionStartOptions` — and the two answer tools, `AnswerSessionPermission` and
/// `AnswerSessionQuestion`, the answers to a prompt or a question of a session it started
/// (`#home-inbox` 6/8, 8/9).
pub fn is_conversation_spawner_tool(name: &str) -> bool {
    matches!(
        name,
        START_SESSION
            | GET_SESSION_START
            | CANCEL_SESSION_START
            | DESCRIBE_SESSION_START_OPTIONS
            | ANSWER_SESSION_PERMISSION
            | ANSWER_SESSION_QUESTION
    )
}

/// Whether `name` is a tool a [`SessionToolRole::ConversationSpawner`] is offered for the sessions
/// it started alone — `ReadSessionActivity`, so it can read the `request_id` its two answer tools
/// need (`#home-inbox` 11/11). The daemon serves such a call only when the target's changeset names
/// the caller as its `orchestrator_session_id`, and refuses any other as permission denied.
pub fn is_conversation_spawner_scoped_tool(name: &str) -> bool {
    name == READ_SESSION_ACTIVITY
}

/// [`session_offered_catalog`] by [`SessionToolRole`]: a conversation spawner is offered the
/// daemon-wide catalog and [`is_conversation_spawner_tool`]'s six, `StartSession` offering
/// `choices`.
pub fn session_offered_catalog_for(role: SessionToolRole, choices: &StartChoices) -> Vec<ToolDef> {
    match role {
        SessionToolRole::ProjectHome => session_offered_catalog(true, choices),
        SessionToolRole::Ordinary => session_offered_catalog(false, choices),
        SessionToolRole::ConversationSpawner => tool_catalog()
            .into_iter()
            .chain(
                project_home_tool_defs_with(start_session_schema(choices))
                    .into_iter()
                    .filter(|tool| {
                        is_conversation_spawner_tool(&tool.name)
                            || is_conversation_spawner_scoped_tool(&tool.name)
                    }),
            )
            .collect(),
    }
}

pub(crate) fn project_home_catalog_with(start_schema: Value) -> Vec<ToolDef> {
    tool_catalog()
        .into_iter()
        .filter(|tool| is_project_home_allowed_tool(&tool.name))
        .chain(project_home_tool_defs_with(start_schema))
        .chain(project_home_git_tool_defs())
        .collect()
}

/// The project-home tools' definitions, as a client registers them before the daemon is asked:
/// `StartSession`'s schema names no host or recipe choices
/// ([`start_session_schema_without_choices`]).
pub fn project_home_tool_defs() -> Vec<ToolDef> {
    project_home_tool_defs_with(start_session_schema_without_choices())
}

pub(crate) fn project_home_tool_defs_with(start_schema: Value) -> Vec<ToolDef> {
    let mut defs = vec![
        tool_def(
            LIST_PROJECT_SESSIONS,
            "List the other sessions of this project on every host, with their host, worktree, activity and status. A host that does not answer is named in unreachable_hosts.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        tool_def(
            START_SESSION,
            "Start a session in this project, with any option the New-session form offers, on this host or another eligible one. It is recorded as started by this project-home session on the host that holds it. It returns at once with start_id, task_id and daemon_instance_id: poll the start with GetSessionStart, stop it with CancelSessionStart. A start the form would refuse outright (a CLI session without a model, a branch another session owns) is refused now, naming why. A start whose session type emits no progress (tool, sandboxed and stack-child starts) reads running until its outcome.",
            start_schema,
        ),
        tool_def(
            GET_SESSION_START,
            "Read a start by start_id: its state (running, succeeded, failed or cancelled), the step it is in and that step's status, the clone's progress, and once it succeeded its session_id, branch and worktree, or once it failed or was cancelled, why.",
            json!({
                "type": "object",
                "properties": { "start_id": { "type": "string" } },
                "required": ["start_id"]
            }),
        ),
        tool_def(
            CANCEL_SESSION_START,
            "Cancel a start by start_id while it is running, on whichever host runs it. It returns at once; the start reads cancelled once it has stopped, and a session it had already created is removed.",
            json!({
                "type": "object",
                "properties": { "start_id": { "type": "string" } },
                "required": ["start_id"]
            }),
        ),
        tool_def(
            READ_SESSION_WORKTREE,
            "Read a file, or list a directory, in another session's worktree of this project.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "path": { "type": "string", "description": "Relative to the worktree root; empty for the root." },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id"]
            }),
        ),
        tool_def(
            DIFF_SESSION_WORKTREE,
            "Diff another session's worktree of this project against its base, including uncommitted and untracked files.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id"]
            }),
        ),
    ];
    defs.push(start_options_tool_def());
    defs.extend(worktree_tool_defs());
    defs.extend(session_follow_tool_defs());
    defs.extend(project_home_terminal_tool_defs());
    defs.extend(project_home_answer_tool_defs());
    defs.extend(project_home_inbox_tool_defs());
    defs.extend(terminate_session_tool_def());
    defs.extend(refresh_worktree_disk_usage_tool_def());
    defs.push(notify_operator_tool_def());
    defs
}

pub(crate) fn tool_def(name: &str, description: &str, schema: Value) -> ToolDef {
    ToolDef {
        name: name.to_string(),
        description: description.to_string(),
        input_schema_json: schema.to_string(),
    }
}
