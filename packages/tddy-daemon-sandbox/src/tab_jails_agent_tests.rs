//! `TabJails` and a jailed **agent** tab (`#session-liveness` 7/8): started through the spawn's agent
//! variant with the agent it runs, registered and watched like a shell tab's jail, and — unlike a
//! shell's — counted as a live agent of its session while its runner runs.
//!
//! The jails are started through a stand-in [`TabJailSpawn`] whose "runner" is a `sleep`: what the
//! registry owns is the runner's lifetime and what the jail was asked to run.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_daemon_kernel::live_sessions::LiveSessions;
use tddy_rpc::{Code, Status};
use tddy_task::TerminalCapture;
use tokio::sync::{broadcast, mpsc};

use super::{TabJailAgent, TabJailKey, TabJailLaunch, TabJailPaths, TabJailSpawn, TabJails};
use crate::sandbox_session::{SandboxSessionState, SandboxSessionStateInit};

const SESSION: &str = "jailed-agent-session";
const OTHER_SESSION: &str = "other-jailed-agent-session";
const AGENT_TAB: &str = "agent-tab-1";
const SHELL_TAB: &str = "shell-tab-1";
const CONVERSATION: &str = "5f3c0000-0000-7000-8000-00000000a6e1";

/// A safety net for an exited runner to be noticed, not a prediction of how long it takes.
const DEATH_WATCH: Duration = Duration::from_secs(10);

/// What the stand-in spawn was asked to start: a shell, or an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asked {
    Shell {
        terminal_id: String,
    },
    Agent {
        terminal_id: String,
        agent: TabJailAgent,
    },
}

/// A spawn whose jail's runner is a `sleep`, recording what every jail was asked to run.
struct StandInSpawn {
    asked: StdMutex<Vec<Asked>>,
    refusal: Option<String>,
    dir: tempfile::TempDir,
}

fn a_stand_in_spawn() -> StandInSpawn {
    StandInSpawn {
        asked: StdMutex::new(Vec::new()),
        refusal: None,
        dir: tempfile::tempdir().unwrap(),
    }
}

fn a_spawn_refusing(reason: &str) -> StandInSpawn {
    StandInSpawn {
        refusal: Some(reason.to_string()),
        ..a_stand_in_spawn()
    }
}

impl StandInSpawn {
    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    fn a_stand_in_jail(&self, launch: &TabJailLaunch) -> Result<Arc<SandboxSessionState>, Status> {
        if let Some(reason) = &self.refusal {
            return Err(Status::failed_precondition(reason.clone()));
        }
        let child = Command::new("sleep")
            .arg("300")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the stand-in runner starts");
        let pid = child.id();
        let handle = tddy_sandbox::SandboxHandle::new(
            child,
            self.dir.path().join("profile.sb"),
            self.dir.path().join("unused.sock"),
            self.dir.path().join("sandbox.ready"),
        );
        let (stdout_tx, _) = broadcast::channel(4);
        let (stdin_tx, _) = mpsc::unbounded_channel();
        Ok(Arc::new(SandboxSessionState::new(
            SandboxSessionStateInit {
                pid,
                worktree_path: launch.worktree.clone(),
                stdout_tx,
                capture: Arc::new(StdMutex::new(TerminalCapture::new())),
                stdin_tx,
                ready_marker: launch.paths.ready_marker.clone(),
                handle,
                managed_workflow: None,
            },
        )))
    }
}

#[async_trait]
impl TabJailSpawn for StandInSpawn {
    async fn spawn_shell(
        &self,
        launch: &TabJailLaunch,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        self.asked.lock().unwrap().push(Asked::Shell {
            terminal_id: launch.terminal_id.clone(),
        });
        self.a_stand_in_jail(launch)
    }

    async fn spawn_agent(
        &self,
        launch: &TabJailLaunch,
        agent: &TabJailAgent,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        self.asked.lock().unwrap().push(Asked::Agent {
            terminal_id: launch.terminal_id.clone(),
            agent: agent.clone(),
        });
        self.a_stand_in_jail(launch)
    }
}

