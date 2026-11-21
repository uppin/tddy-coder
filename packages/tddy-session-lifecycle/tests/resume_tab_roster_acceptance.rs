//! Acceptance tests: `ResumeSession` brings back every tab of the session's roster (`tabs.yaml`), not
//! only main, each under its recorded terminal id — an agent tab resuming its own conversation where
//! its harness can, a tmux-wrapped tab re-attached or relaunched, a shell fresh, a `tmux-attach` tab
//! re-attached or dropped — and reports each in `ResumeSessionResponse.restored_tabs`.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-19-resume-restores-tab-roster.md (`#session-liveness` 4/4)
//!
//! Each roster is arranged as an earlier daemon run left it — written to `tabs.yaml`, or recorded by
//! opening tabs through `StartTerminalSession` and then restarting the daemon — and the session is
//! resumed through `SessionService::resume_session` on `DaemonSessionHost`. What came back is read
//! from `ListTerminalSessions`, the stub harnesses' launch logs, the private tmux server and the
//! response. Serial — the tmux socket directory is process-wide.

mod resume_roster_support;
mod sandbox_tab_support;
mod support;
mod target_tab_support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_agent_launch::restore_tabs::RESUMABLE_PROPERTY;
use tddy_core::tab_roster::{
    LaunchNode, TabEntry, CONVERSATION_ID_PROPERTY, SESSION_PROPERTY, TMUX_KIND,
};
use tddy_service::proto::session::{RestoredTab, RestoredTabOutcome};
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;

use resume_roster_support::{
    a_stopped_session, a_stopped_session_whose_tab_jails_start_through,
    a_stopped_session_whose_target_shells_resolve_through, MINTED_CHAT_ID, RESUMED_SESSION_ID,
};
use sandbox_tab_support::fake_tab_jails::{a_tab_jail_spawn, a_tab_jail_spawn_refusing};
use support::{this_os_user, until_attached};
use target_tab_support::fake_target_shells::{
    target_shells_reaching_a_machine, target_shells_reaching_an_unreachable_machine,
    target_shells_refusing, UNREACHABLE,
};
use target_tab_support::{where_the_login_shell_started, OWNER};
use tddy_agent_launch::tab_target_shell::TabTargetShell;
use tddy_daemon_sandbox::tab_jails::TabJailSpawn;
use tddy_rpc::Status;

/// The conversation main resumes: a claude-cli main's is its session id.
const MAIN_CONVERSATION: &str = RESUMED_SESSION_ID;
/// A second Claude tab, and the conversation it holds.
const CLAUDE_TAB: &str = "019b0000-0000-7000-8000-00000000c1a0";
const CLAUDE_TAB_CONVERSATION: &str = "5f3c0000-0000-7000-8000-00000000c1a0";
/// A shell tab.
const SHELL_TAB: &str = "019b0000-0000-7000-8000-0000000005e1";
/// A tmux-wrapped Claude tab, the tmux session it runs in, and its conversation.
const TMUX_TAB: &str = "019b0000-0000-7000-8000-00000000712a";
const TMUX_TAB_SESSION: &str = "tddy-resume-roster-session-tmux-tab";
const TMUX_TAB_CONVERSATION: &str = "5f3c0000-0000-7000-8000-00000000712a";
/// A tab attached to the user's own tmux session `work`.
const ATTACH_TAB: &str = "019b0000-0000-7000-8000-00000000a77a";
/// A shell tab run in a jail of its own (`#session-liveness` 5/8).
const JAILED_TAB: &str = "019b0000-0000-7000-8000-0000000a11ed";
/// A shell tab run on the session's target (`#session-liveness` 6/8).
const TARGET_TAB: &str = "019b0000-0000-7000-8000-0000000a7a26";
/// Why the target of a Coder session cannot be resolved while its owner's vault is locked, as the
/// daemon's target shells refuse it.
const VAULT_LOCKED: &str =
    "the vault holding this session's Coder account is locked: unlock it and try again";
/// Why the jail of [`JAILED_TAB`] cannot start, when a test says it cannot.
const JAIL_REFUSAL: &str = "the session's sandbox grants cannot be lowered: no claude tool";

fn the_main_tab() -> TabEntry {
    TabEntry::new(
        MAIN_TERMINAL_ID,
        vec![LaunchNode::agent("claude-cli", MAIN_CONVERSATION)],
    )
}

