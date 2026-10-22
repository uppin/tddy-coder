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
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-session-tools.md.

use crate::catalog::ToolDef;

pub const LIST_PROJECT_SESSIONS: &str = "ListProjectSessions";
pub const START_PROJECT_SESSION: &str = "StartProjectSession";
pub const READ_SESSION_WORKTREE: &str = "ReadSessionWorktree";
pub const DIFF_SESSION_WORKTREE: &str = "DiffSessionWorktree";

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
}

/// What the tools need from the daemon.
pub trait ProjectHomePort: Send + Sync {
    /// Every session of `project_id` except `home_session_id`.
    fn list_sessions(
        &self,
        project_id: &str,
        home_session_id: &str,
    ) -> anyhow::Result<Vec<ListedSession>>;
    /// Start a session in `project_id` with `home_session_id` as its orchestrator; its id.
    fn start_session(
        &self,
        project_id: &str,
        home_session_id: &str,
        args: StartProjectSessionArgs,
    ) -> anyhow::Result<String>;
}

/// The four tools' definitions, as advertised to a project-home session.
pub fn project_home_tool_defs() -> Vec<ToolDef> {
    // TODO(home-session-tools): implement
    Vec::new()
}
