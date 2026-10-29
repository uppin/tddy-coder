//! The project-home session's reach (`#project-home` 5/7): four exec tools, scoped to the caller's
//! own project and executable only by a session with the `project-home` role.
//!
//! | Tool | Does |
//! |---|---|
//! | `ListProjectSessions` | every other session of the project on every host, with its host, status and worktree |
//! | `StartSession` | starts a session in the project with any New-session option, on any eligible host, linked to the caller as its orchestrator ([`crate::start_session_args`]); it returns a handle at once ([`crate::start_ledger`]) |
//! | `GetSessionStart` | a start's state, step, and on success its session, branch and worktree, or its failure |
//! | `CancelSessionStart` | cancels a start still running, on whichever host runs it |
//! | `DescribeSessionStartOptions` | the agents, models, provider routes, SSH hosts, subagents, branches and stack parents a start may choose on a host ([`crate::start_options`]) |
//! | `ReadSessionWorktree` | reads a file or lists a directory in a target session's worktree |
//! | `DiffSessionWorktree` | a target worktree's diff against its base |
//! | `ListWorktrees` | the project's worktrees as the Worktrees screen shows them, a page at a time ([`crate::project_home_worktree_tools`]) |
//! | `GetWorktree` | one of those worktrees, with the session that owns it |
//! | `ReadSessionActivity` | a target session's tool calls, paged, with the question it waits on ([`crate::session_follow`]) |
//! | `ReadSessionTranscript` | a target session's transcript, paged; one tool call whole by id |
//! | `TerminateSession` | sends a target session's process SIGTERM, as the UI's Terminate does; the record, worktree and branch stay ([`crate::project_home_terminate`]) |
//! | `RefreshWorktreeDiskUsage` | queues the Worktrees screen's size walk for one worktree or every one, and answers at once; the progress is read with `ListWorktrees` ([`crate::project_home_disk_refresh`]) |
//! | `ReadSessionTerminal` | what a target session's main terminal shows right now, as text, with its cursor ([`crate::project_home_terminal_tools`]) |
//! | `ClaimSessionTerminal` | takes a target session's terminal control lease for this home session ([`crate::project_home_terminal_tools`]) |
//! | `SendSessionKeys` | types text and named keys into a target session's terminal while this home session holds the lease ([`crate::terminal_keys`]) |
//! | `AnswerSessionPermission` | answers the permission prompt a session this one started is waiting on; its orchestrator only ([`crate::project_home_answer_tools`]) |
//! | `AnswerSessionQuestion` | answers the `AskUserQuestion` a session this one started is waiting on, validated against its options; its orchestrator only ([`crate::project_home_answer_tools`]) |
//! | `WaitProjectNotifications` | holds until this project's sessions need it, or a timeout passes, and answers what they raised ([`crate::project_home_inbox_tools`]) |
//! | `ReadProjectNotifications` | reads what this project's sessions raised, without waiting ([`crate::project_home_inbox_tools`]) |
//! | `NotifyOperator` | notifies the operator in the agent's own words, as `tddy-tools notify` does, for a session with no shell ([`crate::project_home_notify_tools`]) |
//!
//! Targets resolve through `tddy_worktree_service::session_target`. Listing and starting need the
//! daemon's session store and start path, which reach this crate through [`ProjectHomePort`].
//! The role gate is the daemon's: it refuses these names for any session that is not a
//! project-home one before anything here runs.
//!
//! A target tool takes [`TARGET_DAEMON_INSTANCE_ID`] to name the host its target runs on; the
//! daemon forwards such a call to that host whole, and it runs there unchanged (`#start-parity`
//! 1/9). The argument is never read here.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::path::{Component, Path};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{
    resolve_session_target, HomeCaller, SessionTarget, TargetRecord,
};
use tddy_worktree_service::worktree_files::{
    list_worktree_directory_entries, read_worktree_file_utf8,
};

