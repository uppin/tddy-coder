//! `SetSessionMark`, `DeleteSession`, `ClearWorktree`, `DeleteWorktree` and `RestoreWorktree`: an
//! orchestrator keeps a ledger of its sessions in the sessions themselves and cleans up after
//! them, with every action the web offers for a session and its worktree (`#orchestrators` 5/11).
//!
//! - **`SetSessionMark`** records `paused`, `blocked` or `done`, with a note, in the session's
//!   `.session.yaml` ([`OrchestrationMark`]); `none` removes the mark. **`paused` also stops the
//!   session**, as `TerminateSession` does, and any resume clears it. `blocked` and `done` only
//!   mark.
//! - **`DeleteSession`** does what the web's Delete does, for a stopped session. A running session
//!   is refused: stop it first.
//! - **`ClearWorktree`**, **`DeleteWorktree`** and **`RestoreWorktree`** are the Worktrees
//!   screen's Clear, Delete and Restore, over the functions its RPCs call. Clear and Delete are
//!   addressed by `path` as `ListWorktrees` lists it; Restore by `session_id`, as the screen's
//!   `RestoreSessionWorktree` is — a deleted worktree is no longer listed (developer, 2026-11-13).
//!   Each is refused while a session owning the worktree is active (the git tools' activity gate).
//!   `DeleteWorktree` keeps the branch, and also refuses a worktree with uncommitted changes
//!   (naming `CommitSessionWorktree` and `StashSessionWorktree`) and **the project's home
//!   worktree** — the one path main and every orchestrator share
//!   ([`ProjectHomeLifecyclePort::project_home_worktree`]), whoever calls (developer, 2026-11-13).
//!
//! All five are target tools: they take
//! [`TARGET_DAEMON_INSTANCE_ID`](crate::project_home_tools::TARGET_DAEMON_INSTANCE_ID) and the
//! daemon forwards a call naming another host to that host whole, so the port below is only ever
//! asked about this host's sessions and worktrees.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-session-lifecycle-tools.md.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tddy_core::orchestration_mark::{OrchestrationMark, OrchestrationMarkKind};
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

use crate::catalog::ToolDef;
use crate::project_home_tools::{
    resolve_target, target_host_property, tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use crate::ToolOutcome;

pub const SET_SESSION_MARK: &str = "SetSessionMark";
pub const DELETE_SESSION: &str = "DeleteSession";
pub const CLEAR_WORKTREE: &str = "ClearWorktree";
pub const DELETE_WORKTREE: &str = "DeleteWorktree";
pub const RESTORE_WORKTREE: &str = "RestoreWorktree";

/// Whether `name` is one of the five lifecycle tools.
pub fn is_project_home_lifecycle_tool(name: &str) -> bool {
    matches!(
        name,
        SET_SESSION_MARK | DELETE_SESSION | CLEAR_WORKTREE | DELETE_WORKTREE | RESTORE_WORKTREE
    )
}

/// The `mark` argument of `SetSessionMark`: one of the three marks, or `none` to remove it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMark {
    Mark(OrchestrationMarkKind),
    None,
}

impl SessionMark {
    /// `paused`, `blocked`, `done` or `none`; anything else is an error naming the four.
    pub fn parse(word: &str) -> Result<Self, String> {
        match word {
            "paused" => Ok(Self::Mark(OrchestrationMarkKind::Paused)),
            "blocked" => Ok(Self::Mark(OrchestrationMarkKind::Blocked)),
            "done" => Ok(Self::Mark(OrchestrationMarkKind::Done)),
            "none" => Ok(Self::None),
            _ => Err(format!(
                "mark {word:?} is not one of paused, blocked, done or none"
            )),
        }
    }

    /// The word the answer spells it with.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mark(kind) => kind.as_str(),
            Self::None => "none",
        }
    }
}

