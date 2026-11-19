//! Restoring a session's **tab roster** on Resume (`#session-liveness` 4/4): after main's resume,
//! every other entry of `tabs.yaml` ([`tddy_core::tab_roster`]) is brought back under its recorded
//! terminal id, each as its launch chain allows.
//!
//! The rule per entry is [`restore_plan`] — pure, over the entry and a [`LiveTabs`] snapshot — and
//! [`restore_tab_roster`] carries it out, one [`RestoredTab`] per entry. A tab that cannot be restored
//! is dropped from the roster and reported; it never fails the resume.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-19-resume-restores-tab-roster.md

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_cli_sessions::tab_roster_store::TabRosterStore;
use tddy_coding_agent_specs::{CodingAgentSpec, SpecError};
use tddy_core::tab_roster::TabEntry;
use tddy_service::proto::session::RestoredTab;

use crate::spec_launch::HostCodingAgents;

/// An [`AGENT_KIND`](tddy_core::tab_roster::AGENT_KIND) node's resumability: `"false"` on a tab
/// whose harness cannot resume a given conversation (opencode and muse `--continue` the worktree's
/// last one; warp needs an exit-hook token a tab does not capture). Absent means resumable.
pub const RESUMABLE_PROPERTY: &str = "resumable";

/// What a resume does with one roster entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabRestore {
    /// The tab's terminal is already live: it is not restarted.
    LeaveLive,
    /// An agent tab started under its id, resuming its own conversation.
    ResumeAgent {
        harness: String,
        conversation_id: String,
    },
    /// An agent tab whose harness cannot resume a given conversation, started fresh under its id.
    FreshAgent { harness: String },
    /// A tmux-wrapped tab whose tmux session lives: a client re-attached under its id; the agent in
    /// the tmux session is not touched.
    ReattachTmux { session: String, os_user: String },
    /// A tmux-wrapped agent tab whose tmux session is gone: the agent relaunched in a new tmux
    /// session, resuming `resume_conversation` when it is resumable, fresh when it is `None`.
    RelaunchTmux {
        session: String,
        os_user: String,
        harness: String,
        resume_conversation: Option<String>,
    },
    /// The session user's login shell, reopened fresh under its id.
    FreshShell,
    /// A tab attached to the user's own tmux session, which still exists: re-attached under its id.
    ReattachTmuxAttach { session: String, os_user: String },
    /// Not restorable: dropped from the roster and reported with `reason`.
    Drop { reason: String },
}

/// What is live when the roster is restored: the session's served terminals, and the tmux sessions
/// on each user's tmux server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveTabs {
    /// The terminal ids the daemon serves for the session.
    pub terminals: BTreeSet<String>,
    /// `(os_user, tmux session name)` of every tmux session that exists.
    pub tmux_sessions: BTreeSet<(String, String)>,
}

impl LiveTabs {
    /// Nothing live.
    pub fn none() -> Self {
        Self::default()
    }

    /// With `terminal_id` served.
    pub fn with_terminal(mut self, terminal_id: &str) -> Self {
        self.terminals.insert(terminal_id.to_string());
        self
    }

    /// With the tmux session `session` existing on `os_user`'s tmux server.
    pub fn with_tmux_session(mut self, os_user: &str, session: &str) -> Self {
        self.tmux_sessions
            .insert((os_user.to_string(), session.to_string()));
        self
    }

    pub fn terminal_is_live(&self, terminal_id: &str) -> bool {
        self.terminals.contains(terminal_id)
    }

    pub fn tmux_session_exists(&self, os_user: &str, session: &str) -> bool {
        self.tmux_sessions
            .contains(&(os_user.to_string(), session.to_string()))
    }
}

/// What a resume does with `entry`, given what is `live`:
///
/// - a live terminal → [`TabRestore::LeaveLive`];
/// - `agent` → [`TabRestore::ResumeAgent`], or [`TabRestore::FreshAgent`] when marked
///   `resumable: false`;
/// - `tmux` → `agent` → [`TabRestore::ReattachTmux`] when the tmux session exists, otherwise
///   [`TabRestore::RelaunchTmux`];
/// - `shell` → [`TabRestore::FreshShell`];
/// - `tmux-attach` → [`TabRestore::ReattachTmuxAttach`] when the user's tmux session exists,
///   otherwise [`TabRestore::Drop`] naming it;
/// - an empty chain, an unknown kind, or a node missing what it needs → [`TabRestore::Drop`]
///   naming why.
pub fn restore_plan(entry: &TabEntry, live: &LiveTabs) -> TabRestore {
    // TODO(session-liveness 4/4): implemented in the green step.
    todo!(
        "the restore of tab {} ({} launch node(s)) against {} live terminal(s)",
        entry.terminal_id,
        entry.launch.len(),
        live.terminals.len()
    )
}

/// What [`restore_tab_roster`] restores, and with what.
pub struct RosterRestore<'a> {
    pub session_id: &'a str,
    /// The session directory holding `tabs.yaml`.
    pub session_dir: &'a Path,
    /// The worktree every restored tab runs in: main's.
    pub worktree: &'a Path,
    /// The session's OS user, as main was resumed as.
    pub os_user: &'a str,
    pub cli_manager: &'a Arc<CliSessionManager>,
    pub coding_agents: &'a HostCodingAgents,
    /// The roster's single writer: a dropped tab is forgotten through it.
    pub roster_store: &'a TabRosterStore,
}

/// Restore every entry of the session's roster other than main, in roster order, each under its
/// recorded terminal id as [`restore_plan`] decides, and report each. A cursor agent tab resumes the
/// chat it was minted; an agent tab started fresh is reported `FRESH` with the reason; a tab that
/// cannot be restored is forgotten from the roster and reported `DROPPED` — the resume itself goes
/// on.
pub async fn restore_tab_roster(restore: RosterRestore<'_>) -> Vec<RestoredTab> {
    // TODO(session-liveness 4/4): implemented in the green step.
    let _ = (
        restore.cli_manager,
        restore.coding_agents,
        restore.roster_store,
    );
    todo!(
        "restore the tab roster of {} in {} (worktree {}) as {}",
        restore.session_id,
        restore.session_dir.display(),
        restore.worktree.display(),
        restore.os_user
    )
}

/// The argv of an agent tab of `spec` holding `conversation_id`, run as `binary`: `resume` set,
/// it continues that conversation (claude `--resume <id>`, cursor `--resume <chat id>`); unset, it
/// starts it (claude `--session-id <id>`).
pub fn agent_tab_argv(
    spec: &CodingAgentSpec,
    binary: &Path,
    conversation_id: &str,
    resume: bool,
) -> Result<Vec<String>, SpecError> {
    // TODO(session-liveness 4/4): implemented in the green step.
    todo!(
        "the argv of a {} tab in conversation {conversation_id} run as {} (resume: {resume})",
        spec.id,
        binary.display()
    )
}

#[cfg(test)]
#[path = "restore_tabs_tests.rs"]
mod tests;
