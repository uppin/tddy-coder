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

use async_trait::async_trait;
use serde_json::json;
use tddy_core::orchestration_mark::{OrchestrationMark, OrchestrationMarkKind};
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

use crate::catalog::ToolDef;
use crate::project_home_tools::{
    target_host_property, tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
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
        // TODO(session-lifecycle-tools): implement
        Err(format!(
            "mark {word:?} is not one of paused, blocked, done or none"
        ))
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
        // TODO(session-lifecycle-tools): implement
        let _ = (target, mark);
        Err(NO_LIFECYCLE.to_string())
    }

    /// Delete `target` as `DeleteSession` does — record, worktree and all. The tool has already
    /// refused an active target.
    async fn delete_session(&self, target: &SessionTarget) -> Result<(), String> {
        // TODO(session-lifecycle-tools): implement
        let _ = target;
        Err(NO_LIFECYCLE.to_string())
    }

    /// `CleanWorktree` on the worktree at `path` of `project_id` (`git clean -fdx`).
    async fn clear_worktree(&self, project_id: &str, path: &Path) -> Result<(), String> {
        // TODO(session-lifecycle-tools): implement
        let _ = (project_id, path);
        Err(NO_LIFECYCLE.to_string())
    }

    /// `RemoveWorktree` on the worktree at `path` of `project_id` (`git worktree remove`; the
    /// branch is kept). An error names why it was not removed — uncommitted changes naming
    /// `CommitSessionWorktree` and `StashSessionWorktree`, among them.
    async fn delete_worktree(&self, project_id: &str, path: &Path) -> Result<(), String> {
        // TODO(session-lifecycle-tools): implement
        let _ = (project_id, path);
        Err(NO_LIFECYCLE.to_string())
    }

    /// The project's home worktree on this host — `<worktrees root>/<project_id>/home`, the one
    /// checkout main and every orchestrator of `project_id` work in. `DeleteWorktree` refuses it,
    /// whoever calls; it is never read off the caller's own session.
    async fn project_home_worktree(&self, project_id: &str) -> Result<PathBuf, String> {
        // TODO(session-lifecycle-tools): implement
        let _ = project_id;
        Err(NO_LIFECYCLE.to_string())
    }

    /// `RestoreSessionWorktree` for `target`, whose worktree was deleted: the path it was cut at.
    /// The tool has already refused an active target.
    async fn restore_worktree(&self, target: &SessionTarget) -> Result<PathBuf, String> {
        // TODO(session-lifecycle-tools): implement
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
    // TODO(session-lifecycle-tools): implement — parse the arguments, resolve the session (Mark,
    // DeleteSession, RestoreWorktree) or the listed worktree and the session owning it (Clear,
    // Delete), refuse an active owner and the project's home worktree, call the
    // port (and `terminate_session` for a paused mark on a running session), and answer.
    let _ = (port, home, args_json);
    ToolOutcome::err(format!("{tool_name}: not implemented on this host yet"))
}
