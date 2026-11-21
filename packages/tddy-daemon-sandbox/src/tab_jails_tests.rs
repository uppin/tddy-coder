//! `TabJails`: one jail per `(session, terminal)`, stopped per tab, per session or all at once, and
//! watched for death — a jail whose runner exits is announced and forgotten.
//!
//! The jails are started through a stand-in [`TabJailSpawn`] whose "runner" is a `sleep`: what the
//! registry owns is the runner's lifetime, which a real process shows and a kernel sandbox adds
//! nothing to.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_rpc::{Code, Status};

use super::test_support::{a_launch, a_sleeping_jail};
use super::{TabJailAgent, TabJailKey, TabJailLaunch, TabJailSpawn, TabJails};
use crate::sandbox_session::SandboxSessionState;

const SESSION: &str = "jailed-session";
const OTHER_SESSION: &str = "other-jailed-session";
const TAB: &str = "tab-1";
const OTHER_TAB: &str = "tab-2";
const THIRD_TAB: &str = "tab-3";

/// A safety net for the death watch to notice an exited runner, not a prediction of how long it
/// takes.
const DEATH_WATCH: Duration = Duration::from_secs(10);

/// A spawn whose jail's runner is a `sleep`, recording every launch it was asked for.
struct StandInSpawn {
    launches: StdMutex<Vec<TabJailLaunch>>,
    refusal: Option<String>,
    dir: tempfile::TempDir,
}

fn a_stand_in_spawn() -> StandInSpawn {
    StandInSpawn {
        launches: StdMutex::new(Vec::new()),
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
    fn launch_count(&self) -> usize {
        self.launches.lock().unwrap().len()
    }
}

#[async_trait]
impl TabJailSpawn for StandInSpawn {
    async fn spawn_shell(
        &self,
        launch: &TabJailLaunch,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        self.launches.lock().unwrap().push(launch.clone());
        if let Some(reason) = &self.refusal {
            return Err(Status::failed_precondition(reason.clone()));
        }
        Ok(a_sleeping_jail(self.dir.path(), launch))
    }

    /// An agent's jail is the same stand-in: this suite is about the registry, not what runs in it.
    async fn spawn_agent(
        &self,
        launch: &TabJailLaunch,
        _agent: &TabJailAgent,
    ) -> Result<Arc<SandboxSessionState>, Status> {
        self.spawn_shell(launch).await
    }
}

/// Start the jail of each `(session, terminal)` through `spawn`, returning each one's state.
async fn started(
    jails: &TabJails,
    spawn: &StandInSpawn,
    tabs: &[(&str, &str)],
) -> Vec<Arc<SandboxSessionState>> {
    let mut states = Vec::new();
    for (session_id, terminal_id) in tabs {
        states.push(
            jails
                .start(spawn, a_launch(session_id, terminal_id))
                .await
                .expect("the tab's jail starts"),
        );
    }
    states
}

/// The runner behind `state` exits on its own, as a crashed or `exit`ed jail does.
fn the_runner_exits(state: &SandboxSessionState) {
    let pid = state.runner_pid().expect("the runner is running") as libc::pid_t;
    // SAFETY: `pid` is the stand-in runner this test spawned, still running and unreaped.
    unsafe { libc::kill(pid, libc::SIGKILL) };
}

fn keys(tabs: &[(&str, &str)]) -> Vec<TabJailKey> {
    tabs.iter()
        .map(|(session_id, terminal_id)| TabJailKey::new(session_id, terminal_id))
        .collect()
}

fn ids(terminal_ids: &[&str]) -> BTreeSet<String> {
    terminal_ids.iter().map(|id| id.to_string()).collect()
}

// ---------------------------------------------------------------------------
// start
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_started_tab_jail_is_registered_under_its_session_and_terminal() {
    // Given
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();

    // When
    let state = jails
        .start(&spawn, a_launch(SESSION, TAB))
        .await
        .expect("the tab's jail starts");

    // Then
    let registered = jails
        .get(SESSION, TAB)
        .await
        .expect("the tab's jail is registered");
    assert!(Arc::ptr_eq(&registered, &state));
    assert!(jails.get(SESSION, OTHER_TAB).await.is_none());
}

#[tokio::test]
async fn a_second_jail_for_a_tab_that_has_one_is_refused_and_not_started() {
    // Given
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    started(&jails, &spawn, &[(SESSION, TAB)]).await;

    // When
    let refused = jails
        .start(&spawn, a_launch(SESSION, TAB))
        .await
        .map(|_| ())
        .expect_err("a tab runs in one jail");

    // Then
    assert_eq!(refused.code(), Code::AlreadyExists);
    assert_eq!(spawn.launch_count(), 1, "no second runner was started");
}

#[tokio::test]
async fn a_jail_that_cannot_start_is_refused_with_its_reason_and_registers_nothing() {
    // Given
    let jails = TabJails::new();
    let spawn = a_spawn_refusing("the session's grants cannot be lowered");

    // When
    let refused = jails
        .start(&spawn, a_launch(SESSION, TAB))
        .await
        .map(|_| ())
        .expect_err("the spawn refused");

    // Then
    assert_eq!(refused.code(), Code::FailedPrecondition);
    assert_eq!(refused.message(), "the session's grants cannot be lowered");
    assert!(jails.get(SESSION, TAB).await.is_none());
}

// ---------------------------------------------------------------------------
// stop, stop_session, stop_all
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stopping_a_tab_jail_stops_its_runner_and_forgets_it() {
    // Given
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    let state = started(&jails, &spawn, &[(SESSION, TAB)]).await.remove(0);

    // When
    let stopped = jails.stop(SESSION, TAB).await;

    // Then
    assert!(stopped);
    assert!(!state.is_running(), "the jail's runner is stopped");
    assert!(jails.get(SESSION, TAB).await.is_none());
}

#[tokio::test]
async fn stopping_a_tab_that_has_no_jail_answers_false() {
    // Given
    let jails = TabJails::new();

    // When
    let stopped = jails.stop(SESSION, TAB).await;

    // Then
    assert!(!stopped);
}

#[tokio::test]
async fn stopping_a_session_stops_its_tab_jails_and_no_other_sessions() {
    // Given two tabs jailed in one session and one in another
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    let states = started(
        &jails,
        &spawn,
        &[
            (SESSION, TAB),
            (SESSION, OTHER_TAB),
            (OTHER_SESSION, THIRD_TAB),
        ],
    )
    .await;

    // When
    let stopped = jails.stop_session(SESSION).await;

    // Then
    assert_eq!(stopped, vec![TAB.to_string(), OTHER_TAB.to_string()]);
    assert_eq!(
        states
            .iter()
            .map(|state| state.is_running())
            .collect::<Vec<_>>(),
        vec![false, false, true]
    );
    assert_eq!(jails.live(OTHER_SESSION).await, ids(&[THIRD_TAB]));
}

#[tokio::test]
async fn stopping_all_stops_every_tab_jail_of_every_session() {
    // Given
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    let tabs = [(SESSION, TAB), (OTHER_SESSION, OTHER_TAB)];
    let states = started(&jails, &spawn, &tabs).await;

    // When
    let stopped = jails.stop_all().await;

    // Then
    assert_eq!(stopped, keys(&[(SESSION, TAB), (OTHER_SESSION, OTHER_TAB)]));
    assert_eq!(
        states
            .iter()
            .map(|state| state.is_running())
            .collect::<Vec<_>>(),
        vec![false, false]
    );
    assert_eq!(jails.live(SESSION).await, ids(&[]));
}

// ---------------------------------------------------------------------------
// live, and the death watch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn live_lists_the_tabs_of_a_session_whose_runner_runs() {
    // Given
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    started(
        &jails,
        &spawn,
        &[
            (SESSION, TAB),
            (SESSION, OTHER_TAB),
            (OTHER_SESSION, THIRD_TAB),
        ],
    )
    .await;

    // When
    let live = jails.live(SESSION).await;

    // Then
    assert_eq!(live, ids(&[TAB, OTHER_TAB]));
}

