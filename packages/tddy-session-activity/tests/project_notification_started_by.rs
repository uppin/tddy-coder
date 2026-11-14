//! Acceptance: a project's notification records name the session's orchestrator, and an
//! orchestrator can read and wait on the records of the sessions it started alone
//! (`#orchestrators` 4/11).
//!
//! A turn's end is already an `attention_required` record ("finished a turn and is waiting"); each
//! record now also carries `orchestrator_session_id` — the orchestrator the session's changeset
//! names — and a scope with `started_by` returns only the records of sessions that name it.
//!
//! The path under test is the real log registered on a real bus over a real data directory, as in
//! `project_notification_log_acceptance.rs`. Every wait here is bounded.
//!
//! Feature doc: docs/ft/daemon/session-notifications.md, docs/ft/daemon/project-concept.md.
//! Feature doc: docs/ft/daemon/project-concept.md § Sending a session a turn.
//! History: docs/dev/changesets/2026-11-13-session-turn-control.md

mod project_notification_log_support;

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use project_notification_log_support::{a_log_over, sessions_of, write_session};
use tddy_session_activity::project_notification_log::{
    record_json, ProjectNotificationLog, ProjectNotificationRecord, ProjectNotificationScope,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind,
    SessionNotificationSource, SessionNotificationSubscriber,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const MAIN: &str = "01900000-0000-7000-8000-AABB00000400";
const ANOTHER_ORCHESTRATOR: &str = "01900000-0000-7000-8000-AABB00000401";
const MAINS_CHILD: &str = "01900000-0000-7000-8000-AABB00000402";
const THE_OTHERS_CHILD: &str = "01900000-0000-7000-8000-AABB00000403";
const NOBODYS_CHILD: &str = "01900000-0000-7000-8000-AABB00000404";

/// How long a test lets a wait run before it counts as hung.
const BOUND: Duration = Duration::from_secs(10);
/// A wait long enough that only a published record ends it.
const LONG_WAIT: Duration = Duration::from_secs(8);
/// How long after a wait begins its records are published.
const PUBLISH_DELAY: Duration = Duration::from_millis(100);

async fn bounded<T>(what: &str, future: impl Future<Output = T>) -> T {
    tokio::time::timeout(BOUND, future)
        .await
        .unwrap_or_else(|_| panic!("{what} did not finish within {BOUND:?}"))
}

// ── the world ────────────────────────────────────────────────────────────────────────────────

/// One daemon holding a project's main, another orchestrator, a child each of them started, and a
/// session nobody started; its log is registered on its bus.
struct ADaemon {
    _data_dir: tempfile::TempDir,
    log: Arc<ProjectNotificationLog>,
    bus: SessionNotificationBus,
}

fn a_daemon_holding_two_orchestrators_and_their_children() -> ADaemon {
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    let dir = data_dir.path();
    for session in [
        MAIN,
        ANOTHER_ORCHESTRATOR,
        MAINS_CHILD,
        THE_OTHERS_CHILD,
        NOBODYS_CHILD,
    ] {
        write_session(dir, ADA, session, PROJECT);
    }
    started_by(dir, MAINS_CHILD, MAIN);
    started_by(dir, THE_OTHERS_CHILD, ANOTHER_ORCHESTRATOR);
    let (log, bus) = a_log_over(dir);
    ADaemon {
        _data_dir: data_dir,
        log,
        bus,
    }
}

/// `child`'s changeset names `orchestrator` as the session that started it, as a start records it.
fn started_by(data_dir: &Path, child: &str, orchestrator: &str) {
    let session_dir = sessions_of(data_dir, ADA).join("sessions").join(child);
    let mut changeset = tddy_core::read_changeset(&session_dir).unwrap_or_default();
    changeset.orchestrator_session_id = Some(orchestrator.to_string());
    tddy_core::write_changeset(&session_dir, &changeset).expect("the child's changeset");
}

impl ADaemon {
    /// `session_id`'s agent finished its turn and waits: the record a turn's end leaves.
    async fn a_turn_ends_in(&self, session_id: &str) {
        self.bus.publish(a_turn_end_of(session_id)).await;
    }
}

fn a_turn_end_of(session_id: &str) -> SessionNotification {
    SessionNotification {
        session_id: session_id.to_string(),
        os_user: ADA.to_string(),
        label: "my-feature-branch".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: format!("{session_id} finished its turn"),
        agent: None,
        question: None,
        at_unix_ms: 1_793_000_000_000,
    }
}

/// What `caller` may see of the project; with `started_by_me`, only its own children's records.
fn the_scope_of(caller: &str, started_by_me: bool) -> ProjectNotificationScope {
    ProjectNotificationScope {
        project_id: PROJECT.to_string(),
        os_user: ADA.to_string(),
        exclude_session_id: caller.to_string(),
        started_by: started_by_me.then(|| caller.to_string()),
    }
}

fn sessions_of_records(records: &[ProjectNotificationRecord]) -> Vec<String> {
    records.iter().map(|r| r.session_id.clone()).collect()
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_turn_end_record_names_the_sessions_orchestrator() {
    // Given a daemon whose main started a child
    let daemon = a_daemon_holding_two_orchestrators_and_their_children();

    // When the child's turn ends
    daemon.a_turn_ends_in(MAINS_CHILD).await;

    // Then its record names main as its orchestrator, and so does the record's JSON form the
    // notification tools answer with
    let read = daemon
        .log
        .read(&the_scope_of(MAIN, false), "", None)
        .expect("the log reads");
    let record = read.page.items.last().expect("the turn's end is recorded");
    assert_eq!(record.orchestrator_session_id.as_deref(), Some(MAIN));
    assert_eq!(
        record_json(record).expect("the record serialises")["orchestrator_session_id"],
        serde_json::json!(MAIN)
    );
}

#[tokio::test]
async fn started_by_me_returns_only_the_callers_sessions() {
    // Given main waiting, from now, on its own children's records
    let daemon = a_daemon_holding_two_orchestrators_and_their_children();
    let waiting = {
        let log = Arc::clone(&daemon.log);
        tokio::spawn(async move { log.wait(&the_scope_of(MAIN, true), "", LONG_WAIT).await })
    };
    tokio::time::sleep(PUBLISH_DELAY).await;

    // When the other orchestrator's child and a child nobody started end their turns, and then
    // main's child does
    daemon.a_turn_ends_in(THE_OTHERS_CHILD).await;
    daemon.a_turn_ends_in(NOBODYS_CHILD).await;
    daemon.a_turn_ends_in(MAINS_CHILD).await;

    // Then the wait returns main's child's record alone
    let woken = bounded("main's wait", waiting)
        .await
        .expect("the wait task")
        .expect("the wait answers");
    assert_eq!(
        sessions_of_records(&woken.page.items),
        vec![MAINS_CHILD.to_string()]
    );
}

#[tokio::test]
async fn a_read_started_by_me_skips_the_records_of_sessions_it_did_not_start() {
    // Given turn ends in main's child, the other orchestrator's child and a child nobody started
    let daemon = a_daemon_holding_two_orchestrators_and_their_children();
    daemon.a_turn_ends_in(MAINS_CHILD).await;
    daemon.a_turn_ends_in(THE_OTHERS_CHILD).await;
    daemon.a_turn_ends_in(NOBODYS_CHILD).await;

    // When main reads its own children's records, and then every session's
    let mine = daemon
        .log
        .read(&the_scope_of(MAIN, true), "0", None)
        .expect("the log reads");
    let everyones = daemon
        .log
        .read(&the_scope_of(MAIN, false), "0", None)
        .expect("the log reads");

    // Then the first holds main's child alone, and the second all three
    assert_eq!(
        (
            sessions_of_records(&mine.page.items),
            sessions_of_records(&everyones.page.items),
        ),
        (
            vec![MAINS_CHILD.to_string()],
            vec![
                MAINS_CHILD.to_string(),
                THE_OTHERS_CHILD.to_string(),
                NOBODYS_CHILD.to_string(),
            ],
        )
    );
}

#[tokio::test]
async fn a_record_of_a_session_nobody_started_names_no_orchestrator_and_the_json_leaves_it_out() {
    // Given a session no orchestrator started
    let daemon = a_daemon_holding_two_orchestrators_and_their_children();

    // When its turn ends, and then main's child's does
    daemon.a_turn_ends_in(NOBODYS_CHILD).await;
    daemon.a_turn_ends_in(MAINS_CHILD).await;

    // Then the first record names no orchestrator, and its JSON carries no such key, while the
    // second names main
    let read = daemon
        .log
        .read(&the_scope_of(MAIN, false), "0", None)
        .expect("the log reads");
    let orchestrators: Vec<Option<String>> = read
        .page
        .items
        .iter()
        .map(|r| r.orchestrator_session_id.clone())
        .collect();
    assert_eq!(orchestrators, vec![None, Some(MAIN.to_string())]);
    assert!(record_json(&read.page.items[0])
        .expect("the record serialises")
        .get("orchestrator_session_id")
        .is_none());
}

#[tokio::test]
async fn a_session_whose_changeset_cannot_be_read_is_a_delivery_error_not_a_record_of_nobody() {
    // Given main's child whose changeset exists but cannot be parsed
    let daemon = a_daemon_holding_two_orchestrators_and_their_children();
    let session_dir = sessions_of(daemon._data_dir.path(), ADA)
        .join("sessions")
        .join(MAINS_CHILD);
    std::fs::write(
        session_dir.join("changeset.yaml"),
        "orchestrator_session_id: [unclosed",
    )
    .expect("the corrupt changeset");

    // When its turn's end is delivered to the log
    let delivered = daemon.log.deliver(&a_turn_end_of(MAINS_CHILD)).await;

    // Then the delivery fails naming the unreadable changeset, and nothing is recorded — neither
    // in main's started_by_me view nor as a record that names no orchestrator
    let error = delivered.expect_err("an unreadable changeset is a delivery error");
    assert!(
        format!("{error:#}").contains("unreadable changeset"),
        "the error names the cause: {error:#}"
    );
    let everyones = daemon
        .log
        .read(&the_scope_of(MAIN, false), "0", None)
        .expect("the log reads");
    assert_eq!(
        sessions_of_records(&everyones.page.items),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn the_orchestrator_a_record_names_survives_a_restart() {
    // Given main's child's turn ended on a daemon that then restarts over the same data directory
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    write_session(data_dir.path(), ADA, MAIN, PROJECT);
    write_session(data_dir.path(), ADA, MAINS_CHILD, PROJECT);
    started_by(data_dir.path(), MAINS_CHILD, MAIN);
    let (_, bus) = a_log_over(data_dir.path());
    bus.publish(a_turn_end_of(MAINS_CHILD)).await;

    // When the restarted log is read with started_by_me
    let (restarted, _) = a_log_over(data_dir.path());
    let read = restarted
        .read(&the_scope_of(MAIN, true), "0", None)
        .expect("the log reads");

    // Then the replayed record still names main, and main still sees it
    assert_eq!(
        read.page
            .items
            .iter()
            .map(|r| (r.session_id.clone(), r.orchestrator_session_id.clone()))
            .collect::<Vec<_>>(),
        vec![(MAINS_CHILD.to_string(), Some(MAIN.to_string()))]
    );
}