/// The five tools' definitions.
pub fn project_home_lifecycle_tool_defs() -> Vec<ToolDef> {
    let session_property = json!({
        "type": "string",
        "description": "The session of this project, as ListProjectSessions reports it."
    });
    let path_property = json!({
        "type": "string",
        "description": "The worktree's path, as ListWorktrees lists it."
    });
    vec![
        tool_def(
            SET_SESSION_MARK,
            "Mark a session of this project paused, blocked or done, with an optional note, or remove its mark with none. The mark is recorded in the session itself, listed by ListProjectSessions and shown on the session's row in the web UI, note on hover. paused also stops the session, as TerminateSession does; resuming it, from the web UI or with ResumeSession, clears the paused mark. blocked and done only mark: the session keeps running. Answers session_id, mark, note, set_by and at_unix_ms.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": session_property.clone(),
                    "mark": {
                        "type": "string",
                        "enum": ["paused", "blocked", "done", "none"],
                        "description": "paused stops the session; blocked and done only mark it; none removes its mark."
                    },
                    "note": {
                        "type": "string",
                        "description": "Why, in a few words. Shown on hover in the web UI."
                    },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id", "mark"]
            }),
        ),
        tool_def(
            DELETE_SESSION,
            "Delete a stopped session of this project, as the web UI's Delete does: its record, its worktree and its process are removed. A running session is refused: stop it first with TerminateSession, or mark it paused with SetSessionMark.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": session_property.clone(),
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id"]
            }),
        ),
        tool_def(
            CLEAR_WORKTREE,
            "Clear one of this project's worktrees, as the Worktrees screen's Clear does: untracked and ignored files (build output) are removed to reclaim disk; tracked files, commits and the worktree itself stay. Refused while a session owning the worktree is active.",
            json!({
                "type": "object",
                "properties": {
                    "path": path_property,
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["path"]
            }),
        ),
        tool_def(
            DELETE_WORKTREE,
            "Delete one of this project's worktrees, as the Worktrees screen's Delete does (git worktree remove). Its branch is kept. Refused while a session owning the worktree is active, for a worktree with uncommitted changes (commit them with CommitSessionWorktree or set them aside with StashSessionWorktree first), and for the project's home worktree. A session whose worktree was deleted can have it back with RestoreWorktree.",
            json!({
                "type": "object",
                "properties": {
                    "path": path_property,
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["path"]
            }),
        ),
        tool_def(
            RESTORE_WORKTREE,
            "Restore a session's deleted worktree, as the Worktrees screen's Restore does: it is cut again from the base the session recorded. Refused while that session is active. Answers session_id and the restored worktree's path, worktree_path.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": session_property,
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id"]
            }),
        ),
    ]
}

/// What the tools need from the daemon beyond the target resolver and `terminate_session`: writing
/// a session's mark, deleting a session, and the Worktrees screen's three actions, all on this host.
///
/// Every default refuses, so a port that does none of them need not name them.
#[async_trait]
pub trait ProjectHomeLifecyclePort: ProjectHomePort {
    /// Record `mark` in `target`'s `.session.yaml`, or remove its mark when `mark` is `None`. Only
    /// records: stopping a session marked paused is the tool's, through
    /// [`ProjectHomePort::terminate_session`].
    async fn set_session_mark(
        &self,
        target: &SessionTarget,
        mark: Option<&OrchestrationMark>,
    ) -> Result<(), String> {
        let _ = (target, mark);
        Err(NO_LIFECYCLE.to_string())
    }

    /// Delete `target` as `DeleteSession` does — record, worktree and all. The tool has already
    /// refused an active target.
    async fn delete_session(&self, target: &SessionTarget) -> Result<(), String> {
        let _ = target;
        Err(NO_LIFECYCLE.to_string())
    }

    /// `CleanWorktree` on the worktree at `path` of `project_id` (`git clean -fdx`).
    async fn clear_worktree(&self, project_id: &str, path: &Path) -> Result<(), String> {
        let _ = (project_id, path);
        Err(NO_LIFECYCLE.to_string())
    }

