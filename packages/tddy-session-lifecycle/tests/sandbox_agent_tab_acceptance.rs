//! Acceptance tests: a sandboxed session's **agent** tab opened "In sandbox" runs the agent in a jail
//! of its own — a second runner over the session's worktree, with files of its own — in a
//! conversation of its own; it keeps the session live while it runs, writes none of the main's
//! session records, and is stopped with its session.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-21-sandbox-agent-tab.md (`#session-liveness` 7/8)
//!
//! Tab jails start through the in-memory `TabJailSpawn` (`sandbox_tab_support::fake_tab_jails`),
//! injected with `DaemonSessionHost::with_tab_jail_spawn`; the session's main jail is a stand-in
//! runner registered where the daemon resolves jails, and its Claude Code and OpenCode are stub
//! binaries, resolved but never run. Everything else is the daemon's own: the terminal RPCs, the
//! roster, the store, the listing and the stop. What the daemon's own spawn builds for the jail —
//! the agent's exec plan and grants — is `tddy-agent-launch`'s `tab_jail_agent` unit suite.

mod sandbox_tab_support;
mod support;

use std::collections::BTreeSet;

use pretty_assertions::{assert_eq, assert_ne};
use tddy_core::tab_roster::LaunchNode;

use sandbox_tab_support::fake_tab_jails::jailed_agent_banner;
use sandbox_tab_support::{a_sandboxed_session_running_agents, wire_kinds_of, JAILED_SESSION_ID};

fn ids(terminal_ids: &[&str]) -> BTreeSet<String> {
    terminal_ids.iter().map(|id| id.to_string()).collect()
}

// ---------------------------------------------------------------------------
// A jailed agent tab
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_tab_opened_in_sandbox_runs_the_agent_in_a_jail_of_its_own() {
    // Given a sandboxed session whose main jail runs
    let daemon = a_sandboxed_session_running_agents().await;

    // When Claude is opened "In sandbox"
    let tab = daemon.opens_a_jailed_agent("claude-cli").await;

    // Then a jail is started for that tab, running Claude, over the session's worktree, beside the
    // main's ...
    let launch = daemon.jails.launch_of(&tab);
    assert_eq!(
        (
            launch.session_id.as_str(),
            launch.worktree.clone(),
            launch.session_dir.clone(),
            daemon.jails.agent_of(&tab).harness,
        ),
        (
            JAILED_SESSION_ID,
            daemon.worktree.clone(),
            daemon.session_dir(),
            "claude-cli".to_string(),
        )
    );
    assert_eq!(daemon.jails.running_jails(), ids(&[&tab]));
    assert!(daemon.main_jail_runs(), "the main's jail is not the tab's");
    // ... what its jail's agent printed streams on that tab, and it is listed as Claude in a sandbox
    let streamed = daemon
        .until_streamed_output_contains(&tab, &jailed_agent_banner("claude-cli", &tab))
        .await;
    assert_eq!(streamed, jailed_agent_banner("claude-cli", &tab));
    let listed = daemon.listed_terminal(&tab).await;
    assert_eq!(
        (listed.kind.as_str(), wire_kinds_of(&listed)),
        ("claude-cli", vec!["sandbox", "agent"])
    );
}

#[tokio::test]
async fn a_jailed_agent_tab_runs_in_a_conversation_of_its_own() {
    // Given a sandboxed session — a claude-cli main's conversation is its session id
    let daemon = a_sandboxed_session_running_agents().await;

    // When Claude is opened "In sandbox" twice
    let first = daemon.opens_a_jailed_agent("claude-cli").await;
    let second = daemon.opens_a_jailed_agent("claude-cli").await;

    // Then each tab's agent starts a conversation of its own — neither the main's nor the other
    // tab's — and the roster records each tab in a jail, in the conversation it started
    let (one, other) = (
        daemon.jails.agent_of(&first),
        daemon.jails.agent_of(&second),
    );
    assert_eq!((one.resume, other.resume), (false, false));
    assert_eq!(
        BTreeSet::from([
            one.conversation_id.as_str(),
            other.conversation_id.as_str(),
            JAILED_SESSION_ID
        ])
        .len(),
        3,
        "{:?}, {:?} and the main's {JAILED_SESSION_ID:?} must all differ",
        one.conversation_id,
        other.conversation_id
    );
    assert_eq!(
        (
            daemon.roster_entry(&first).launch,
            daemon.roster_entry(&second).launch
        ),
        (
            vec![
                LaunchNode::sandbox(),
                LaunchNode::agent("claude-cli", &one.conversation_id)
            ],
            vec![
                LaunchNode::sandbox(),
                LaunchNode::agent("claude-cli", &other.conversation_id)
            ],
        )
    );
}

#[tokio::test]
async fn a_jailed_agent_tab_keeps_the_session_live() {
    // Given a sandboxed session with Claude open "In sandbox", whose main agent has since exited
    let daemon = a_sandboxed_session_running_agents().await;
    let tab = daemon.opens_a_jailed_agent("claude-cli").await;
    daemon.the_main_jail_exits();

    // When sessions are listed
    let active_while_the_tab_runs = daemon.is_listed_active().await;

    // Then the session is active on its jailed agent tab alone ...
    assert!(
        active_while_the_tab_runs,
        "a live jailed agent tab keeps its session active"
    );
    // ... and is not once that tab's jail exits too
    daemon.jails.the_jail_exits(&tab);
    daemon.until_unlisted(&tab).await;
    assert!(!daemon.is_listed_active().await);
}

#[tokio::test]
async fn a_jailed_agent_tab_does_not_overwrite_the_mains_session_records() {
    // Given a sandboxed session whose main recorded its session and its usage context
    let daemon = a_sandboxed_session_running_agents().await;
    daemon.has_a_main_usage_context();
    let recorded_by_the_main = daemon.session_records();

    // When Claude is opened "In sandbox"
    let tab = daemon.opens_a_jailed_agent("claude-cli").await;

    // Then the session's records are still the main's, byte for byte — the tab's conversation is
    // in the roster, not in them
    assert_eq!(daemon.session_records(), recorded_by_the_main);
    assert_ne!(
        daemon.jails.agent_of(&tab).conversation_id,
        JAILED_SESSION_ID
    );
}

// ---------------------------------------------------------------------------
// A jailed agent tab's end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stopping_the_session_stops_its_jailed_agent_tabs() {
    // Given a sandboxed session with Claude and OpenCode open "In sandbox"
    let daemon = a_sandboxed_session_running_agents().await;
    daemon.opens_a_jailed_agent("claude-cli").await;
    daemon.opens_a_jailed_agent("opencode-cli").await;

    // When the session is stopped, as a stop of its live processes does
    daemon.stops_the_session().await;

    // Then no jail of the session runs — neither tab's nor the main's — and it is no longer active
    assert_eq!(daemon.jails.running_jails(), ids(&[]));
    assert!(!daemon.main_jail_runs());
    assert!(!daemon.is_listed_active().await);
}
