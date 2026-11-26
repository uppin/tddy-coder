//! Acceptance: the tmux tabs of a target session run on its **target** (`#target-tmux` 2/2) — the
//! "+" menu lists the target's tmux, attach / new tmux session and "<agent> in tmux" tabs run in tmux
//! there, closing an agent-in-tmux tab kills its tmux session there, and a resume re-attaches or
//! recreates them.
//!
//! A module of `remote_target_session_acceptance`: the real daemon, and the stand-in ssh target
//! whose tmux is this host's binary on the machine's own socket directory, so no test reaches a
//! person's tmux server.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-23-target-tmux.md (n2)

use pretty_assertions::assert_eq;
use tddy_core::tab_roster::{
    read_tab_roster, write_tab_roster, LaunchNode, TabEntry, TabRoster, AGENT_KIND,
    CONVERSATION_ID_PROPERTY, SESSION_PROPERTY, TMUX_KIND,
};
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::session::RestoredTabOutcome;
use tddy_terminal_rpc::proto::terminal_session::{
    LaunchNode as WireLaunchNode, ListTmuxSessionsRequest, StartTerminalSessionRequest,
    StopTerminalSessionRequest, TerminalSessionService as TerminalSessionServiceTrait,
};
use tddy_testing_commons::wait::eventually;

use super::cli_harness_support::VALID_TOKEN;
use super::{a_daemon_with_a_reachable_target, ADaemonWithATarget, OBSERVE, SESSION_TYPE};

/// A tmux session a person keeps on the target.
const THEIR_TMUX_SESSION: &str = "work";

/// The terminal kind of a tab attached to a tmux session.
const TMUX: &str = "tmux";
/// The terminal kind of a tab on a new tmux session.
const TMUX_NEW: &str = "tmux-new";
/// The wrapper of a tab run on the session's target.
const TARGET: &str = "target";

fn wrappers(kinds: &[&str]) -> Vec<WireLaunchNode> {
    kinds
        .iter()
        .map(|kind| WireLaunchNode {
            kind: kind.to_string(),
            properties: Default::default(),
        })
        .collect()
}

impl ADaemonWithATarget {
    /// The tmux sessions the "+" menu of `session_id` lists, by name.
    async fn tmux_listing_of(&self, session_id: &str) -> Result<Vec<String>, Status> {
        self.service
            .terminal_session_service()
            .list_tmux_sessions(Request::direct(ListTmuxSessionsRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
            }))
            .await
            .map(|listed| {
                listed
                    .into_inner()
                    .sessions
                    .into_iter()
                    .map(|session| session.name)
                    .collect()
            })
    }

    /// Open a tab of `kind` (attached to `tmux_session` for a tmux kind) wrapped in `wrapped_in`.
    pub(crate) async fn opens_a_tab(
        &self,
        session_id: &str,
        kind: &str,
        tmux_session: &str,
        wrapped_in: &[&str],
    ) -> Result<String, Status> {
        self.service
            .terminal_session_service()
            .start_terminal_session(Request::direct(StartTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                control_token: String::new(),
                kind: kind.to_string(),
                tmux_session: tmux_session.to_string(),
                wrappers: wrappers(wrapped_in),
            }))
            .await
            .map(|started| started.into_inner().terminal_id)
    }

    /// Close the tab `terminal_id`, as its tab's ✕ does.
    async fn closes_the_tab(&self, session_id: &str, terminal_id: &str) {
        self.service
            .terminal_session_service()
            .stop_terminal_session(Request::direct(StopTerminalSessionRequest {
                session_token: VALID_TOKEN.to_string(),
                session_id: session_id.to_string(),
                terminal_id: terminal_id.to_string(),
                control_token: String::new(),
            }))
            .await
            .expect("the tab closes");
    }

    /// A tmux session `name` the target's own user started, in the session's worktree there.
    fn the_target_user_starts_a_tmux_session(&self, session_id: &str, name: &str) {
        let worktree = self.target.worktree_of(session_id);
        let started = self.target.tmux(&[
            "new-session",
            "-d",
            "-s",
            name,
            "-c",
            &worktree.to_string_lossy(),
        ]);
        assert!(started.status.success(), "{started:?}");
    }

    /// The tmux session `name` on the target ends, as a person's `exit` ends it.
    fn the_tmux_session_ends_on_the_target(&self, name: &str) {
        let killed = self
            .target
            .tmux(&["kill-session", "-t", &format!("={name}")]);
        assert!(killed.status.success(), "{killed:?}");
    }

    /// The number of clients attached to the target's tmux session `name`.
    fn clients_of(&self, name: &str) -> usize {
        let listed = self
            .target
            .tmux(&["list-clients", "-t", &format!("={name}")]);
        String::from_utf8_lossy(&listed.stdout).lines().count()
    }

    /// Wait until the target's tmux session `name` has `count` attached clients.
    async fn the_target_tmux_session_has_clients(&self, name: &str, count: usize) {
        eventually(
            &format!("{count} client(s) on the target's tmux session {name:?}"),
            OBSERVE,
            || {
                let clients = self.clients_of(name);
                (clients == count)
                    .then_some(())
                    .ok_or_else(|| format!("{clients} client(s) so far"))
            },
        )
        .await;
    }

    /// The directory the first pane of the target's tmux session `name` started in.
    fn pane_path_of(&self, name: &str) -> String {
        let shown = self.target.tmux(&[
            "display-message",
            "-p",
            "-t",
            &format!("={name}:"),
            "#{pane_current_path}",
        ]);
        String::from_utf8_lossy(&shown.stdout).trim().to_string()
    }

    fn roster_of(&self, session_id: &str) -> TabRoster {
        read_tab_roster(&self.session_dir(session_id)).expect("the session's tabs.yaml reads")
    }

    /// The roster's entry for `terminal_id`.
    fn roster_entry_of(&self, session_id: &str, terminal_id: &str) -> TabEntry {
        self.roster_of(session_id)
            .tabs
            .into_iter()
            .find(|tab| tab.terminal_id == terminal_id)
            .unwrap_or_else(|| panic!("the roster records terminal {terminal_id}"))
    }

    /// The tmux session the recorded chain of `terminal_id` names.
    pub(crate) fn recorded_tmux_session_of(&self, session_id: &str, terminal_id: &str) -> String {
        self.roster_entry_of(session_id, terminal_id)
            .launch
            .iter()
            .find_map(|node| node.property(SESSION_PROPERTY).map(str::to_string))
            .expect("the chain names its tmux session")
    }

    /// Stop the session and lose every tab's local client, as a daemon that went away does: what
    /// runs in tmux on the target stays.
    async fn every_client_is_gone(&self, session_id: &str, tabs: &[&str]) {
        for tab in tabs {
            self.manager.stop_terminal(session_id, tab).await;
        }
        self.stop(session_id).await;
    }
}

