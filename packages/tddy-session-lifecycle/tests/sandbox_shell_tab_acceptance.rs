//! Acceptance tests: a sandboxed session's shell tab opened "In sandbox" runs in a jail of its own —
//! a second runner over the session's worktree, with files of its own — streams, is listed, is
//! recorded `[sandbox, shell]`, and leaves no jail behind when it is closed or its session is
//! deleted; a jail that exits ends its tab, which stays recorded for Resume. An "On host" tab of a sandboxed session is reachable, and a `sandbox` wrapper is refused
//! around an agent and on a session that is not sandboxed.
//!
//! PRD: docs/ft/daemon/session-liveness.md (A sandboxed session's tabs) (`#session-liveness` 5/8)
//!
//! Tab jails start through an in-memory `TabJailSpawn` (`sandbox_tab_support::fake_tab_jails`),
//! injected with `DaemonSessionHost::with_tab_jail_spawn`; the session's main jail is a stand-in
//! runner registered where the daemon resolves jails. Everything else is the daemon's own: the
//! terminal RPCs, the roster, the store and the delete.

mod sandbox_tab_support;
mod support;

use std::collections::BTreeSet;

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::LaunchNode;
use tddy_rpc::Code;

use sandbox_tab_support::fake_tab_jails::jailed_shell_banner;
use support::this_os_user;

use sandbox_tab_support::{
    a_sandboxed_session, an_unsandboxed_session, wire_kinds_of, JAILED_SESSION_ID,
};

fn ids(terminal_ids: &[&str]) -> BTreeSet<String> {
    terminal_ids.iter().map(|id| id.to_string()).collect()
}

// ---------------------------------------------------------------------------
// A jailed shell tab
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_shell_tab_opened_in_sandbox_starts_a_jail_of_its_own_over_the_session_worktree() {
    // Given a sandboxed session whose main jail runs
    let daemon = a_sandboxed_session().await;

    // When a shell tab is opened "In sandbox"
    let tab = daemon.opens_a_jailed_shell().await;

    // Then a jail is started for that tab, over the session's worktree, beside the main's
    let launch = daemon.jails.launch_of(&tab);
    assert_eq!(
        (
            launch.session_id.as_str(),
            launch.terminal_id.as_str(),
            launch.worktree.clone(),
            launch.session_dir.clone(),
        ),
        (
            JAILED_SESSION_ID,
            tab.as_str(),
            daemon.worktree.clone(),
            daemon.session_dir(),
        )
    );
    assert_eq!(daemon.jails.running_jails(), ids(&[&tab]));
    assert!(daemon.main_jail_runs(), "the main's jail is not the tab's");
}

#[tokio::test]
async fn a_jailed_shell_tab_runs_the_session_users_login_shell_as_a_host_tab_does() {
    // Given a sandboxed session, opened by a person the daemon maps to an OS user
    let daemon = a_sandboxed_session().await;

    // When a shell tab is opened "In sandbox"
    let tab = daemon.opens_a_jailed_shell().await;

    // Then its jail is asked to run that user's own login shell, for that user, and to resolve the
    // session's provider route from the vault of the person who opened the tab
    let launch = daemon.jails.launch_of(&tab);
    assert_eq!(
        (
            launch.os_user.clone(),
            launch.login_shell.clone(),
            launch.subject.clone()
        ),
        (
            this_os_user(),
            tddy_terminal_rpc::login_shell_for(&this_os_user()),
            "testuser".to_string(),
        )
    );
}

#[tokio::test]
async fn a_jailed_shell_tab_streams_its_output_and_is_listed() {
    // Given a sandboxed session
    let daemon = a_sandboxed_session().await;

    // When a shell tab is opened "In sandbox"
    let tab = daemon.opens_a_jailed_shell().await;

    // Then what its jail's shell printed streams on that tab ...
    let streamed = daemon
        .until_streamed_output_contains(&tab, &jailed_shell_banner(&tab))
        .await;
    assert_eq!(streamed, jailed_shell_banner(&tab));
    // ... and the tab is listed as a shell run in a sandbox
    let listed = daemon.listed_terminal(&tab).await;
    assert_eq!(
        (listed.kind.as_str(), wire_kinds_of(&listed)),
        ("bash", vec!["sandbox", "shell"])
    );
}

#[tokio::test]
async fn a_jailed_shell_tab_is_recorded_as_sandbox_then_shell() {
    // Given a sandboxed session
    let daemon = a_sandboxed_session().await;

    // When a shell tab is opened "In sandbox"
    let tab = daemon.opens_a_jailed_shell().await;

    // Then the roster records its chain as a jail wrapping the shell
    assert_eq!(
        daemon.roster_entry(&tab).launch,
        vec![LaunchNode::sandbox(), LaunchNode::shell()]
    );
}

