//! Acceptance tests: an agent tab started with a `tmux` wrapper runs in a tmux session of its own as
//! the session's OS user, keeps its session live while it runs — after main exits, and while its
//! client is detached — and is re-attached, not relaunched, when the daemon starts again. A
//! pre-roster tmux main is re-attached at startup from its `.session.yaml`.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-19-tmux-wrapped-tab-roster.md (`#session-liveness` 2/4)
//!
//! A private tmux server stands in for the host's; Muse Code and Claude are stub scripts that log
//! each launch, so "not relaunched" is a launch count that did not move. The restart is a second
//! daemon over the same sessions directory and tmux server, as in
//! `claude_cli_tmux_reattach_acceptance.rs`. Serial — the tmux socket directory is process-wide.

mod support;

use pretty_assertions::assert_eq;
use tddy_cli_sessions::tmux::{agent_tab_tmux_session_name, agent_tmux_session_name};
use tddy_core::tab_roster::{LaunchNode, HARNESS_PROPERTY};
use tddy_rpc::Code;
use tddy_session_lifecycle::claude_cli_session::MAIN_TERMINAL_ID;
use tddy_session_lifecycle::connection_service::ReattachedTab;
use tddy_testing_commons::wait::eventually;

use support::tab_daemon::{a_tab_daemon, is_listed_active_on, kinds_of, TabDaemon, TAB_SESSION_ID};
use support::{this_os_user, until_attached, OBSERVE};

/// Open a Muse Code tab wrapped in tmux, and wait until its agent runs in its tmux session with a
/// client attached. Answers the tab's terminal id and its tmux session.
async fn a_running_tmux_wrapped_muse_tab(daemon: &TabDaemon) -> (String, String) {
    let tab = daemon
        .opens_a_tab_wrapped_in("muse-cli", &["tmux"])
        .await
        .expect("a tmux-wrapped agent tab starts");
    let name = agent_tab_tmux_session_name(TAB_SESSION_ID, &tab);
    eventually(
        "the tab's agent to run in its tmux session",
        OBSERVE,
        || {
            (daemon.tmux.has_session(&name) && daemon.muse_launch_count() == 1)
                .then_some(())
                .ok_or_else(|| {
                    format!(
                        "tmux session {name:?} present: {}; agent launches: {}",
                        daemon.tmux.has_session(&name),
                        daemon.muse_launch_count()
                    )
                })
        },
    )
    .await;
    until_attached(&daemon.tmux, &name, 1).await;
    (tab, name)
}

// ---------------------------------------------------------------------------
// Starting a tmux-wrapped tab
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn an_agent_tab_wrapped_in_tmux_runs_in_its_own_tmux_session() {
    // Given a session with no terminal running
    let daemon = a_tab_daemon();

    // When an agent tab is opened wrapped in tmux
    let (tab, name) = a_running_tmux_wrapped_muse_tab(&daemon).await;

    // Then its agent runs in that tmux session, in the session's worktree, launched once ...
    assert_eq!(daemon.tmux.pane_directory(&name), daemon.worktree);
    assert_eq!(daemon.muse_launch_count(), 1);
    // ... the roster records it as tmux wrapping the agent, as the session's OS user ...
    let entry = daemon.roster_entry(&tab);
    assert_eq!(kinds_of(&entry.launch), vec!["tmux", "agent"]);
    assert_eq!(entry.launch[0], LaunchNode::tmux(&name, &this_os_user()));
    assert_eq!(entry.launch[1].property(HARNESS_PROPERTY), Some("muse-cli"));
    // ... and the terminal listing reports the same chain
    let listed = daemon
        .listed_terminals()
        .await
        .into_iter()
        .find(|terminal| terminal.terminal_id == tab)
        .expect("the tab is listed");
    let listed_kinds: Vec<&str> = listed
        .launch
        .iter()
        .map(|node| node.kind.as_str())
        .collect();
    assert_eq!(listed_kinds, vec!["tmux", "agent"]);
}

