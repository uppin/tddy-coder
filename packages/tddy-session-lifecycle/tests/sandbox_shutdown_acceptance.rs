//! Acceptance test: a daemon shutting down stops every jail it runs — each sandboxed session's main
//! jail, which it used to leave running, and every jailed shell tab's
//! (`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`, part 1).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-shell-tab.md (`#session-liveness` 5/8)
//!
//! The main jail is a stand-in runner registered where the daemon resolves jails; the tab's jail
//! starts through the in-memory `TabJailSpawn` (`sandbox_tab_support`).

mod sandbox_tab_support;
mod support;

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;

use sandbox_tab_support::a_sandboxed_session;

#[tokio::test]
async fn shutting_down_the_daemon_stops_main_and_tab_jails() {
    // Given a sandboxed session whose main jail runs, with a jailed shell tab
    let daemon = a_sandboxed_session().await;
    daemon.opens_a_jailed_shell().await;

    // When the daemon shuts down
    daemon.shuts_down().await;

    // Then neither the main's jail nor the tab's runs
    assert_eq!(
        (daemon.main_jail_runs(), daemon.jails.running_jails()),
        (false, BTreeSet::new())
    );
}