use crate::project_home_answer_tools::{ANSWER_SESSION_PERMISSION, ANSWER_SESSION_QUESTION};
use crate::project_home_diff::worktree_diff;
use crate::project_home_disk_refresh::{refresh_worktree_disk_usage, REFRESH_WORKTREE_DISK_USAGE};
use crate::project_home_git_tools::{execute_project_home_git_tool, is_project_home_git_tool};
use crate::project_home_inbox_tools::{READ_PROJECT_NOTIFICATIONS, WAIT_PROJECT_NOTIFICATIONS};
use crate::project_home_terminal_tools::{
    CLAIM_SESSION_TERMINAL, READ_SESSION_TERMINAL, SEND_SESSION_KEYS,
};
use crate::project_home_terminate::{terminate_session, TERMINATE_SESSION};
use crate::project_home_worktree_tools::{
    run_get_worktree, run_list_worktrees, WorktreeEntry, GET_WORKTREE, LIST_WORKTREES,
};
use crate::session_follow::{
    run_session_follow_tool, SessionActivity, ToolCallBodies, TranscriptItem,
    READ_SESSION_ACTIVITY, READ_SESSION_TRANSCRIPT,
};
use crate::start_ledger::{
    StartHandle, StartLedgerRecord, CANCEL_SESSION_START, GET_SESSION_START,
};
use crate::start_options::{StartOptions, StartOptionsQuery, DESCRIBE_SESSION_START_OPTIONS};
use crate::start_session_args::{StartSessionArgs, START_SESSION};
use crate::ToolOutcome;

pub const LIST_PROJECT_SESSIONS: &str = "ListProjectSessions";
pub const READ_SESSION_WORKTREE: &str = "ReadSessionWorktree";
pub const DIFF_SESSION_WORKTREE: &str = "DiffSessionWorktree";

pub use crate::project_home_catalogue::{
    is_conversation_spawner_scoped_tool, is_conversation_spawner_tool, is_project_home_tool,
    project_home_offered_catalog, session_offered_catalog, session_offered_catalog_for,
    SessionToolRole,
};

pub use crate::project_home_listing::{ListProjectSessionsResult, ListedSession, UnreachableHost};

pub(crate) use crate::project_home_target_host::target_host_property;
pub use crate::project_home_target_host::{
    is_project_home_target_tool, target_daemon_instance_id, TARGET_DAEMON_INSTANCE_ID,
};

/// What the tools need from the daemon.
///
/// Async because the daemon's answers read the session store and start a session on disk. The
/// target records are every session of every project: the resolver, not the port, decides which
/// project a target may be in.
#[async_trait]
pub trait ProjectHomePort: Send + Sync {
    /// Every session of `project_id` on this daemon except `home_session_id`.
    async fn list_sessions(
        &self,
        project_id: &str,
        home_session_id: &str,
    ) -> anyhow::Result<Vec<ListedSession>>;

    /// The sessions of `project_id` on every other eligible host, each asked for its own, and the
    /// hosts that did not answer. Empty when the call is not this daemon's to fan out — one that
    /// arrived from a peer — so a listing never recurses.
    async fn list_peer_sessions(
        &self,
        project_id: &str,
        home_session_id: &str,
    ) -> ListProjectSessionsResult;

    /// Begin the session `args` ask for in `project_id`, on the host they name, with
    /// `home_session_id` as its orchestrator. Answers at once with the start's handle; the session
    /// is the start's outcome, read with [`Self::get_session_start`].
    async fn start_session(
        &self,
        project_id: &str,
        home_session_id: &str,
        args: StartSessionArgs,
    ) -> anyhow::Result<StartHandle>;

    /// The record of `start_id`, a start `home_session_id` asked for.
    async fn get_session_start(
        &self,
        home_session_id: &str,
        start_id: &str,
    ) -> anyhow::Result<StartLedgerRecord>;

    /// Ask the host running `start_id` to stop it; it reads cancelled once it has stopped.
    async fn cancel_session_start(
        &self,
        home_session_id: &str,
        start_id: &str,
    ) -> anyhow::Result<()>;

    /// Every session on this daemon, as [`tddy_worktree_service::session_target`] resolves them.
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>>;

    /// The git environment `target`'s own commits and pushes run under, from its project's
    /// identity (`#project-home` 7/7). Empty when that identity does not resolve.
    async fn push_identity(&self, target: &SessionTarget) -> anyhow::Result<PushIdentity>;