fn a_claude_tab() -> TabEntry {
    TabEntry::new(
        CLAUDE_TAB,
        vec![LaunchNode::agent("claude-cli", CLAUDE_TAB_CONVERSATION)],
    )
}

fn a_tmux_wrapped_claude_tab() -> TabEntry {
    TabEntry::new(
        TMUX_TAB,
        vec![
            LaunchNode::tmux(TMUX_TAB_SESSION, &this_os_user()),
            LaunchNode::agent("claude-cli", TMUX_TAB_CONVERSATION),
        ],
    )
}

/// A shell tab recorded in a jail of its own.
fn a_jailed_shell_tab() -> TabEntry {
    TabEntry::new(JAILED_TAB, vec![LaunchNode::sandbox(), LaunchNode::shell()])
}

/// A shell tab recorded on the session's target.
fn a_target_shell_tab() -> TabEntry {
    TabEntry::new(TARGET_TAB, vec![LaunchNode::target(), LaunchNode::shell()])
}

fn restored(terminal_id: &str) -> RestoredTab {
    RestoredTab {
        terminal_id: terminal_id.to_string(),
        outcome: RestoredTabOutcome::Restored as i32,
        reason: String::new(),
    }
}

/// `(terminal_id, outcome)` of each reported tab, in report order.
fn outcomes_of(tabs: &[RestoredTab]) -> Vec<(String, RestoredTabOutcome)> {
    tabs.iter()
        .map(|tab| {
            (
                tab.terminal_id.clone(),
                RestoredTabOutcome::try_from(tab.outcome).expect("a known outcome"),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Agent tabs and shells
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn resume_restores_every_roster_tab_under_its_recorded_id() {
    // Given a stopped session whose roster holds main, a second Claude tab and a shell
    let daemon = a_stopped_session();
    daemon.has_recorded_tabs(vec![
        the_main_tab(),
        a_claude_tab(),
        TabEntry::new(SHELL_TAB, vec![LaunchNode::shell()]),
    ]);

    // When it is resumed
    let response = daemon.resumes().await;

    // Then all three are back under their recorded ids, and each tab but main is reported
    daemon
        .until_listed_terminals_are(&[MAIN_TERMINAL_ID, CLAUDE_TAB, SHELL_TAB])
        .await;
    let reported: Vec<&str> = response
        .restored_tabs
        .iter()
        .map(|tab| tab.terminal_id.as_str())
        .collect();
    assert_eq!(reported, vec![CLAUDE_TAB, SHELL_TAB]);
}

#[tokio::test]
#[serial_test::serial]
async fn a_claude_tab_resumes_its_own_conversation() {
    // Given a stopped session with a second Claude tab in its roster
    let daemon = a_stopped_session();
    daemon.has_recorded_tabs(vec![the_main_tab(), a_claude_tab()]);

    // When it is resumed
    let response = daemon.resumes().await;

    // Then that tab's Claude is launched resuming the tab's conversation, never main's
    daemon
        .claude
        .until_launched_with("--resume", CLAUDE_TAB_CONVERSATION)
        .await;
    assert_eq!(response.restored_tabs, vec![restored(CLAUDE_TAB)]);
}

#[tokio::test]
#[serial_test::serial]
async fn a_cursor_tab_resumes_its_minted_chat() {
    // Given a Cursor tab opened on the session, which mints a chat of its own, and a daemon restart
    let daemon = a_stopped_session();
    let tab = daemon.opens_a_tab("cursor-cli").await;
    daemon
        .cursor
        .until_launched_with("--resume", MINTED_CHAT_ID)
        .await;
    assert_eq!(daemon.minted_chats(), 1, "the tab minted one chat");
    assert_eq!(
        daemon.roster_entry(&tab).launch[0].property(CONVERSATION_ID_PROPERTY),
        Some(MINTED_CHAT_ID),
        "the roster records the minted chat as the tab's conversation"
    );
    let daemon = daemon.restarts().await;

    // When the session is resumed
    let response = daemon.resumes().await;

    // Then the tab's Cursor is launched again into that chat, without minting another
    let relaunch = daemon.cursor.until_launched_times(2).await;
    assert!(
        relaunch
            .windows(2)
            .any(|pair| pair == ["--resume", MINTED_CHAT_ID]),
        "{relaunch:?}"
    );
    assert_eq!(daemon.minted_chats(), 1, "no second chat was minted");
    assert_eq!(response.restored_tabs, vec![restored(&tab)]);
}

#[tokio::test]
#[serial_test::serial]
async fn an_opencode_tab_reopens_fresh_and_is_reported_fresh() {
    // Given an OpenCode tab opened on the session, and a daemon restart
    let daemon = a_stopped_session();
    let tab = daemon.opens_a_tab("opencode-cli").await;
    daemon.opencode.until_launched_times(1).await;
    assert_eq!(
        daemon.roster_entry(&tab).launch[0].property(RESUMABLE_PROPERTY),
        Some("false"),
        "an OpenCode tab is recorded as one its harness cannot resume"
    );
    let daemon = daemon.restarts().await;

    // When the session is resumed
    let response = daemon.resumes().await;

    // Then the tab is back under its id, its OpenCode started fresh rather than `--continue`d, and it
    // is reported fresh with the reason
    let relaunch = daemon.opencode.until_launched_times(2).await;
    assert_eq!(relaunch, Vec::<String>::new());
    assert!(daemon.listed_terminal_ids().await.contains(&tab));
    assert_eq!(
        outcomes_of(&response.restored_tabs),
        vec![(tab.clone(), RestoredTabOutcome::Fresh)]
    );
    assert!(!response.restored_tabs[0].reason.is_empty());
}

#[tokio::test]
#[serial_test::serial]
async fn a_tab_live_at_resume_is_not_restarted() {
    // Given a stopped session with an OpenCode tab the daemon still runs
    let daemon = a_stopped_session();
    let tab = daemon.opens_a_tab("opencode-cli").await;
    daemon.opencode.until_launched_times(1).await;

    // When the session is resumed on the same daemon
    let response = daemon.resumes().await;

    // Then the tab is left running as it was: OpenCode is not launched again
    assert!(daemon.listed_terminal_ids().await.contains(&tab));
    assert_eq!(
        daemon.opencode.launch_count(),
        1,
        "the live tab was not restarted"
    );
    assert_eq!(response.restored_tabs, vec![restored(&tab)]);
}

// ---------------------------------------------------------------------------
// tmux
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_live_tmux_wrapped_tab_is_reattached_not_relaunched_on_resume() {
    // Given a tmux-wrapped Claude tab whose agent still runs in its tmux session
    let daemon = a_stopped_session();
    daemon
        .runs_claude_in_tmux_session(TMUX_TAB_SESSION, TMUX_TAB_CONVERSATION)
        .await;
    daemon.has_recorded_tabs(vec![the_main_tab(), a_tmux_wrapped_claude_tab()]);

    // When the session is resumed
    let response = daemon.resumes().await;

    // Then the tab is back under its id as a client of that tmux session, and its Claude was not
    // launched again
    until_attached(&daemon.tmux, TMUX_TAB_SESSION, 1).await;
    assert!(daemon.listed_terminal_ids().await.contains(TMUX_TAB));
    assert_eq!(
        daemon.claude.launches_carrying(TMUX_TAB_CONVERSATION),
        1,
        "the tab's agent was not relaunched"
    );
    assert_eq!(response.restored_tabs, vec![restored(TMUX_TAB)]);
}

#[tokio::test]
#[serial_test::serial]
async fn a_tmux_wrapped_tab_whose_tmux_session_is_gone_is_relaunched_and_resumed() {
    // Given a tmux-wrapped Claude tab whose tmux session no longer exists
    let daemon = a_stopped_session();
    daemon.has_recorded_tabs(vec![the_main_tab(), a_tmux_wrapped_claude_tab()]);

    // When the session is resumed
    let response = daemon.resumes().await;

    // Then its Claude is relaunched resuming the tab's conversation, inside a tmux session the
    // roster records and the tab is a client of
    daemon
        .claude
        .until_launched_with("--resume", TMUX_TAB_CONVERSATION)
        .await;
    let entry = daemon.roster_entry(TMUX_TAB);
    assert_eq!(entry.launch[0].kind, TMUX_KIND);
    let tmux_session = entry.launch[0]
        .property(SESSION_PROPERTY)
        .expect("the tmux node names its session")
        .to_string();
    until_attached(&daemon.tmux, &tmux_session, 1).await;
    assert!(daemon.listed_terminal_ids().await.contains(TMUX_TAB));
    assert_eq!(response.restored_tabs, vec![restored(TMUX_TAB)]);
}

#[tokio::test]
#[serial_test::serial]
async fn a_tmux_attach_tab_whose_session_is_gone_is_dropped_and_reported() {
    // Given a tab attached to the user's tmux session `work`, which no longer exists
    let daemon = a_stopped_session();
    daemon.has_recorded_tabs(vec![
        the_main_tab(),
        TabEntry::new(
            ATTACH_TAB,
            vec![LaunchNode::tmux_attach("work", &this_os_user())],
        ),
    ]);

    // When the session is resumed
    let response = daemon.resumes().await;

    // Then the resume succeeds without the tab, which is reported dropped naming the session, and is
    // gone from the roster
    assert_eq!(
        outcomes_of(&response.restored_tabs),
        vec![(ATTACH_TAB.to_string(), RestoredTabOutcome::Dropped)]
    );
    assert!(
        response.restored_tabs[0].reason.contains("work"),
        "{}",
        response.restored_tabs[0].reason
    );
    assert!(!daemon.listed_terminal_ids().await.contains(ATTACH_TAB));
    assert!(!daemon.roster_records(ATTACH_TAB));
}

// ---------------------------------------------------------------------------
// Jailed shell tabs (`#session-liveness` 5/8)
//
// The session's main resumes on the host here: a jailed main cannot start without a kernel
// sandbox, and every resume path restores the whole roster the same way, wherever its main runs.
// What is under test is the tab's own jail, started through the in-memory `TabJailSpawn`.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_jailed_shell_tab_is_restored_into_a_new_jail_on_resume() {
    // Given a stopped session whose roster holds a shell tab recorded in a jail of its own
    let jails = a_tab_jail_spawn();
    let daemon =
        a_stopped_session_whose_tab_jails_start_through(jails.clone() as Arc<dyn TabJailSpawn>);
    daemon.has_recorded_tabs(vec![the_main_tab(), a_jailed_shell_tab()]);

    // When it is resumed
    let response = daemon.resumes().await;

    // Then a new jail is started for that tab, over the session's worktree, and the tab is back
    // under its id, still recorded in a jail, and reported restored
    let launch = jails.launch_of(JAILED_TAB);
    assert_eq!(
        (launch.session_id.as_str(), launch.worktree.clone()),
        (RESUMED_SESSION_ID, daemon.worktree.clone())
    );
    // ... for the session's OS user and its login shell, with the resuming person as the owner of the route's vault
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
    assert!(jails.jail_runs(JAILED_TAB));
    assert!(daemon.listed_terminal_ids().await.contains(JAILED_TAB));
    assert_eq!(
        daemon.roster_entry(JAILED_TAB).launch,
        vec![LaunchNode::sandbox(), LaunchNode::shell()]
    );
    assert_eq!(response.restored_tabs, vec![restored(JAILED_TAB)]);
}

#[tokio::test]
#[serial_test::serial]
async fn a_jailed_tab_whose_jail_cannot_start_is_dropped_with_its_reason() {
    // Given a stopped session with a jailed shell tab, on a daemon whose tab jails cannot start
    let jails = a_tab_jail_spawn_refusing(JAIL_REFUSAL);
    let daemon =
        a_stopped_session_whose_tab_jails_start_through(jails.clone() as Arc<dyn TabJailSpawn>);
    daemon.has_recorded_tabs(vec![the_main_tab(), a_jailed_shell_tab()]);

    // When it is resumed
    let response = daemon.resumes().await;

    // Then the resume succeeds without the tab, which is reported dropped with why its jail did
    // not start, and is gone from the roster
    assert_eq!(
        outcomes_of(&response.restored_tabs),
        vec![(JAILED_TAB.to_string(), RestoredTabOutcome::Dropped)]
    );
    assert!(
        response.restored_tabs[0].reason.contains(JAIL_REFUSAL),
        "{}",
        response.restored_tabs[0].reason
    );
    assert!(!daemon.listed_terminal_ids().await.contains(JAILED_TAB));
    assert!(!daemon.roster_records(JAILED_TAB));
}

#[tokio::test]
#[serial_test::serial]
async fn a_mixed_roster_restores_each_tab_where_its_chain_says() {
    // Given a stopped session whose roster holds a jailed shell tab and a shell tab on the host
    let jails = a_tab_jail_spawn();
    let daemon =
        a_stopped_session_whose_tab_jails_start_through(jails.clone() as Arc<dyn TabJailSpawn>);
    daemon.has_recorded_tabs(vec![
        the_main_tab(),
        a_jailed_shell_tab(),
        TabEntry::new(SHELL_TAB, vec![LaunchNode::shell()]),
    ]);

    // When it is resumed
    let response = daemon.resumes().await;

    // Then only the jailed tab gets a jail, both tabs are back under their ids, and both are
    // reported restored
    assert_eq!(jails.launched_terminals(), vec![JAILED_TAB.to_string()]);
    daemon
        .until_listed_terminals_are(&[MAIN_TERMINAL_ID, JAILED_TAB, SHELL_TAB])
        .await;
    assert_eq!(
        response.restored_tabs,
        vec![restored(JAILED_TAB), restored(SHELL_TAB)]
    );
}

// ---------------------------------------------------------------------------
// Shell tabs on the session's target (`#session-liveness` 6/8)
//
// The session's main resumes on the host here: a target main cannot resume without a reachable
// target, and every resume path restores the whole roster the same way, wherever its main runs.
// What is under test is the tab's own target, resolved through the in-memory `TabTargetShell`.
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial_test::serial]
async fn a_target_shell_tab_is_reopened_on_the_target_on_resume() {
    // Given a stopped session whose roster holds a shell tab recorded on its target, a reachable
    // machine
    let (target_shells, machine) = target_shells_reaching_a_machine();
    let daemon = a_stopped_session_whose_target_shells_resolve_through(
        target_shells.clone() as Arc<dyn TabTargetShell>
    );
    daemon.has_recorded_tabs(vec![the_main_tab(), a_target_shell_tab()]);

    // When it is resumed
    let response = daemon.resumes().await;

    // Then the session's target was resolved as the resume's caller, and the target's login shell
    // runs in the session's worktree there ...
    assert_eq!(
        target_shells.asked(),
        vec![(daemon.session_dir(), OWNER.to_string())]
    );
    assert_eq!(
        where_the_login_shell_started(&machine).await,
        machine.worktree()
    );
    // ... and the tab is back under its id, still recorded on the target, and reported restored
    assert!(daemon.listed_terminal_ids().await.contains(TARGET_TAB));
    assert_eq!(
        daemon.roster_entry(TARGET_TAB).launch,
        vec![LaunchNode::target(), LaunchNode::shell()]
    );
    assert_eq!(response.restored_tabs, vec![restored(TARGET_TAB)]);
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_tab_whose_target_is_unreachable_is_dropped_with_its_reason() {
    // Given a stopped session with a shell tab on its target, a machine its transport cannot reach
    let (target_shells, machine) = target_shells_reaching_an_unreachable_machine();
    let daemon = a_stopped_session_whose_target_shells_resolve_through(
        target_shells as Arc<dyn TabTargetShell>,
    );
    daemon.has_recorded_tabs(vec![the_main_tab(), a_target_shell_tab()]);

    // When it is resumed
    let response = daemon.resumes().await;

    // Then the resume succeeds without the tab, which is reported dropped with what the transport
    // said, is gone from the roster, and opened no terminal on the target
    assert_eq!(
        outcomes_of(&response.restored_tabs),
        vec![(TARGET_TAB.to_string(), RestoredTabOutcome::Dropped)]
    );
    assert!(
        response.restored_tabs[0].reason.contains(UNREACHABLE),
        "{}",
        response.restored_tabs[0].reason
    );
    assert!(!daemon.listed_terminal_ids().await.contains(TARGET_TAB));
    assert!(!daemon.roster_records(TARGET_TAB));
    assert_eq!(machine.interactive_commands(), Vec::<String>::new());
}

#[tokio::test]
#[serial_test::serial]
async fn a_coder_target_tab_with_a_locked_vault_is_dropped_with_its_reason() {
    // Given a stopped session with a shell tab on its Coder target, whose owner's vault is locked
    let target_shells = target_shells_refusing(Status::failed_precondition(VAULT_LOCKED));
    let daemon = a_stopped_session_whose_target_shells_resolve_through(
        target_shells as Arc<dyn TabTargetShell>,
    );
    daemon.has_recorded_tabs(vec![the_main_tab(), a_target_shell_tab()]);

    // When it is resumed
    let response = daemon.resumes().await;

    // Then the resume succeeds without the tab, which is reported dropped saying the vault is
    // locked, and is gone from the roster
    assert_eq!(
        outcomes_of(&response.restored_tabs),
        vec![(TARGET_TAB.to_string(), RestoredTabOutcome::Dropped)]
    );
    assert!(
        response.restored_tabs[0].reason.contains(VAULT_LOCKED),
        "{}",
        response.restored_tabs[0].reason
    );
    assert!(!daemon.listed_terminal_ids().await.contains(TARGET_TAB));
    assert!(!daemon.roster_records(TARGET_TAB));
}