/// The launch of tab `terminal_id`'s jail, with paths that only have to be distinct.
fn a_launch(session_id: &str, terminal_id: &str) -> TabJailLaunch {
    let root = PathBuf::from("/sessions")
        .join(session_id)
        .join("sandbox/tabs")
        .join(terminal_id);
    TabJailLaunch {
        session_id: session_id.to_string(),
        terminal_id: terminal_id.to_string(),
        session_dir: PathBuf::from("/sessions").join(session_id),
        worktree: PathBuf::from("/worktrees").join(session_id),
        paths: TabJailPaths {
            root: root.clone(),
            ipc_socket: root.join("tab.sock"),
            ready_marker: root.join("sandbox.ready"),
            exec_plan: root.join("exec-plan.json"),
            profile: root.join("sandbox.sb"),
            egress_dir: root.join("egress"),
            scratch_dir: root.join(".work"),
        },
        os_user: "tddy".to_string(),
        login_shell: "/bin/zsh".to_string(),
        subject: "ada".to_string(),
    }
}

/// A Claude tab in a conversation of its own.
fn a_claude_agent() -> TabJailAgent {
    TabJailAgent {
        harness: "claude-cli".to_string(),
        conversation_id: CONVERSATION.to_string(),
        resume: false,
    }
}

/// Start the agent tab `terminal_id` of `session_id` through `spawn`, which must start.
async fn an_agent_tab_started(
    jails: &TabJails,
    spawn: &StandInSpawn,
    session_id: &str,
    terminal_id: &str,
) -> Arc<SandboxSessionState> {
    jails
        .start_agent(spawn, a_launch(session_id, terminal_id), a_claude_agent())
        .await
        .expect("the agent tab's jail starts")
}

/// The runner behind `state` exits on its own, as a crashed agent's jail does.
fn the_runner_exits(state: &SandboxSessionState) {
    let pid = state.runner_pid().expect("the runner is running") as libc::pid_t;
    // SAFETY: `pid` is the stand-in runner this test spawned, still running and unreaped.
    unsafe { libc::kill(pid, libc::SIGKILL) };
}