    /// `RemoveWorktree` on the worktree at `path` of `project_id` (`git worktree remove`; the
    /// branch is kept). An error names why it was not removed — uncommitted changes naming
    /// `CommitSessionWorktree` and `StashSessionWorktree`, among them.
    async fn delete_worktree(&self, project_id: &str, path: &Path) -> Result<(), String> {
        let _ = (project_id, path);
        Err(NO_LIFECYCLE.to_string())
    }

    /// The project's home worktree on this host — `<worktrees root>/<project_id>/home`, the one
    /// checkout main and every orchestrator of `project_id` work in. `DeleteWorktree` refuses it,
    /// whoever calls; it is never read off the caller's own session.
    async fn project_home_worktree(&self, project_id: &str) -> Result<PathBuf, String> {
        let _ = project_id;
        Err(NO_LIFECYCLE.to_string())
    }

    /// `RestoreSessionWorktree` for `target`, whose worktree was deleted: the path it was cut at.
    /// The tool has already refused an active target.
    async fn restore_worktree(&self, target: &SessionTarget) -> Result<PathBuf, String> {
        let _ = target;
        Err(NO_LIFECYCLE.to_string())
    }
}

/// The refusal of a port that does none of the lifecycle actions.
pub const NO_LIFECYCLE: &str = "this host marks, deletes and clears nothing";

/// Run one of the five lifecycle tools for `home`.
///
/// Bad arguments, a target or path refusal, the activity gate and a refusal of the port come back
/// as an `is_error` outcome naming what was refused; the role refusal is the daemon's and happens
/// before this is called.
pub async fn execute_project_home_lifecycle_tool(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    if !is_project_home_lifecycle_tool(tool_name) {
        return ToolOutcome::err(format!("unknown project-home lifecycle tool: {tool_name}"));
    }
    let args: Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("invalid args_json: {e}")),
    };
    match tool_name {
        SET_SESSION_MARK => set_session_mark(port, home, args).await,
        DELETE_SESSION => delete_session(port, home, args).await,
        CLEAR_WORKTREE => clear_worktree(port, home, args).await,
        DELETE_WORKTREE => delete_worktree(port, home, args).await,
        RESTORE_WORKTREE => restore_worktree(port, home, args).await,
        _ => ToolOutcome::err(format!("unknown project-home lifecycle tool: {tool_name}")),
    }
}

#[derive(Deserialize)]
struct SessionArgs {
    session_id: String,
}

#[derive(Deserialize)]
struct MarkArgs {
    session_id: String,
    mark: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Deserialize)]