#[tokio::test]
async fn two_jailed_tabs_of_one_session_get_distinct_jail_paths() {
    // Given a sandboxed session
    let daemon = a_sandboxed_session().await;

    // When two shell tabs are opened "In sandbox"
    let first = daemon.opens_a_jailed_shell().await;
    let second = daemon.opens_a_jailed_shell().await;

    // Then each jail's files live in its own tab's directory of the session's sandbox ...
    let (one, other) = (
        daemon.jails.launch_of(&first),
        daemon.jails.launch_of(&second),
    );
    assert_eq!(
        (one.paths.root.clone(), other.paths.root.clone()),
        (
            one.session_dir.join("sandbox").join("tabs").join(&first),
            other.session_dir.join("sandbox").join("tabs").join(&second),
        )
    );
    // ... with a tool-IPC socket that is neither the other tab's nor the main's
    let main_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(JAILED_SESSION_ID);
    assert_eq!(
        BTreeSet::from([&one.paths.ipc_socket, &other.paths.ipc_socket, &main_socket]).len(),
        3,
        "{:?}, {:?} and the main's {:?} must all differ",
        one.paths.ipc_socket,
        other.paths.ipc_socket,
        main_socket
    );
    assert_eq!(daemon.jails.running_jails(), ids(&[&first, &second]));
}

// ---------------------------------------------------------------------------
// A jailed tab's end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn closing_a_jailed_tab_stops_its_jail_and_leaves_the_roster() {
    // Given a sandboxed session with a jailed shell tab
    let daemon = a_sandboxed_session().await;
    let tab = daemon.opens_a_jailed_shell().await;

    // When the tab is closed
    daemon.closes_the_tab(&tab).await;

    // Then its jail is stopped, the tab is neither listed nor recorded, and the main's jail runs on
    assert!(!daemon.jails.jail_runs(&tab), "the tab's jail is stopped");
    assert!(!daemon.listed_terminal_ids().await.contains(&tab));
    assert!(!daemon.roster_records(&tab));
    assert!(daemon.main_jail_runs());
}

#[tokio::test]
async fn a_tab_jail_that_exits_ends_its_tab() {
    // Given a sandboxed session with a jailed shell tab
    let daemon = a_sandboxed_session().await;
    let tab = daemon.opens_a_jailed_shell().await;

    // When the tab's jail exits on its own
    daemon.jails.the_jail_exits(&tab);

    // Then the tab ends — it is no longer listed — but stays in the roster as a jailed shell, as a
    // host tab that exits does, so Resume reopens it
    daemon.until_unlisted(&tab).await;
    assert_eq!(
        daemon.roster_entry(&tab).launch,
        vec![LaunchNode::sandbox(), LaunchNode::shell()]
    );
    assert!(
        daemon.main_jail_runs(),
        "the main's jail outlives the tab's"
    );
}

#[tokio::test]
async fn deleting_a_sandboxed_session_stops_every_tab_jail() {
    // Given a sandboxed session with two jailed shell tabs
    let daemon = a_sandboxed_session().await;
    daemon.opens_a_jailed_shell().await;
    daemon.opens_a_jailed_shell().await;

    // When the session is deleted
    daemon.deletes_the_session().await;

    // Then no jail of the session runs: neither tab's nor the main's
    assert_eq!(daemon.jails.running_jails(), ids(&[]));
    assert!(!daemon.main_jail_runs());
}

// ---------------------------------------------------------------------------
// "On host", and what is refused
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_on_host_tab_of_a_sandboxed_session_streams_its_output() {
    // Given a sandboxed session with a shell tab opened "On host"
    let daemon = a_sandboxed_session().await;
    let tab = daemon.opens_a_host_shell().await;

    // When something is typed into it
    daemon.types_into(&tab, "echo on-host-$((40+2))\n").await;

    // Then its output streams — the arithmetic proves it is the shell's answer, not the echo of the
    // typed line — and it is recorded as a plain shell, started in no jail
    daemon
        .until_streamed_output_contains(&tab, "on-host-42")
        .await;
    assert_eq!(daemon.roster_entry(&tab).launch, vec![LaunchNode::shell()]);
    assert_eq!(daemon.jails.launched_terminals(), Vec::<String>::new());
}

#[tokio::test]
async fn a_sandbox_wrapper_around_an_agent_is_refused() {
    // Given a sandboxed session
    let daemon = a_sandboxed_session().await;

    // When Claude is asked for "In sandbox"
    let refused = daemon
        .opens_a_tab_in_sandbox("claude-cli")
        .await
        .expect_err("an agent tab runs in no jail of its own here");

    // Then the start is refused naming the agent, and nothing is started or recorded
    assert_eq!(refused.code(), Code::InvalidArgument);
    assert!(
        refused.message().contains("\"claude-cli\""),
        "{}",
        refused.message()
    );
    assert_eq!(daemon.jails.launched_terminals(), Vec::<String>::new());
    assert_eq!(daemon.roster().tabs, Vec::new());
}

#[tokio::test]
async fn a_sandbox_wrapper_on_an_unsandboxed_session_is_refused() {
    // Given a session that runs on the host
    let daemon = an_unsandboxed_session();

    // When a shell tab is asked for "In sandbox"
    let refused = daemon
        .opens_a_tab_in_sandbox("bash")
        .await
        .expect_err("only a sandboxed session runs tab jails");

    // Then the start is refused as one the session cannot serve, and nothing is started or recorded
    assert_eq!(refused.code(), Code::FailedPrecondition);
    assert_eq!(daemon.jails.launched_terminals(), Vec::<String>::new());
    assert_eq!(daemon.roster().tabs, Vec::new());
}

// ---------------------------------------------------------------------------
// The listing the web's menu reads
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_sandboxed_session_is_listed_as_sandboxed() {
    // Given a sandboxed session
    let daemon = a_sandboxed_session().await;

    // When sessions are listed
    let sandboxed = daemon.is_listed_sandboxed().await;

    // Then the listing says so, which is what offers "In sandbox" in its new-tab menu
    assert!(sandboxed);
}
