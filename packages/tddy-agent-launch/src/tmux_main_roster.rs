//! The main terminal's tmux run in the session's tab roster (`#session-liveness` 2/4): recorded
//! when the main starts in tmux, and read when a re-attach asks whether the main ran there.
//!
//! The roster is `tabs.yaml` beside `.session.yaml`; `.session.yaml`'s `tmux_session` stays the
//! main's record for daemons that predate the roster, which is why a session without a roster entry
//! for its main is still a tmux main when `.session.yaml` says so.

use std::path::Path;

use tddy_cli_sessions::cli_session_manager::{CliSessionManager, MAIN_TERMINAL_ID};
use tddy_core::tab_roster::{read_tab_roster, LaunchNode, TabEntry, HARNESS_PROPERTY, TMUX_KIND};
use tddy_core::SessionMetadata;

/// Record that `session_dir`'s main runs agent `harness` in tmux session `tmux_session`, as
/// `os_user`. The main's conversation is `.session.yaml`'s to say, so the agent node names the
/// harness only.
pub(crate) async fn record_tmux_main(
    manager: &CliSessionManager,
    session_dir: &Path,
    harness: &str,
    tmux_session: &str,
    os_user: &str,
) -> anyhow::Result<()> {
    let main = TabEntry::new(
        MAIN_TERMINAL_ID,
        vec![
            LaunchNode::tmux(tmux_session, os_user),
            LaunchNode::new(
                tddy_core::tab_roster::AGENT_KIND,
                [(HARNESS_PROPERTY, harness)],
            ),
        ],
    );
    manager.tab_rosters().record(session_dir, main).await
}

/// Whether the session in `session_dir` ran its main in tmux: its roster says so, or — for a
/// session recorded before rosters existed — its `.session.yaml` names a tmux session.
pub(crate) fn main_ran_in_tmux(
    session_dir: &Path,
    metadata: &SessionMetadata,
) -> anyhow::Result<bool> {
    let in_roster = read_tab_roster(session_dir)?.tabs.iter().any(|tab| {
        tab.terminal_id == MAIN_TERMINAL_ID && tab.launch.iter().any(|node| node.kind == TMUX_KIND)
    });
    Ok(in_roster || metadata.tmux_session.is_some())
}