    /// Every worktree of `project_id` on this host, as the Worktrees screen's snapshot has them
    /// (`#start-parity` 3/9).
    async fn list_worktrees(&self, project_id: &str) -> anyhow::Result<Vec<WorktreeEntry>>;

    /// Every call `target` has made and the question it waits on (`#start-parity` 6/9).
    async fn session_activity(&self, target: &SessionTarget) -> anyhow::Result<SessionActivity>;

    /// `target`'s transcript as the Activities view reads it, oldest first, tool bodies stripped;
    /// an item's index is its `seq`.
    async fn session_transcript(
        &self,
        target: &SessionTarget,
    ) -> anyhow::Result<Vec<TranscriptItem>>;

    /// One tool call's whole input and output, or `None` for an id the session never made.
    async fn session_tool_call(
        &self,
        target: &SessionTarget,
        tool_call_id: &str,
    ) -> anyhow::Result<Option<ToolCallBodies>>;

    /// The per-host choices behind `StartSession`'s arguments, as `query` narrows them, for
    /// `project_id`'s stack and branch pickers (`#start-parity` 5/9).
    async fn start_options(
        &self,
        project_id: &str,
        query: StartOptionsQuery,
    ) -> anyhow::Result<StartOptions>;

    /// Send SIGTERM to `target`'s process, as the web UI's Terminate does (`#start-parity` 7/9).
    async fn terminate_session(&self, target: &SessionTarget) -> anyhow::Result<()>;

    /// Queue a size walk for the worktree at `path` of `project_id`, or for every worktree of it
    /// when `path` is `None`, as the Worktrees screen's Refresh does; how many were queued
    /// (`#start-parity` 8/9). A `path` not in the worktree list is an error naming it.
    async fn refresh_worktree_disk_usage(
        &self,
        project_id: &str,
        path: Option<&str>,
    ) -> anyhow::Result<crate::project_home_disk_refresh::RefreshQueued>;
}

pub use crate::project_home_catalogue::project_home_tool_defs;
pub(crate) use crate::project_home_catalogue::tool_def;

/// Run one of the project-home tools for `home`: the four here, or the seven git tools.
///
/// A target or path refusal, and a failure of the port, come back as an `is_error` outcome whose
/// message names what was refused. The role refusal is not here: it is the daemon's, and happens
/// before this is called.
pub async fn execute_project_home_tool(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    if is_project_home_git_tool(tool_name) {
        return execute_project_home_git_tool(port, home, tool_name, args_json).await;
    }
    let args: Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("invalid args_json: {e}")),
    };
    match tool_name {
        LIST_PROJECT_SESSIONS => list_project_sessions(port, home).await,
        START_SESSION => start_session(port, home, args).await,
        GET_SESSION_START => get_session_start(port, home, args).await,
        CANCEL_SESSION_START => cancel_session_start(port, home, args).await,
        DESCRIBE_SESSION_START_OPTIONS => describe_session_start_options(port, home, args).await,
        READ_SESSION_WORKTREE => read_session_worktree(port, home, args).await,
        DIFF_SESSION_WORKTREE => diff_session_worktree(port, home, args).await,
        LIST_WORKTREES => run_list_worktrees(port, home, args).await,
        GET_WORKTREE => run_get_worktree(port, home, args).await,
        READ_SESSION_ACTIVITY | READ_SESSION_TRANSCRIPT => {
            run_session_follow_tool(port, home, tool_name, args).await
        }
        TERMINATE_SESSION => terminate_session(port, home, args).await,
        REFRESH_WORKTREE_DISK_USAGE => refresh_worktree_disk_usage(port, home, args).await,
        // Need the terminal port as well, so the daemon answers them through
        // `execute_project_home_terminal_tool`; reaching here is a wiring mistake, named as one.
        READ_SESSION_TERMINAL | CLAIM_SESSION_TERMINAL | SEND_SESSION_KEYS => ToolOutcome::err(
            format!("{tool_name} is answered by execute_project_home_terminal_tool, not by execute_project_home_tool"),
        ),
        ANSWER_SESSION_PERMISSION | ANSWER_SESSION_QUESTION => ToolOutcome::err(
            format!("{tool_name} is answered by execute_project_home_answer_tool, not by execute_project_home_tool"),
        ),
        // Read the daemon's notification log, so the daemon answers them through
        // `execute_project_home_inbox_tool`.
        WAIT_PROJECT_NOTIFICATIONS | READ_PROJECT_NOTIFICATIONS => ToolOutcome::err(format!(
            "{tool_name} is answered by execute_project_home_inbox_tool, not by execute_project_home_tool"
        )),
        other => ToolOutcome::err(format!("unknown project-home tool: {other}")),
    }
}