/// Wait until `jails` counts no live jailed agent tab of `session_id`.
async fn until_not_live(jails: &TabJails, session_id: &str) {
    let settled = tokio::time::timeout(DEATH_WATCH, async {
        while jails.live_agent_sessions().await.contains(session_id) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        settled.is_ok(),
        "session {session_id} is still counted live after its agent's jail exited"
    );
}

// ---------------------------------------------------------------------------
// Starting an agent tab's jail
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_agent_tabs_jail_is_started_with_the_agent_it_runs_and_registered_under_its_tab() {
    // Given
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();

    // When
    let started = an_agent_tab_started(&jails, &spawn, SESSION, AGENT_TAB).await;

    // Then the spawn was asked for the agent, not a shell, and the jail is the tab's
    assert_eq!(
        spawn.asked(),
        vec![Asked::Agent {
            terminal_id: AGENT_TAB.to_string(),
            agent: a_claude_agent(),
        }]
    );
    let registered = jails
        .get(SESSION, AGENT_TAB)
        .await
        .expect("the agent tab's jail is registered");
    assert!(Arc::ptr_eq(&registered, &started));
}

#[tokio::test]
async fn a_second_jail_for_an_agent_tab_that_has_one_is_refused_and_not_started() {
    // Given an agent tab whose jail runs
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    an_agent_tab_started(&jails, &spawn, SESSION, AGENT_TAB).await;

    // When a jail is asked for the same tab again
    let refused = jails
        .start_agent(&spawn, a_launch(SESSION, AGENT_TAB), a_claude_agent())
        .await
        .err()
        .expect("one jail per tab");

    // Then it is refused, and the spawn was asked only once
    assert_eq!(refused.code(), Code::AlreadyExists);
    assert_eq!(spawn.asked().len(), 1);
}

#[tokio::test]
async fn an_agent_jail_that_cannot_start_is_refused_with_its_reason_and_registers_nothing() {
    // Given a spawn that cannot start the agent's jail
    let jails = TabJails::new();
    let spawn = a_spawn_refusing("no claude tool for the jail");

    // When the agent tab's jail is started
    let refused = jails
        .start_agent(&spawn, a_launch(SESSION, AGENT_TAB), a_claude_agent())
        .await
        .err()
        .expect("the spawn refuses");

    // Then the refusal is the spawn's, and nothing is registered or counted live
    assert_eq!(
        (refused.code(), refused.message()),
        (Code::FailedPrecondition, "no claude tool for the jail")
    );
    assert!(jails.get(SESSION, AGENT_TAB).await.is_none());
    assert_eq!(jails.live_agent_sessions().await, LiveSessions::none());
}

#[tokio::test]
async fn an_agent_jail_whose_runner_exits_is_announced_ended() {
    // Given an agent tab whose jail runs, and the death watch subscribed to
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    let mut ended = jails.subscribe_ended();
    let state = an_agent_tab_started(&jails, &spawn, SESSION, AGENT_TAB).await;

    // When its runner exits on its own
    the_runner_exits(&state);

    // Then the tab is announced ended and the registry no longer holds it
    let announced = tokio::time::timeout(DEATH_WATCH, ended.recv())
        .await
        .expect("the death watch announces the exit")
        .expect("the death watch is still subscribed");
    assert_eq!(announced, TabJailKey::new(SESSION, AGENT_TAB));
    assert!(jails.get(SESSION, AGENT_TAB).await.is_none());
}

// ---------------------------------------------------------------------------
// A jailed agent tab's liveness
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_jailed_agent_tab_whose_runner_runs_makes_its_session_live() {
    // Given
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    an_agent_tab_started(&jails, &spawn, SESSION, AGENT_TAB).await;

    // When
    let live = jails.live_agent_sessions().await;

    // Then
    assert_eq!(live, LiveSessions::of([SESSION]));
}

#[tokio::test]
async fn a_jailed_agent_tab_makes_only_its_own_session_live() {
    // Given an agent tab of one session and nothing of another
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    an_agent_tab_started(&jails, &spawn, SESSION, AGENT_TAB).await;

    // When
    let live = jails.live_agent_sessions().await;

    // Then
    assert!(!live.contains(OTHER_SESSION));
}

#[tokio::test]
async fn a_jailed_agent_tab_whose_runner_exited_no_longer_makes_its_session_live() {
    // Given an agent tab whose jail ran
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    let state = an_agent_tab_started(&jails, &spawn, SESSION, AGENT_TAB).await;

    // When its runner exits on its own
    the_runner_exits(&state);

    // Then its session is no longer counted live
    until_not_live(&jails, SESSION).await;
    assert_eq!(jails.live_agent_sessions().await, LiveSessions::none());
}

#[tokio::test]
async fn a_jailed_shell_tab_never_makes_its_session_live() {
    // Given a session whose only jailed tab is a shell
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    jails
        .start(&spawn, a_launch(SESSION, SHELL_TAB))
        .await
        .expect("the shell tab's jail starts");

    // When
    let live = jails.live_agent_sessions().await;

    // Then a shell is a tool of the user's, not an agent
    assert_eq!(live, LiveSessions::none());
}

#[tokio::test]
async fn a_stopped_agent_tab_no_longer_makes_its_session_live() {
    // Given an agent tab whose jail runs
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    an_agent_tab_started(&jails, &spawn, SESSION, AGENT_TAB).await;

    // When its tab is closed
    let stopped = jails.stop(SESSION, AGENT_TAB).await;

    // Then its jail is gone and its session is not counted live
    assert!(stopped, "the agent tab had a jail to stop");
    assert_eq!(jails.live_agent_sessions().await, LiveSessions::none());
}
