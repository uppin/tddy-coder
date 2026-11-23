//! Eager re-attach at daemon startup: every tmux-wrapped tab whose tmux session outlived the last
//! daemon is attached again, so its session is live without anyone opening it (`#session-liveness`
//! 2/4).
//!
//! Lives in its own module rather than in `connection_service.rs`, which is over its size budget
//! (`packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md`).

use std::path::Path;
use std::sync::Arc;

use tddy_agent_launch::claude_cli_tmux::agent_tmux_session_is_alive;
use tddy_cli_sessions::tmux::{agent_tmux_session_name, tmux_session_alive};
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::read_session_metadata;
use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_core::tab_roster::{
    read_tab_roster, TabEntry, AGENT_KIND, HARNESS_PROPERTY, OS_USER_PROPERTY, SESSION_PROPERTY,
    TARGET_KIND, TMUX_KIND,
};

use super::svc_terminal_ports::TmuxMainReattach;
use super::DaemonSessionHost;
use crate::cli_session_manager::{ReattachOutcome, MAIN_TERMINAL_ID};

/// What one startup re-attach did, session by session and tab by tab.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReattachReport {
    /// The tabs attached again to their live tmux sessions.
    pub reattached: Vec<ReattachedTab>,
    /// The tmux-wrapped tabs left as they were, and why (their tmux session is gone, the record
    /// names no OS user, …). Resume brings these back.
    pub left: Vec<LeftTab>,
}

/// A tab attached again to its tmux session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReattachedTab {
    pub session_id: String,
    pub terminal_id: String,
    pub tmux_session: String,
}

/// A tmux-wrapped tab not re-attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftTab {
    pub session_id: String,
    pub terminal_id: String,
    pub reason: String,
}

impl DaemonSessionHost {
    /// Re-attach every tmux-wrapped tab, main or secondary, of every session under this daemon's
    /// sessions bases whose tmux session is alive, as the OS user its `tmux` node records, after
    /// serving that user's agent-tool socket. A session with no tab roster falls back to the main
    /// `tmux_session` its `.session.yaml` records. Nothing is relaunched: a tab whose tmux session
    /// is gone is left for Resume.
    pub async fn reattach_live_tmux_tabs(&self) -> ReattachReport {
        let reattach = Reattach {
            host: self,
            main: TmuxMainReattach::new(
                Arc::clone(&self.cli_manager),
                self.launch_coding_agents(),
                self.tddy_data_dir.clone(),
                Arc::clone(&self.agent_tool_sockets),
            ),
        };
        let mut report = ReattachReport::default();
        for session_id in session_ids_under(&self.tddy_data_dir) {
            reattach.session(&session_id, &mut report).await;
        }
        for tab in &report.reattached {
            log::info!(
                "re-attached terminal {} of session {} to tmux session {:?}",
                tab.terminal_id,
                tab.session_id,
                tab.tmux_session
            );
        }
        for tab in &report.left {
            log::info!(
                "left terminal {} of session {} as it was: {}",
                tab.terminal_id,
                tab.session_id,
                tab.reason
            );
        }
        report
    }
}

/// The ids of the sessions whose directory lies under `tddy_data_dir`.
fn session_ids_under(tddy_data_dir: &Path) -> Vec<String> {
    let dir = tddy_data_dir.join(SESSIONS_SUBDIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            log::warn!(
                "no tmux tab is re-attached: the sessions directory {} cannot be read: {error}",
                dir.display()
            );
            return Vec::new();
        }
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|id| validate_session_id_segment(id).is_ok())
        .collect();
    ids.sort();
    ids
}

/// One startup re-attach: the host, and the single-flight main re-attach it shares with the lazy
/// path.
struct Reattach<'a> {
    host: &'a DaemonSessionHost,
    main: TmuxMainReattach,
}

