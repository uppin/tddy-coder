//! Acceptance tests: every terminal tab a session opens is recorded in its tab roster
//! (`<session dir>/tabs.yaml`) as the launch chain it was started through, under its terminal id; a
//! closed tab leaves the roster, and a tab that exits on its own stays in it.
//!
//! PRD: docs/ft/daemon/session-liveness.md (`#session-liveness` 2/4)
//!
//! Driven through the daemon's own `StartTerminalSession` / `StopTerminalSession`; the roster is
//! read back from disk. Serial — the tmux-attach tab needs the process-wide private tmux server.

mod support;

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::{LaunchNode, CONVERSATION_ID_PROPERTY, HARNESS_PROPERTY};

use support::tab_daemon::{a_tab_daemon, kinds_of};
use support::this_os_user;

#[tokio::test]
#[serial_test::serial]
async fn opening_tabs_records_each_with_its_launch_chain() {
    // Given a session, and a tmux session of the user's own to attach a tab to
    let daemon = a_tab_daemon();
    daemon.tmux.tmux(&["new-session", "-d", "-s", "work"]);

    // When an agent tab, a shell and a tmux-attach tab are opened
    let agent = daemon.opens_a_tab("muse-cli").await;
    let shell = daemon.opens_a_tab("bash").await;
    let attach = daemon.opens_a_tmux_attach_tab("work").await;

    // Then each is recorded under its terminal id with the chain it was launched through
    let agent_entry = daemon.roster_entry(&agent);
    assert_eq!(kinds_of(&agent_entry.launch), vec!["agent"]);
    assert_eq!(
        agent_entry.launch[0].property(HARNESS_PROPERTY),
        Some("muse-cli")
    );
    assert!(
        agent_entry.launch[0]
            .property(CONVERSATION_ID_PROPERTY)
            .is_some_and(|id| !id.is_empty()),
        "an agent tab records the conversation it holds: {agent_entry:?}"
    );
    assert_eq!(
        daemon.roster_entry(&shell).launch,
        vec![LaunchNode::shell()]
    );
    assert_eq!(
        daemon.roster_entry(&attach).launch,
        vec![LaunchNode::tmux_attach("work", &this_os_user())]
    );
}

#[tokio::test]
#[serial_test::serial]
async fn closing_a_tab_removes_it_from_the_roster() {
    // Given a session with two shells open
    let daemon = a_tab_daemon();
    let closed = daemon.opens_a_tab("bash").await;
    let kept = daemon.opens_a_tab("bash").await;

    // When one is closed
    daemon.closes_the_tab(&closed).await;

    // Then only the other is still recorded
    assert!(
        !daemon.roster_records(&closed),
        "a closed tab leaves the roster"
    );
    assert!(daemon.roster_records(&kept), "the open tab stays recorded");
}

#[tokio::test]
#[serial_test::serial]
async fn a_tab_that_exits_on_its_own_stays_in_the_roster() {
    // Given a session with an agent tab open
    let daemon = a_tab_daemon();
    let tab = daemon.opens_a_tab("muse-cli").await;

    // When its agent exits on its own
    daemon.muse_tabs_exit();
    daemon.until_the_tab_is_gone(&tab).await;

    // Then the tab is still recorded, so Resume can bring it back
    assert_eq!(kinds_of(&daemon.roster_entry(&tab).launch), vec!["agent"]);
}