struct PathArgs {
    path: String,
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn set_session_mark(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: MarkArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{SET_SESSION_MARK}: invalid arguments: {e}")),
    };
    let mark = match SessionMark::parse(&args.mark) {
        Ok(m) => m,
        Err(e) => return ToolOutcome::err(format!("{SET_SESSION_MARK}: {e}")),
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let at_unix_ms = now_unix_ms();
    let recorded = match mark {
        SessionMark::Mark(kind) => Some(OrchestrationMark {
            mark: kind,
            note: args.note.clone(),
            set_by_session_id: home.session_id.clone(),
            at_unix_ms,
        }),
        SessionMark::None => None,
    };
    if let Err(e) = port.set_session_mark(&target, recorded.as_ref()).await {
        return ToolOutcome::err(format!("{SET_SESSION_MARK}: {e}"));
    }
    if let Some(recorded) = &recorded {
        if recorded.mark == OrchestrationMarkKind::Paused && target.is_active {
            if let Err(e) = port.terminate_session(&target).await {
                return ToolOutcome::err(format!("{SET_SESSION_MARK}: {e:#}"));
            }
        }
    }
    ToolOutcome::ok(
        json!({
            "session_id": target.session_id,
            "mark": mark.as_str(),
            "note": recorded.as_ref().and_then(|m| m.note.clone()),
            "set_by": home.session_id,
            "at_unix_ms": at_unix_ms,
        })
        .to_string(),
    )
}

async fn delete_session(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: SessionArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{DELETE_SESSION}: invalid arguments: {e}")),
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    if target.is_active {
        return ToolOutcome::err(format!(
            "{DELETE_SESSION}: session {} is running: stop it first with TerminateSession",
            target.session_id
        ));
    }
    match port.delete_session(&target).await {
        Ok(()) => ToolOutcome::ok(json!({ "session_id": target.session_id }).to_string()),
        Err(e) => ToolOutcome::err(format!("{DELETE_SESSION}: {e}")),
    }
}

/// Resolve a Clear/Delete `path`: it must be listed, and no active session may own it.
/// Returns the path to act on, or the refusal.
async fn listed_idle_worktree(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    tool_name: &str,
    path: &str,
) -> Result<PathBuf, ToolOutcome> {
    let entries = port
        .list_worktrees(&home.project_id)
        .await
        .map_err(|e| ToolOutcome::err(format!("{tool_name}: {e:#}")))?;
    let Some(entry) = entries
        .into_iter()
        .find(|entry| Path::new(&entry.path) == Path::new(path))
    else {
        return Err(ToolOutcome::err(format!(
            "{tool_name}: {path} is not a worktree of this project"
        )));
    };
    let records = port.target_records().await.map_err(|e| {
        ToolOutcome::err(format!(
            "{tool_name}: cannot read the daemon's sessions: {e:#}"
        ))
    })?;
    if let Some(owner) = records.into_iter().find(|record| {
        record.project_id == home.project_id
            && record.worktree.as_deref() == Some(Path::new(&entry.path))
            && record.is_active
    }) {
        return Err(ToolOutcome::err(format!(
            "{tool_name}: session {} is active in this worktree",
            owner.session_id
        )));
    }
    Ok(PathBuf::from(entry.path))
}

async fn clear_worktree(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: PathArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{CLEAR_WORKTREE}: invalid arguments: {e}")),
    };
    let path = match listed_idle_worktree(port, home, CLEAR_WORKTREE, &args.path).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    match port.clear_worktree(&home.project_id, &path).await {
        Ok(()) => ToolOutcome::ok(json!({ "path": path.display().to_string() }).to_string()),
        Err(e) => ToolOutcome::err(format!("{CLEAR_WORKTREE}: {e}")),
    }
}

async fn delete_worktree(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: PathArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{DELETE_WORKTREE}: invalid arguments: {e}")),
    };
    let path = match listed_idle_worktree(port, home, DELETE_WORKTREE, &args.path).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    match port.project_home_worktree(&home.project_id).await {
        Ok(home_worktree) if paths_equal(&home_worktree, &path) => {
            return ToolOutcome::err(format!(
                "{DELETE_WORKTREE}: {} is the project's home worktree",
                path.display()
            ));
        }
        Ok(_) => {}
        Err(e) => return ToolOutcome::err(format!("{DELETE_WORKTREE}: {e}")),
    }
    match port.delete_worktree(&home.project_id, &path).await {
        Ok(()) => ToolOutcome::ok(json!({ "path": path.display().to_string() }).to_string()),
        Err(e) => ToolOutcome::err(format!("{DELETE_WORKTREE}: {e}")),
    }
}

/// Whether two paths name the same worktree, allowing for a symlinked spelling of either.
fn paths_equal(a: &Path, b: &Path) -> bool {
    a == b
        || match (a.canonicalize(), b.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
}

async fn restore_worktree(
    port: &dyn ProjectHomeLifecyclePort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: SessionArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{RESTORE_WORKTREE}: invalid arguments: {e}")),
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    if target.is_active {
        return ToolOutcome::err(format!(
            "{RESTORE_WORKTREE}: session {} is active",
            target.session_id
        ));
    }
    match port.restore_worktree(&target).await {
        Ok(path) => ToolOutcome::ok(
            json!({
                "session_id": target.session_id,
                "worktree_path": path.display().to_string(),
            })
            .to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("{RESTORE_WORKTREE}: {e}")),
    }
}