#[tokio::test]
#[serial_test::serial]
async fn a_tmux_wrapped_tab_keeps_its_session_live_after_main_exits() {
    // Given a session whose main agent exits while a tmux-wrapped agent tab runs
    let daemon = a_tab_daemon()
        .with_a_main_agent_that_exits_on_release()
        .await;
    a_running_tmux_wrapped_muse_tab(&daemon).await;
    daemon.the_main_agent_exits().await;

    // When the sessions are listed
    let active = daemon.is_listed_active().await;

    // Then the session is still active
    assert!(
        active,
        "a live tmux-wrapped agent tab keeps its session active"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_detached_tmux_wrapped_tab_stays_live_until_its_tmux_session_ends() {
    // Given a tmux-wrapped agent tab whose client was detached, its agent still running
    let daemon = a_tab_daemon();
    let (tab, name) = a_running_tmux_wrapped_muse_tab(&daemon).await;
    assert!(daemon
        .tmux
        .tmux(&["detach-client", "-s", &name])
        .status
        .success());
    daemon.until_the_tab_is_gone(&tab).await;
    assert!(
        daemon.is_listed_active().await,
        "a detached tmux-wrapped agent still keeps its session active"
    );

    // When its tmux session ends
    assert!(daemon
        .tmux
        .tmux(&["kill-session", "-t", &name])
        .status
        .success());

    // Then the session stops being active
    tddy_testing_commons::wait::eventually_awaiting(
        "the session to stop being active",
        OBSERVE,
        || async {
            match daemon.is_listed_active().await {
                false => Ok(()),
                true => Err("the session is still listed active".to_string()),
            }
        },
    )
    .await;
}

#[tokio::test]
#[serial_test::serial]
async fn a_wrapper_other_than_tmux_is_refused_naming_it() {
    // Given a session
    let daemon = a_tab_daemon();

    // When an agent tab is opened wrapped in ssh
    let refusal = daemon
        .opens_a_tab_wrapped_in("muse-cli", &["ssh"])
        .await
        .expect_err("only a tmux wrapper is accepted");

    // Then the start is refused naming the wrapper, and nothing was launched
    assert_eq!(refusal.code, Code::InvalidArgument);
    assert!(
        refusal.message.contains("ssh"),
        "the refusal names the wrapper kind: {}",
        refusal.message
    );
    assert_eq!(daemon.muse_launch_count(), 0);
    assert!(daemon
        .manager
        .list_terminals(TAB_SESSION_ID)
        .await
        .is_empty());
}

// ---------------------------------------------------------------------------
// Startup re-attach
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn the_daemon_reattaches_live_tmux_wrapped_tabs_at_startup() {
    // Given a tmux-wrapped agent tab whose tmux session outlived its daemon
    let daemon = a_tab_daemon();
    let (tab, name) = a_running_tmux_wrapped_muse_tab(&daemon).await;
    daemon.host.shut_down_children().await;
    assert!(
        daemon.tmux.has_session(&name),
        "the tab's tmux session survives"
    );
    let fresh = daemon.restarted();

    // When the fresh daemon re-attaches at startup
    let report = fresh.reattach_live_tmux_tabs().await;

    // Then the tab is re-attached, its session lists active without any client opening it, and its
    // agent was not relaunched
    assert_eq!(
        report.reattached,
        vec![ReattachedTab {
            session_id: TAB_SESSION_ID.to_string(),
            terminal_id: tab,
            tmux_session: name.clone(),
        }]
    );
    assert!(
        is_listed_active_on(&fresh).await,
        "a re-attached tmux-wrapped tab makes its session active"
    );
    assert_eq!(daemon.muse_launch_count(), 1, "the agent is not relaunched");
    until_attached(&daemon.tmux, &name, 1).await;
}

#[tokio::test]
#[serial_test::serial]
async fn the_daemon_reattaches_a_pre_roster_tmux_main_at_startup() {
    // Given a session left by a daemon that predates tab rosters: its main ran in tmux, recorded
    // only in `.session.yaml`, and its tmux session is alive
    let daemon = a_tab_daemon();
    let name = agent_tmux_session_name(TAB_SESSION_ID);
    daemon.runs_claude_in_tmux_session(&name).await;
    daemon.records_a_pre_roster_tmux_main(&name);
    let fresh = daemon.restarted();

    // When the fresh daemon re-attaches at startup
    let report = fresh.reattach_live_tmux_tabs().await;

    // Then its main is re-attached from the record, the session lists active, and the agent was
    // not relaunched
    assert_eq!(
        report.reattached,
        vec![ReattachedTab {
            session_id: TAB_SESSION_ID.to_string(),
            terminal_id: MAIN_TERMINAL_ID.to_string(),
            tmux_session: name.clone(),
        }]
    );
    assert!(
        is_listed_active_on(&fresh).await,
        "a re-attached tmux main makes its session active"
    );
    assert_eq!(
        daemon.claude_launch_count(),
        1,
        "the agent is not relaunched"
    );
    until_attached(&daemon.tmux, &name, 1).await;
}