impl Reattach<'_> {
    /// Re-attach what `session_id`'s roster records as tmux-wrapped, and its pre-roster main.
    async fn session(&self, session_id: &str, report: &mut ReattachReport) {
        let session_dir = unified_session_dir_path(&self.host.tddy_data_dir, session_id);
        let roster = match read_tab_roster(&session_dir) {
            Ok(roster) => roster,
            Err(error) => {
                log::warn!("the tab roster of session {session_id} is not re-attached: {error:#}");
                return;
            }
        };
        let mut main_in_roster = false;
        for tab in &roster.tabs {
            let Some(tmux) = tab.launch.iter().find(|node| node.kind == TMUX_KIND) else {
                continue;
            };
            main_in_roster |= tab.terminal_id == MAIN_TERMINAL_ID;
            // A tab on a target keeps its tmux there, which only a caller's credentials reach: never
            // looked for in this host's tmux, it is left for Resume (`#target-tmux` 2/2).
            if tab.launch.first().map(|node| node.kind.as_str()) == Some(TARGET_KIND) {
                report.leave(
                    session_id,
                    &tab.terminal_id,
                    "it runs in tmux on its target, which Resume re-attaches",
                );
                continue;
            }
            let (Some(os_user), Some(name)) = (
                tmux.property(OS_USER_PROPERTY),
                tmux.property(SESSION_PROPERTY),
            ) else {
                report.leave(
                    session_id,
                    &tab.terminal_id,
                    "its tmux node records no OS user or session",
                );
                continue;
            };
            if tab.terminal_id == MAIN_TERMINAL_ID {
                self.main(session_id, os_user, name, report).await;
            } else {
                self.tab(session_id, tab, os_user, name, &session_dir, report)
                    .await;
            }
        }
        if !main_in_roster {
            self.pre_roster_main(session_id, &session_dir, report).await;
        }
    }

    /// A session left by a daemon that predates tab rosters: its `.session.yaml` records the main's
    /// tmux session, and no OS user — tmux servers are per user, so the user is the configured one
    /// whose tmux holds that session.
    async fn pre_roster_main(
        &self,
        session_id: &str,
        session_dir: &Path,
        report: &mut ReattachReport,
    ) {
        // A target session's tmux runs on its target, never in a tmux server of this host.
        let ran_in_tmux = read_session_metadata(session_dir)
            .ok()
            .is_some_and(|metadata| metadata.tmux_session.is_some() && metadata.target.is_none());
        if !ran_in_tmux {
            return;
        }
        let name = agent_tmux_session_name(session_id);
        match self.os_user_running(session_id).await {
            Some(os_user) => self.main(session_id, &os_user, &name, report).await,
            None => report.leave(
                session_id,
                MAIN_TERMINAL_ID,
                "no configured OS user has its tmux session",
            ),
        }
    }

    /// The configured OS user whose tmux server holds `session_id`'s main tmux session.
    async fn os_user_running(&self, session_id: &str) -> Option<String> {
        let users: std::collections::BTreeSet<String> = self
            .host
            .config
            .users
            .snapshot()
            .into_iter()
            .map(|row| row.os_user)
            .collect();
        for os_user in users {
            if agent_tmux_session_is_alive(&os_user, session_id)
                .await
                .unwrap_or(false)
            {
                return Some(os_user);
            }
        }
        None
    }

    /// Re-attach `session_id`'s main, which ran in tmux session `name` as `os_user`.
    async fn main(&self, session_id: &str, os_user: &str, name: &str, report: &mut ReattachReport) {
        match self.main.attach(os_user, session_id).await {
            ReattachOutcome::Reattached => report.reattach(session_id, MAIN_TERMINAL_ID, name),
            ReattachOutcome::AlreadyAttached => {}
            ReattachOutcome::Skipped(reason) => {
                report.leave(session_id, MAIN_TERMINAL_ID, &reason.to_string())
            }
        }
    }

    /// Re-attach the tmux-wrapped secondary tab `tab`, which runs in tmux session `name` as
    /// `os_user`, to its live tmux session.
    async fn tab(
        &self,
        session_id: &str,
        tab: &TabEntry,
        os_user: &str,
        name: &str,
        session_dir: &Path,
        report: &mut ReattachReport,
    ) {
        let terminal_id = tab.terminal_id.as_str();
        let manager = &self.host.cli_manager;
        if manager
            .get_terminal(session_id, terminal_id)
            .await
            .is_some()
        {
            return;
        }
        let Some(harness) = tab
            .launch
            .iter()
            .find(|node| node.kind == AGENT_KIND)
            .and_then(|node| node.property(HARNESS_PROPERTY))
        else {
            return report.leave(session_id, terminal_id, "its launch chain records no agent");
        };
        // The agent that outlived its daemon reports over its user's socket, which a fresh daemon
        // serves only once asked.
        self.main
            .serve_agent_tool_socket_of(os_user, session_id)
            .await;
        if let Some(why) = why_tmux_session_is_not_alive(os_user, name).await {
            return report.leave(session_id, terminal_id, &why);
        }
        let Some(worktree) = tddy_core::repo_root_for_session(session_dir) else {
            return report.leave(session_id, terminal_id, "the session records no worktree");
        };
        match manager
            .reattach_terminal(session_id, terminal_id, harness, worktree, os_user, name)
            .await
        {
            Ok(_) => report.reattach(session_id, terminal_id, name),
            Err(error) => report.leave(
                session_id,
                terminal_id,
                &format!("the attach failed: {error:#}"),
            ),
        }
    }
}

/// Why `os_user`'s tmux session `name` cannot be attached to — it is gone, or the probe did not
/// finish — and `None` when it is alive. Probed on the blocking pool.
async fn why_tmux_session_is_not_alive(os_user: &str, name: &str) -> Option<String> {
    let (os_user, name) = (os_user.to_string(), name.to_string());
    match tokio::task::spawn_blocking(move || tmux_session_alive(&os_user, &name)).await {
        Ok(true) => None,
        Ok(false) => Some("its tmux session is gone".to_string()),
        Err(error) => Some(format!("its tmux session could not be probed: {error}")),
    }
}

impl ReattachReport {
    fn reattach(&mut self, session_id: &str, terminal_id: &str, tmux_session: &str) {
        self.reattached.push(ReattachedTab {
            session_id: session_id.to_string(),
            terminal_id: terminal_id.to_string(),
            tmux_session: tmux_session.to_string(),
        });
    }

    fn leave(&mut self, session_id: &str, terminal_id: &str, reason: &str) {
        self.left.push(LeftTab {
            session_id: session_id.to_string(),
            terminal_id: terminal_id.to_string(),
            reason: reason.to_string(),
        });
    }
}