#[tokio::test]
async fn a_tab_jail_whose_runner_exits_is_announced_ended_and_leaves_the_live_set() {
    // Given a jailed tab, and the death watch subscribed to
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    let mut ended = jails.subscribe_ended();
    let state = started(&jails, &spawn, &[(SESSION, TAB)]).await.remove(0);

    // When its runner exits on its own
    the_runner_exits(&state);

    // Then the tab is announced ended, and the registry no longer holds or lists it
    let announced = tokio::time::timeout(DEATH_WATCH, ended.recv())
        .await
        .expect("the death watch announces the exit")
        .expect("the death watch is still subscribed");
    assert_eq!(announced, TabJailKey::new(SESSION, TAB));
    assert!(jails.get(SESSION, TAB).await.is_none());
    assert_eq!(jails.live(SESSION).await, ids(&[]));
}

#[tokio::test]
async fn a_tab_jail_stopped_through_the_registry_is_not_announced_ended() {
    // Given two jailed tabs, and the death watch subscribed to
    let jails = TabJails::new();
    let spawn = a_stand_in_spawn();
    let mut ended = jails.subscribe_ended();
    let states = started(&jails, &spawn, &[(SESSION, TAB), (SESSION, OTHER_TAB)]).await;

    // When the first is stopped through the registry, and then the second's runner exits by itself
    jails.stop(SESSION, TAB).await;
    the_runner_exits(&states[1]);

    // Then the only end announced is the second's: a stop that was asked for is not a death
    let announced = tokio::time::timeout(DEATH_WATCH, ended.recv())
        .await
        .expect("the death watch announces the exit")
        .expect("the death watch is still subscribed");
    assert_eq!(announced, TabJailKey::new(SESSION, OTHER_TAB));
}