/// A started session on the target, its agent running there.
async fn a_running_session_on_the_target(daemon: &ADaemonWithATarget) -> String {
    let session_id = daemon.a_session_on_the_target().await;
    daemon.the_agent_on_the_target(&session_id).await;
    session_id
}

#[tokio::test]
#[serial_test::serial]
async fn a_target_sessions_tmux_listing_lists_the_targets_tmux_sessions() {
    // Given a session on the target, whose user keeps a tmux session there
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;
    daemon.the_target_user_starts_a_tmux_session(&session_id, THEIR_TMUX_SESSION);

    // When its "+" menu lists tmux sessions
    let listed = daemon
        .tmux_listing_of(&session_id)
        .await
        .expect("a target with tmux lists its sessions");

    // Then it lists the target's tmux sessions
    assert_eq!(listed, vec![THEIR_TMUX_SESSION.to_string()]);
}

#[tokio::test]
#[serial_test::serial]
async fn a_new_tmux_session_tab_on_the_target_runs_that_tmux_session_there_and_is_recorded_on_the_target(
) {
    // Given a session on the target
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;

    // When a new tmux session tab is opened on the target
    let tab = daemon
        .opens_a_tab(&session_id, TMUX_NEW, "scratch", &[TARGET])
        .await
        .expect("a new tmux session tab starts on the target");

    // Then the target runs that tmux session in the session's worktree there, the tab attached to
    // it, and the tab is recorded as attached to it on the target
    daemon
        .the_target_tmux_session_has_clients("scratch", 1)
        .await;
    assert_eq!(
        (
            daemon.pane_path_of("scratch"),
            daemon
                .roster_entry_of(&session_id, &tab)
                .launch
                .iter()
                .map(|node| node.kind.as_str())
                .collect::<Vec<_>>(),
            daemon.recorded_tmux_session_of(&session_id, &tab),
        ),
        (
            daemon
                .target
                .worktree_of(&session_id)
                .to_string_lossy()
                .into_owned(),
            vec![TARGET, "tmux-attach"],
            "scratch".to_string(),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn attaching_a_tab_to_a_tmux_session_on_the_target_runs_a_client_of_it_there() {
    // Given a session on the target, whose user keeps a tmux session there
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;
    daemon.the_target_user_starts_a_tmux_session(&session_id, THEIR_TMUX_SESSION);

    // When a tab is attached to it on the target
    daemon
        .opens_a_tab(&session_id, TMUX, THEIR_TMUX_SESSION, &[TARGET])
        .await
        .expect("a tab attaches to a tmux session on the target");

    // Then the tab is a client of that tmux session there
    daemon
        .the_target_tmux_session_has_clients(THEIR_TMUX_SESSION, 1)
        .await;
}

#[tokio::test]
#[serial_test::serial]
async fn attaching_a_tab_to_a_tmux_session_the_target_lacks_is_refused_not_found() {
    // Given a session on the target, whose target holds no tmux session "work"
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;

    // When a tab is attached to "work" on the target
    let refusal = daemon
        .opens_a_tab(&session_id, TMUX, THEIR_TMUX_SESSION, &[TARGET])
        .await
        .expect_err("a tab attached to a tmux session the target lacks fails");

    // Then it is refused NOT_FOUND naming the session, no tmux session was made there, and nothing is
    // recorded
    assert_eq!(
        (
            refusal.code(),
            refusal.message().contains(THEIR_TMUX_SESSION),
            daemon.target.has_tmux_session(THEIR_TMUX_SESSION),
            daemon.roster_of(&session_id).tabs.len(),
        ),
        (Code::NotFound, true, false, 0),
        "the refusal: {refusal:?}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn an_agent_in_tmux_tab_on_the_target_runs_its_agent_in_a_tmux_session_of_its_own_there() {
    // Given a session on the target
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;

    // When an agent tab is opened in tmux on the target
    let tab = daemon
        .opens_a_tab(&session_id, SESSION_TYPE, "", &[TARGET, TMUX])
        .await
        .expect("an agent in tmux tab starts on the target");

    // Then the agent runs in the session's worktree on the target, in the tmux session its chain
    // records — target, tmux, agent — which lives there
    let runs = daemon.runs_on_the_target(&session_id, 2).await;
    let name = daemon.recorded_tmux_session_of(&session_id, &tab);
    assert_eq!(
        (
            runs[1].is_alive(),
            daemon.target.has_tmux_session(&name),
            daemon
                .roster_entry_of(&session_id, &tab)
                .launch
                .iter()
                .map(|node| node.kind.as_str())
                .collect::<Vec<_>>(),
        ),
        (true, true, vec![TARGET, TMUX_KIND, AGENT_KIND])
    );
}

#[tokio::test]
#[serial_test::serial]
async fn closing_an_agent_in_tmux_tab_on_the_target_kills_its_tmux_session_there() {
    // Given an agent in tmux tab running on the target
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;
    let tab = daemon
        .opens_a_tab(&session_id, SESSION_TYPE, "", &[TARGET, TMUX])
        .await
        .expect("an agent in tmux tab starts on the target");
    let agent = daemon.runs_on_the_target(&session_id, 2).await.remove(1);
    let name = daemon.recorded_tmux_session_of(&session_id, &tab);

    // When the tab is closed
    daemon.closes_the_tab(&session_id, &tab).await;

    // Then its tmux session and its agent are gone from the target
    super::the_agent_has_ended(&agent).await;
    assert!(!daemon.target.has_tmux_session(&name));
}

#[tokio::test]
#[serial_test::serial]
async fn a_resume_re_attaches_a_target_tmux_tab_whose_tmux_session_lives() {
    // Given a tab attached to a tmux session on the target, whose local client went with its daemon
    // while the tmux session lives on
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;
    daemon.the_target_user_starts_a_tmux_session(&session_id, THEIR_TMUX_SESSION);
    let tab = daemon
        .opens_a_tab(&session_id, TMUX, THEIR_TMUX_SESSION, &[TARGET])
        .await
        .expect("a tab attaches to a tmux session on the target");
    daemon.every_client_is_gone(&session_id, &[&tab]).await;
    daemon
        .the_target_tmux_session_has_clients(THEIR_TMUX_SESSION, 0)
        .await;

    // When the session is resumed
    let restored = daemon
        .resume_reporting_tabs(&session_id)
        .await
        .expect("the session resumes");

    // Then the tab is restored as it was: a client of the same tmux session on the target again
    daemon
        .the_target_tmux_session_has_clients(THEIR_TMUX_SESSION, 1)
        .await;
    assert_eq!(
        restored
            .iter()
            .map(|tab| (tab.terminal_id.clone(), tab.outcome))
            .collect::<Vec<_>>(),
        vec![(tab, RestoredTabOutcome::Restored as i32)]
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_resume_recreates_a_target_tmux_tab_whose_tmux_session_is_gone() {
    // Given a new tmux session tab on the target whose tmux session ended with its client gone
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;
    let tab = daemon
        .opens_a_tab(&session_id, TMUX_NEW, "scratch", &[TARGET])
        .await
        .expect("a new tmux session tab starts on the target");
    daemon
        .the_target_tmux_session_has_clients("scratch", 1)
        .await;
    daemon.every_client_is_gone(&session_id, &[&tab]).await;
    daemon.the_tmux_session_ends_on_the_target("scratch");

    // When the session is resumed
    let restored = daemon
        .resume_reporting_tabs(&session_id)
        .await
        .expect("the session resumes");

    // Then the tab comes back fresh: its tmux session made again in the worktree on the target, the
    // tab attached to it
    daemon
        .the_target_tmux_session_has_clients("scratch", 1)
        .await;
    assert_eq!(
        (
            restored
                .iter()
                .map(|tab| (tab.terminal_id.clone(), tab.outcome))
                .collect::<Vec<_>>(),
            daemon.pane_path_of("scratch"),
        ),
        (
            vec![(tab, RestoredTabOutcome::Fresh as i32)],
            daemon
                .target
                .worktree_of(&session_id)
                .to_string_lossy()
                .into_owned(),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_resume_resumes_the_agent_of_an_agent_in_tmux_tab_whose_tmux_session_is_gone_on_the_target(
) {
    // Given an agent in tmux tab on the target whose tmux session — and agent — ended with its
    // client gone
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;
    let tab = daemon
        .opens_a_tab(&session_id, SESSION_TYPE, "", &[TARGET, TMUX])
        .await
        .expect("an agent in tmux tab starts on the target");
    let first = daemon.runs_on_the_target(&session_id, 2).await.remove(1);
    let name = daemon.recorded_tmux_session_of(&session_id, &tab);
    let conversation = daemon
        .roster_entry_of(&session_id, &tab)
        .launch
        .iter()
        .find_map(|node| node.property(CONVERSATION_ID_PROPERTY).map(str::to_string))
        .expect("the agent node records its conversation");
    daemon.every_client_is_gone(&session_id, &[&tab]).await;
    daemon.the_tmux_session_ends_on_the_target(&name);
    super::the_agent_has_ended(&first).await;

    // When the session is resumed
    daemon
        .resume_reporting_tabs(&session_id)
        .await
        .expect("the session resumes");

    // Then the tab's agent resumes its conversation, in its tmux session on the target again
    let resumed = eventually("the tab's agent to resume", OBSERVE, || {
        daemon
            .agent_runs()
            .into_iter()
            .find(|run| run.is_a_resume() && run.args.contains(&conversation))
            .ok_or_else(|| format!("the runs so far: {:?}", daemon.agent_runs()))
    })
    .await;
    assert_eq!(
        (resumed.is_alive(), daemon.target.has_tmux_session(&name)),
        (true, true)
    );
}

#[tokio::test]
#[serial_test::serial]
async fn the_startup_re_attach_leaves_target_tmux_tabs_for_resume() {
    // Given a stopped session on the target whose roster records an agent in tmux tab on the target
    let daemon = a_daemon_with_a_reachable_target();
    let session_id = a_running_session_on_the_target(&daemon).await;
    daemon.stop(&session_id).await;
    let os_user = std::env::var("USER").expect("the test runs as a named user");
    write_tab_roster(
        &daemon.session_dir(&session_id),
        &TabRoster {
            tabs: vec![TabEntry::new(
                "019b-tmux-agent-tab",
                vec![
                    LaunchNode::target(),
                    LaunchNode::tmux("tddy-on-the-target", &os_user),
                    LaunchNode::agent(SESSION_TYPE, "conversation-1"),
                ],
            )],
            ..Default::default()
        },
    )
    .expect("the roster is written");

    // When a fresh daemon re-attaches live tmux tabs at startup
    let report = daemon.service.reattach_live_tmux_tabs().await;

    // Then the target tab is left for resume, named as one that runs on its target — never looked
    // for in this host's tmux
    let left: Vec<(String, bool)> = report
        .left
        .iter()
        .map(|tab| (tab.terminal_id.clone(), tab.reason.contains("target")))
        .collect();
    assert_eq!(
        (report.reattached.len(), left),
        (0, vec![("019b-tmux-agent-tab".to_string(), true)])
    );
}
