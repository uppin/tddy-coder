//! The project-home session's reach (`#project-home` 5/7): four exec tools, scoped to the caller's
//! own project and executable only by a session with the `project-home` role.
//!
//! | Tool | Does |
//! |---|---|
//! | `ListProjectSessions` | every other session of the project, with status and worktree |
//! | `StartProjectSession` | starts a session in the project, tagged with the caller as orchestrator |
//! | `ReadSessionWorktree` | reads a file or lists a directory in a target session's worktree |
//! | `DiffSessionWorktree` | a target worktree's diff against its base |
//!
//! Targets resolve through `tddy_worktree_service::session_target`. Listing and starting need the
//! daemon's session store and start path, which reach this crate through [`ProjectHomePort`].
//! The role gate is the daemon's: it refuses these names for any session that is not a
//! project-home one before anything here runs.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::path::{Component, Path};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tddy_core::cli_harness::{cli_harness_requires_model, cli_harnesses};
use tddy_worktree_service::session_git_ops::PushIdentity;
use tddy_worktree_service::session_target::{
    resolve_session_target, HomeCaller, SessionTarget, TargetRecord,
};
use tddy_worktree_service::worktree_files::{
    list_worktree_directory_entries, read_worktree_file_utf8,
};

use crate::catalog::{tool_catalog, ToolDef};
use crate::project_home_diff::worktree_diff;
use crate::project_home_git_tools::{
    execute_project_home_git_tool, is_project_home_git_tool, project_home_git_tool_defs,
};
use crate::read_only::is_project_home_allowed_tool;
use crate::ToolOutcome;

pub const LIST_PROJECT_SESSIONS: &str = "ListProjectSessions";
pub const START_PROJECT_SESSION: &str = "StartProjectSession";
pub const READ_SESSION_WORKTREE: &str = "ReadSessionWorktree";
pub const DIFF_SESSION_WORKTREE: &str = "DiffSessionWorktree";

/// Whether `name` is one of the four project-home tools.
pub fn is_project_home_tool(name: &str) -> bool {
    matches!(
        name,
        LIST_PROJECT_SESSIONS
            | START_PROJECT_SESSION
            | READ_SESSION_WORKTREE
            | DIFF_SESSION_WORKTREE
    )
}

/// The exec tools a project-home session's agent is offered: exactly the tools
/// [`is_project_home_allowed_tool`] lets it run, in catalog order, then the four project-home tools,
/// then the seven project-home git tools (`#project-home` 7/7).
/// The same predicate the daemon's role guard applies at call time, so what is advertised and what
/// is executed cannot disagree.
pub fn project_home_offered_catalog() -> Vec<ToolDef> {
    tool_catalog()
        .into_iter()
        .filter(|tool| is_project_home_allowed_tool(&tool.name))
        .chain(project_home_tool_defs())
        .chain(project_home_git_tool_defs())
        .collect()
}

/// The exec tools a session's agent is offered, by whether it is a project-home session: the
/// project-home catalog for one, the daemon-wide catalog for any other. The one place that answer is
/// decided, for every host that answers `ListExecTools` (the token route and the jail's bridge).
pub fn session_offered_catalog(is_project_home: bool) -> Vec<ToolDef> {
    if is_project_home {
        project_home_offered_catalog()
    } else {
        tool_catalog()
    }
}

/// One session as `ListProjectSessions` reports it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ListedSession {
    pub session_id: String,
    pub branch: String,
    pub worktree: String,
    pub is_active: bool,
    /// The agent status name (`idle`, `running`, `waiting_for_input`, …).
    pub agent_status: String,
    pub pending_elicitation: bool,
    /// The daemon the session runs on, as the eligible-daemon roster names it. A target tool given
    /// this id reaches the session on that host (`#start-parity` 1/9).
    #[serde(default)]
    pub daemon_instance_id: String,
}

/// What `ListProjectSessions` answers: the project's sessions on every eligible host, and the hosts
/// that did not answer — named rather than silently left out of the list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ListProjectSessionsResult {
    pub sessions: Vec<ListedSession>,
    pub unreachable_hosts: Vec<UnreachableHost>,
}

/// An eligible host that did not answer `ListProjectSessions`'s fan-out, and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnreachableHost {
    pub daemon_instance_id: String,
    pub reason: String,
}

/// The argument every project-home target tool takes to name the host its target runs on. Empty or
/// absent is this host.
pub const TARGET_DAEMON_INSTANCE_ID: &str = "daemon_instance_id";

/// The host `args_json` names in [`TARGET_DAEMON_INSTANCE_ID`], or `None` for this host (the
/// argument absent or empty, or `args_json` not an object).
pub fn target_daemon_instance_id(args_json: &str) -> Option<String> {
    // TODO(remote-targets): implement
    let _ = args_json;
    None
}

/// `StartProjectSession`'s arguments.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StartProjectSessionArgs {
    /// The session type to start (`claude-cli`, `workspace`, …).
    pub session_type: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub agent: String,
    /// Branch to create for the session; empty lets the start path name one.
    #[serde(default)]
    pub new_branch_name: String,
    /// Model id for claude-cli sessions; empty is fine for a workspace start.
    #[serde(default)]
    pub model: String,
}

/// What the tools need from the daemon.
///
/// Async because the daemon's answers read the session store and start a session on disk. The
/// target records are every session of every project: the resolver, not the port, decides which
/// project a target may be in.
#[async_trait]
pub trait ProjectHomePort: Send + Sync {
    /// Every session of `project_id` except `home_session_id`.
    async fn list_sessions(
        &self,
        project_id: &str,
        home_session_id: &str,
    ) -> anyhow::Result<Vec<ListedSession>>;