/// This host's sessions of the project, then every other host's.
async fn list_project_sessions(port: &dyn ProjectHomePort, home: &HomeCaller) -> ToolOutcome {
    let mut sessions = match port.list_sessions(&home.project_id, &home.session_id).await {
        Ok(sessions) => sessions,
        Err(e) => return ToolOutcome::err(format!("{LIST_PROJECT_SESSIONS}: {e:#}")),
    };
    let peers = port
        .list_peer_sessions(&home.project_id, &home.session_id)
        .await;
    sessions.extend(peers.sessions);
    let result = ListProjectSessionsResult {
        sessions,
        unreachable_hosts: peers.unreachable_hosts,
    };
    match serde_json::to_string(&result) {
        Ok(json) => ToolOutcome::ok(json),
        Err(e) => ToolOutcome::err(format!("{LIST_PROJECT_SESSIONS}: {e}")),
    }
}

async fn start_session(port: &dyn ProjectHomePort, home: &HomeCaller, args: Value) -> ToolOutcome {
    let args: StartSessionArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{START_SESSION}: invalid arguments: {e}")),
    };
    match port
        .start_session(&home.project_id, &home.session_id, args)
        .await
    {
        Ok(handle) => ToolOutcome::ok(
            json!({
                "start_id": handle.start_id,
                "task_id": handle.task_id,
                "daemon_instance_id": handle.daemon_instance_id,
            })
            .to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("{START_SESSION}: {e:#}")),
    }
}

/// The arguments of the two tools that follow a start.
#[derive(Deserialize)]
struct StartIdArgs {
    start_id: String,
}

fn parse_start_id_args(tool_name: &str, args: Value) -> Result<StartIdArgs, ToolOutcome> {
    serde_json::from_value(args)
        .map_err(|e| ToolOutcome::err(format!("{tool_name}: invalid arguments: {e}")))
}

async fn get_session_start(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args = match parse_start_id_args(GET_SESSION_START, args) {
        Ok(a) => a,
        Err(outcome) => return outcome,
    };
    match port
        .get_session_start(&home.session_id, &args.start_id)
        .await
    {
        Ok(record) => match serde_json::to_string(&record) {
            Ok(json) => ToolOutcome::ok(json),
            Err(e) => ToolOutcome::err(format!("{GET_SESSION_START}: {e}")),
        },
        Err(e) => ToolOutcome::err(format!("{GET_SESSION_START}: {e:#}")),
    }
}

async fn cancel_session_start(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args = match parse_start_id_args(CANCEL_SESSION_START, args) {
        Ok(a) => a,
        Err(outcome) => return outcome,
    };
    match port
        .cancel_session_start(&home.session_id, &args.start_id)
        .await
    {
        Ok(()) => ToolOutcome::ok(
            json!({ "start_id": args.start_id, "cancel_requested": true }).to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("{CANCEL_SESSION_START}: {e:#}")),
    }
}

/// The host's start options, as `args` narrow them.
async fn describe_session_start_options(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let query: StartOptionsQuery = match serde_json::from_value(args) {
        Ok(query) => query,
        Err(e) => {
            return ToolOutcome::err(format!(
                "{DESCRIBE_SESSION_START_OPTIONS}: invalid arguments: {e}"
            ))
        }
    };
    match port.start_options(&home.project_id, query).await {
        Ok(options) => match serde_json::to_string(&options) {
            Ok(json) => ToolOutcome::ok(json),
            Err(e) => ToolOutcome::err(format!("{DESCRIBE_SESSION_START_OPTIONS}: {e}")),
        },
        Err(e) => ToolOutcome::err(format!("{DESCRIBE_SESSION_START_OPTIONS}: {e:#}")),
    }
}

