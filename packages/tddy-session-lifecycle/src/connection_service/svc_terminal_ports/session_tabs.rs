//! A session's tabs beside its main, as the session's end sees them — a close or a delete
//! (docs/ft/daemon/session-liveness.md § Closing a session): every tab its roster records, every
//! live terminal and every jailed tab, each ended through the roster that started it, and then
//! checked for having survived.

use std::collections::BTreeMap;

use tddy_cli_sessions::cli_session_manager::MAIN_TERMINAL_ID;
use tddy_cli_sessions::tmux::tmux_session_lives;
use tddy_core::tab_roster::{
    read_tab_roster, LaunchNode, AGENT_KIND, HARNESS_PROPERTY, SESSION_PROPERTY, TMUX_ATTACH_KIND,
};

use super::recorded_tabs::{wrapping_tmux, WrappingTmux};
use super::DaemonSessionHost;

/// One tab of a session beside its main.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionTab {
    pub terminal_id: String,
    /// How the operator knows it: what runs in it, and the tmux session wrapping its agent.
    pub label: String,
    /// The tmux session its chain records wrapping its agent: what must be gone once it ended.
    tmux: Option<WrappingTmux>,
}

impl DaemonSessionHost {
    /// Every tab of `session_id` beside its main — each its roster (`tabs.yaml`) records, each live
    /// terminal, each jailed tab — by terminal id. A roster that cannot be read is the error: a tab
    /// recorded only there (a tmux tab whose daemon restarted holds no terminal for it) would be
    /// neither ended nor checked. A session with no `tabs.yaml` records no tab.
    pub(crate) async fn tabs_beside_main(
        &self,
        session_id: &str,
    ) -> Result<Vec<SessionTab>, String> {
        let mut tabs: BTreeMap<String, SessionTab> = BTreeMap::new();
        let dir = self.session_dir_of(session_id)?;
        let recorded = read_tab_roster(&dir).map_err(|error| {
            format!(
                "the tab roster of session {session_id} cannot be read, so not every tab of it can \
                 be ended: {error:#}"
            )
        })?;
        for entry in recorded.tabs {
            tabs.insert(
                entry.terminal_id.clone(),
                SessionTab {
                    label: label_of_chain(&entry.launch),
                    tmux: wrapping_tmux(&entry.launch),
                    terminal_id: entry.terminal_id,
                },
            );
        }
        for handle in self.cli_manager.list_terminals(session_id).await {
            tabs.entry(handle.terminal_id.clone())
                .or_insert_with(|| SessionTab {
                    terminal_id: handle.terminal_id.clone(),
                    label: match &handle.tmux_session {
                        Some(name) => format!("{} in tmux {name}", handle.kind),
                        None => handle.kind.clone(),
                    },
                    tmux: None,
                });
        }
        for terminal_id in self.tab_jails.live(session_id).await {
            tabs.entry(terminal_id.clone())
                .or_insert_with(|| SessionTab {
                    terminal_id,
                    label: "jailed tab".to_string(),
                    tmux: None,
                });
        }
        tabs.remove(MAIN_TERMINAL_ID);
        Ok(tabs.into_values().collect())
    }

    /// The directory of `session_id`, which its tab roster is read from.
    pub(crate) fn session_dir_of(&self, session_id: &str) -> Result<std::path::PathBuf, String> {
        self.terminal_roster(self.session_tmux())
            .session_dir(session_id)
            .map_err(|refusal| refusal.message().to_string())
    }

    /// End `tab` of `session_id` with its session: its jail, the tmux session wrapping its agent
    /// (here, or on the session's target as `subject`) and its terminal. It stays in the roster.
    pub(crate) async fn end_tab_with_its_session(
        &self,
        session_id: &str,
        tab: &SessionTab,
        subject: Option<&str>,
    ) -> Result<(), String> {
        self.terminal_roster(self.session_tmux())
            .end_with_its_session(session_id, &tab.terminal_id, tab.tmux.as_ref(), subject)
            .await
    }

    /// Why `tab` of `session_id` survived its end — its terminal or jail still live, or the tmux
    /// session wrapping its agent still alive — or `None` once nothing of it is left. A tmux session
    /// on a target that cannot be asked counts as surviving: nothing proves it gone.
    pub(crate) async fn tab_survives(
        &self,
        session_id: &str,
        tab: &SessionTab,
        subject: Option<&str>,
    ) -> Option<String> {
        if self
            .cli_manager
            .get_terminal(session_id, &tab.terminal_id)
            .await
            .is_some()
        {
            return Some("its terminal is still live".to_string());
        }
        if self
            .tab_jails
            .live(session_id)
            .await
            .contains(&tab.terminal_id)
        {
            return Some("its jail is still running".to_string());
        }
        match tab.tmux.as_ref()? {
            WrappingTmux::Here { name, os_user } => local_tmux_survives(os_user, name).await,
            WrappingTmux::OnTarget { name } => {
                match self
                    .terminal_roster(self.session_tmux())
                    .target_tmux_lives(session_id, name, subject)
                    .await
                {
                    Ok(false) => None,
                    Ok(true) => Some(format!(
                        "its tmux session {name:?} on the target is still alive"
                    )),
                    Err(error) => Some(format!(
                        "whether its tmux session {name:?} on the target ended cannot be told: \
                         {error}"
                    )),
                }
            }
        }
    }
}

impl SessionTab {
    /// This tab with no tmux session on the target to end: a closed session's (its
    /// `worktree-removed` marker), whose close already ended that tmux session and checked it gone.
    pub(crate) fn without_its_target_tmux(self) -> Self {
        match self.tmux {
            Some(WrappingTmux::OnTarget { .. }) => SessionTab { tmux: None, ..self },
            _ => self,
        }
    }
}

/// Why `os_user`'s tmux session `name` on this host survived, or `None` once it is gone. A tmux
/// that cannot be asked counts as surviving: nothing proves the session gone.
pub(crate) async fn local_tmux_survives(os_user: &str, name: &str) -> Option<String> {
    let (user, session) = (os_user.to_string(), name.to_string());
    match tokio::task::spawn_blocking(move || tmux_session_lives(&user, &session)).await {
        Ok(Ok(false)) => None,
        Ok(Ok(true)) => Some(format!("its tmux session {name:?} is still alive")),
        Ok(Err(refusal)) => Some(format!(
            "whether its tmux session {name:?} ended cannot be told: {refusal}"
        )),
        Err(error) => Some(format!(
            "whether its tmux session {name:?} ended cannot be told: the check did not finish: \
             {error}"
        )),
    }
}

/// How the operator knows a recorded tab: its agent's harness (or what else runs in it), and the
/// tmux session wrapping it.
fn label_of_chain(launch: &[LaunchNode]) -> String {
    let runs = launch
        .iter()
        .find(|node| node.kind == AGENT_KIND)
        .and_then(|node| node.property(HARNESS_PROPERTY))
        .map(str::to_string)
        .or_else(|| {
            launch
                .iter()
                .find(|node| node.kind == TMUX_ATTACH_KIND)
                .and_then(|node| node.property(SESSION_PROPERTY))
                .map(|name| format!("tmux {name}"))
        })
        .or_else(|| launch.last().map(|node| node.kind.clone()))
        .unwrap_or_else(|| "tab".to_string());
    match wrapping_tmux(launch) {
        Some(WrappingTmux::Here { name, .. }) => format!("{runs} in tmux {name}"),
        Some(WrappingTmux::OnTarget { name }) => format!("{runs} in tmux {name} on the target"),
        None => runs,
    }
}