    /// Start a session in `project_id` with `home_session_id` as its orchestrator; its id.
    async fn start_session(
        &self,
        project_id: &str,
        home_session_id: &str,
        args: StartProjectSessionArgs,
    ) -> anyhow::Result<String>;

    /// Every session on this daemon, as [`tddy_worktree_service::session_target`] resolves them.
    async fn target_records(&self) -> anyhow::Result<Vec<TargetRecord>>;

    /// The git environment `target`'s own commits and pushes run under, from its project's
    /// identity (`#project-home` 7/7). Empty when that identity does not resolve.
    async fn push_identity(&self, target: &SessionTarget) -> anyhow::Result<PushIdentity>;
}

/// The four tools' definitions, as advertised to a project-home session.
pub fn project_home_tool_defs() -> Vec<ToolDef> {
    vec![
        tool_def(
            LIST_PROJECT_SESSIONS,
            "List the other sessions of this project with their worktree, activity and status.",
            json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        tool_def(
            START_PROJECT_SESSION,
            "Start a session in this project. It is recorded as started by this project-home session.",
            start_project_session_schema(),
        ),
        tool_def(
            READ_SESSION_WORKTREE,
            "Read a file, or list a directory, in another session's worktree of this project.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "path": { "type": "string", "description": "Relative to the worktree root; empty for the root." }
                },
                "required": ["session_id"]
            }),
        ),
        tool_def(
            DIFF_SESSION_WORKTREE,
            "Diff another session's worktree of this project against its base, including uncommitted and untracked files.",
            json!({
                "type": "object",
                "properties": { "session_id": { "type": "string" } },
                "required": ["session_id"]
            }),
        ),
    ]
}

/// `StartProjectSession`'s arguments as the calling agent is told them. The session types and the
/// ones that need a model are read from the CLI harness registry, so a new harness is offered here
/// without an edit.
fn start_project_session_schema() -> Value {
    let harnesses = cli_harnesses();
    let session_types: Vec<&str> = ["tool", "workspace"]
        .into_iter()
        .chain(harnesses.iter().map(|harness| harness.session_type))
        .collect();
    let harness_list = harnesses
        .iter()
        .map(|harness| format!("`{}` ({})", harness.session_type, harness.label))
        .collect::<Vec<_>>()
        .join(", ");
    let needing_a_model = harnesses
        .iter()
        .map(|harness| harness.session_type)
        .filter(|session_type| cli_harness_requires_model(session_type))
        .map(|session_type| format!("`{session_type}`"))
        .collect::<Vec<_>>()
        .join(" and ");
    json!({
        "type": "object",
        "properties": {
            "session_type": {
                "type": "string",
                "enum": session_types,
                "description": format!(
                    "What to start. `tool`: a tddy-coder workflow session run by `agent`. \
                     `workspace`: a worktree with no agent. Any other value runs that coding agent \
                     interactively in a terminal: {harness_list}."
                )
            },
            "prompt": {
                "type": "string",
                "description": "The first turn sent to the session's agent; empty starts it idle. Ignored for `workspace`."
            },
            "agent": {
                "type": "string",
                "description": "An agent id: a coding backend from the daemon's `allowed_agents`, an assistant in its registry, or a harness profile under `<tddyhome>/agents/` (which supplies its own session settings). Empty uses the session type's default."
            },
            "new_branch_name": {
                "type": "string",
                "description": "Branch to create from the project's base for the session; empty lets the start path name one. A branch another session owns is refused."
            },
            "model": {
                "type": "string",
                "description": format!(
                    "Model id or alias for the session's agent (for `claude-cli`: `opus`, `sonnet`, …). \
                     Required for {needing_a_model}: a start of those without one is refused. \
                     Other session types pick their own when empty."
                )
            }
        },
        "required": ["session_type"]
    })
}

fn tool_def(name: &str, description: &str, schema: Value) -> ToolDef {
    ToolDef {
        name: name.to_string(),
        description: description.to_string(),
        input_schema_json: schema.to_string(),
    }
}

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
        START_PROJECT_SESSION => start_project_session(port, home, args).await,
        READ_SESSION_WORKTREE => read_session_worktree(port, home, args).await,
        DIFF_SESSION_WORKTREE => diff_session_worktree(port, home, args).await,
        other => ToolOutcome::err(format!("unknown project-home tool: {other}")),
    }
}

async fn list_project_sessions(port: &dyn ProjectHomePort, home: &HomeCaller) -> ToolOutcome {
    match port.list_sessions(&home.project_id, &home.session_id).await {
        Ok(sessions) => ToolOutcome::ok(json!({ "sessions": sessions }).to_string()),
        Err(e) => ToolOutcome::err(format!("{LIST_PROJECT_SESSIONS}: {e:#}")),
    }
}

async fn start_project_session(
    port: &dyn ProjectHomePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: StartProjectSessionArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutcome::err(format!("{START_PROJECT_SESSION}: invalid arguments: {e}"))
        }
    };
    match port
        .start_session(&home.project_id, &home.session_id, args)
        .await
    {
        Ok(session_id) => ToolOutcome::ok(json!({ "session_id": session_id }).to_string()),
        Err(e) => ToolOutcome::err(format!("{START_PROJECT_SESSION}: {e:#}")),
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
