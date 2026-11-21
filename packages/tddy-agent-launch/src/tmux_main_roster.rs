//! The main terminal's tmux run in the session's tab roster (`#session-liveness` 2/4): recorded
//! when the main starts in tmux, and read when a re-attach asks whether the main ran there.
//!
//! The roster is `tabs.yaml` beside `.session.yaml`; `.session.yaml`'s `tmux_session` stays the
//! main's record for daemons that predate the roster, which is why a session without a roster entry
//! for its main is still a tmux main when `.session.yaml` says so.
//!
//! The main's `tmux` node also keeps when the agent its pane runs started
//! ([`tddy_core::tab_roster::START_PROPERTY`]), which a delete checks before it signals that pid
//! (`#tab-hardening` 2/4).

use std::path::Path;

use tddy_cli_sessions::cli_session_manager::{CliSessionManager, MAIN_TERMINAL_ID};
use tddy_cli_sessions::process_start::ProcessStart;
use tddy_core::tab_roster::{read_tab_roster, LaunchNode, TabEntry, HARNESS_PROPERTY, TMUX_KIND};
use tddy_core::SessionMetadata;

/// Record that `session_dir`'s main runs agent `harness` in tmux session `tmux_session`, as
/// `os_user`, and — when it is known — that the agent its pane runs started at `start`. The main's
/// conversation is `.session.yaml`'s to say, so the agent node names the harness only.
pub(crate) async fn record_tmux_main(
    manager: &CliSessionManager,
    session_dir: &Path,
    harness: &str,
    tmux_session: &str,
    os_user: &str,
    start: Option<ProcessStart>,
) -> anyhow::Result<()> {
    // TODO(tab-hygiene): record `start` on the tmux node as `START_PROPERTY`
    let _ = start;
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

/// When the agent of `session_dir`'s tmux main started, as [`record_tmux_main`] recorded it:
/// `None` for a session whose roster has no tmux main, or whose tmux main was recorded without a
/// start. A start that is not a number of seconds is an error, never read as none.
pub fn recorded_tmux_main_start(session_dir: &Path) -> anyhow::Result<Option<ProcessStart>> {
    let _ = session_dir;
    // TODO(tab-hygiene): implement
    todo!()
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

#[cfg(test)]
mod tests {
    use tddy_core::tab_roster::{write_tab_roster, TabRoster, START_PROPERTY};

    use super::*;

    const TMUX_SESSION: &str = "tddy-019d3a10-7c2e-7b40-9e51-2a6c0f4d8b11";

    /// A session directory whose roster's main runs in tmux, its tmux node carrying `properties`
    /// beyond the session and user.
    fn a_session_whose_tmux_main_records(extra: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut tmux = LaunchNode::tmux(TMUX_SESSION, "dev");
        for (key, value) in extra {
            tmux.properties.insert(key.to_string(), value.to_string());
        }
        let roster = TabRoster {
            tabs: vec![TabEntry::new(MAIN_TERMINAL_ID, vec![tmux])],
            ..Default::default()
        };
        write_tab_roster(dir.path(), &roster).unwrap();
        dir
    }

    #[tokio::test]
    async fn a_tmux_main_recorded_with_its_agents_start_reads_that_start_back() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let manager = CliSessionManager::new();
        record_tmux_main(
            &manager,
            dir.path(),
            "claude-cli",
            TMUX_SESSION,
            "dev",
            Some(ProcessStart::from_epoch_secs(1_790_000_000)),
        )
        .await
        .unwrap();

        // When
        let start = recorded_tmux_main_start(dir.path()).unwrap();

        // Then
        assert_eq!(start, Some(ProcessStart::from_epoch_secs(1_790_000_000)));
    }

    #[test]
    fn the_start_is_the_tmux_nodes_start_property() {
        // Given
        let dir = a_session_whose_tmux_main_records(&[(START_PROPERTY, "1790000000")]);

        // When
        let start = recorded_tmux_main_start(dir.path()).unwrap();

        // Then
        assert_eq!(start, Some(ProcessStart::from_epoch_secs(1_790_000_000)));
    }

    #[test]
    fn a_tmux_main_recorded_without_a_start_has_none() {
        // Given
        let dir = a_session_whose_tmux_main_records(&[]);

        // When
        let start = recorded_tmux_main_start(dir.path()).unwrap();

        // Then
        assert_eq!(start, None);
    }

    #[test]
    fn a_session_without_a_roster_has_no_start() {
        // Given
        let dir = tempfile::tempdir().unwrap();

        // When
        let start = recorded_tmux_main_start(dir.path()).unwrap();

        // Then
        assert_eq!(start, None);
    }

    #[test]
    fn a_start_that_is_not_a_number_of_seconds_is_an_error() {
        // Given
        let dir = a_session_whose_tmux_main_records(&[(START_PROPERTY, "yesterday")]);

        // When
        let read = recorded_tmux_main_start(dir.path());

        // Then
        assert!(read.is_err(), "an unreadable start is not none: {read:?}");
    }
}