/// The arguments of the two tools that name a target session.
#[derive(Deserialize)]
struct TargetArgs {
    session_id: String,
    #[serde(default)]
    path: String,
}

fn parse_target_args(tool_name: &str, args: Value) -> Result<TargetArgs, ToolOutcome> {
    serde_json::from_value(args)
        .map_err(|e| ToolOutcome::err(format!("{tool_name}: invalid arguments: {e}")))
}

/// Resolve the target named in `session_id` for `home`, or the refusal as an outcome.
pub(crate) async fn resolve_target(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    session_id: &str,
) -> Result<SessionTarget, ToolOutcome> {
    let records = port
        .target_records()
        .await
        .map_err(|e| ToolOutcome::err(format!("cannot read the daemon's sessions: {e:#}")))?;
    resolve_session_target(&records, home, session_id).map_err(|e| ToolOutcome::err(e.to_string()))
}

async fn read_session_worktree(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args = match parse_target_args(READ_SESSION_WORKTREE, args) {
        Ok(a) => a,
        Err(outcome) => return outcome,
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let rel = match contained_rel_path(&target.worktree, &args.path) {
        Ok(rel) => rel,
        Err(message) => return ToolOutcome::err(message),
    };
    let root = target.worktree;
    let read = tokio::task::spawn_blocking(move || read_or_list(&root, &rel)).await;
    match read {
        Ok(Ok(value)) => ToolOutcome::ok(value.to_string()),
        Ok(Err(message)) => ToolOutcome::err(format!("{READ_SESSION_WORKTREE}: {message}")),
        Err(e) => ToolOutcome::err(format!("{READ_SESSION_WORKTREE}: read task failed: {e}")),
    }
}

/// A directory listing when `rel` is a directory, otherwise the file's text. Both go through the
/// worktree reader that the Code pane uses, so a git-ignored or `.git` path is not readable here.
fn read_or_list(root: &Path, rel: &str) -> Result<Value, String> {
    if rel.is_empty() || root.join(rel).is_dir() {
        let entries =
            list_worktree_directory_entries(root, rel).map_err(|s| s.message().to_string())?;
        return Ok(
            json!({ "path": rel, "entries": entries.iter().map(|e| json!({
            "name": e.name, "is_dir": e.is_dir, "size_bytes": e.size_bytes
        })).collect::<Vec<_>>() }),
        );
    }
    let file = read_worktree_file_utf8(root, rel).map_err(|s| s.message().to_string())?;
    Ok(json!({
        "path": rel,
        "content": file.content_utf8,
        "truncated": file.truncated,
        "byte_size": file.byte_size,
    }))
}

/// `path` as a worktree-relative path with no `.` segments, or the refusal when it leaves the
/// worktree: absolute, climbing above the root, or resolving through a symlink to outside it.
fn contained_rel_path(worktree: &Path, path: &str) -> Result<String, String> {
    let outside = || format!("path {path:?} is outside the worktree");
    let as_path = Path::new(path);
    if as_path.is_absolute() {
        return Err(outside());
    }
    let mut segments: Vec<String> = Vec::new();
    for component in as_path.components() {
        match component {
            Component::Normal(segment) => segments.push(segment.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if segments.pop().is_none() {
                    return Err(outside());
                }
            }
            Component::RootDir | Component::Prefix(_) => return Err(outside()),
        }
    }
    let rel = segments.join("/");
    if let (Ok(root), Ok(full)) = (worktree.canonicalize(), worktree.join(&rel).canonicalize()) {
        if !full.starts_with(&root) {
            return Err(outside());
        }
    }
    Ok(rel)
}

async fn diff_session_worktree(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args = match parse_target_args(DIFF_SESSION_WORKTREE, args) {
        Ok(a) => a,
        Err(outcome) => return outcome,
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let base_ref = if target.base_ref.is_empty() {
        "HEAD".to_string()
    } else {
        target.base_ref.clone()
    };
    match worktree_diff(&target.worktree, &base_ref).await {
        Ok(diff) => ToolOutcome::ok(
            json!({ "session_id": target.session_id, "base_ref": base_ref, "diff": diff })
                .to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("{DIFF_SESSION_WORKTREE}: {e}")),
    }
}
