//! An orchestrator's session watches over its project's notification log: which records each kind
//! matches, delivery exactly once, the limits, endings and persistence (`#activity-wake` 2/3).
//!
//! The log is the real one over a temporary data directory, fed through a real bus; the store is
//! opened over the same directory, and "a restart" is a second store opened there. Time is passed
//! in, so an expiry needs no sleep.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

mod project_notification_log_support;

use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use project_notification_log_support::{a_log_over, write_session};
use tddy_daemon_kernel::session_notification::{QuestionNotice, SessionEndNotice};
use tddy_session_activity::project_notification_log::{
    ProjectNotificationLog, ProjectNotificationRecord, PROJECT_NOTIFICATION_CAPACITY,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::session_watches::{
    SessionWatchStore, WatchEnd, WatchEndReason, WatchFilter, WatchKind, WatchOptions, WatchOwner,
    MAX_EVENTS_PER_ANSWER,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const ORCHESTRATOR: &str = "01900000-0000-7000-8000-AABB00000600";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000601";
const OTHER: &str = "01900000-0000-7000-8000-AABB00000602";
const NOW: u64 = 1_790_000_000_000;
const AN_HOUR: Duration = Duration::from_secs(3_600);

// ── the world ────────────────────────────────────────────────────────────────────────────────

struct AHost {
    data_dir: tempfile::TempDir,
    log: Arc<ProjectNotificationLog>,
    bus: SessionNotificationBus,
    watches: SessionWatchStore,
}

impl AHost {
    /// A host with the orchestrator and two sessions of the project, its log and its watches.
    fn of_one_project() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        for session in [ORCHESTRATOR, CHILD, OTHER] {
            write_session(data_dir.path(), ADA, session, PROJECT);
        }
        let (log, bus) = a_log_over(data_dir.path());
        let watches = SessionWatchStore::open(data_dir.path()).expect("the store opens");
        Self {
            data_dir,
            log,
            bus,
            watches,
        }
    }

    /// The daemon restarts: log and store opened again over the same directory.
    fn restarts(&mut self) {
        let (log, bus) = a_log_over(self.data_dir.path());
        self.log = log;
        self.bus = bus;
        self.watches = SessionWatchStore::open(self.data_dir.path()).expect("the store reopens");
    }

    async fn watch(&self, session_ids: &[&str], kinds: &[WatchKind]) -> String {
        self.watches
            .create(
                &the_orchestrator(),
                WatchFilter {
                    session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
                    started_by_me: false,
                    kinds: kinds.to_vec(),
                },
                WatchOptions::default(),
                AN_HOUR,
                &self.log,
                NOW,
            )
            .await
            .expect("the watch is created")
            .watch_id
    }

    /// A watch like [`Self::watch`] that wakes its orchestrator when idle: what the wake delivers.
    async fn waking_watch(&self, session_ids: &[&str], kinds: &[WatchKind]) -> String {
        self.watches
            .create(
                &the_orchestrator(),
                WatchFilter {
                    session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
                    started_by_me: false,
                    kinds: kinds.to_vec(),
                },
                WatchOptions {
                    wake_when_idle: true,
                },
                AN_HOUR,
                &self.log,
                NOW,
            )
            .await
            .expect("the watch is created")
            .watch_id
    }

    async fn publishes(&self, notification: SessionNotification) {
        self.bus.publish(notification).await;
    }

    async fn delivery_at(&self, now: u64) -> tddy_session_activity::session_watches::WatchDelivery {
        self.watches
            .take_delivery(&the_orchestrator(), &self.log, now)
            .await
    }
}

fn the_orchestrator() -> WatchOwner {
    WatchOwner {
        project_id: PROJECT.to_string(),
        session_id: ORCHESTRATOR.to_string(),
        os_user: ADA.to_string(),
    }
}

// ── what sessions raise ──────────────────────────────────────────────────────────────────────

fn a_notification_of(session_id: &str, text: &str) -> SessionNotification {
    SessionNotification {
        session_id: session_id.to_string(),
        os_user: ADA.to_string(),
        label: "fix-login".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: text.to_string(),
        agent: None,
        question: None,
        ended: None,
        activity_status: None,
        at_unix_ms: NOW,
        message: None,
    }
}

fn a_status(session_id: &str, status: &str) -> SessionNotification {
    SessionNotification {
        activity_status: Some(status.to_string()),
        ..a_notification_of(session_id, status)
    }
}

/// `session_id`'s finished turn, told apart by `text`.
fn a_finished_turn(session_id: &str, text: &str) -> SessionNotification {
    SessionNotification {
        activity_status: Some("Done".to_string()),
        ..a_notification_of(session_id, text)
    }
}

fn a_question(session_id: &str, kind: &str, request_id: &str) -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::PendingQuestion,
        question: Some(QuestionNotice {
            request_id: request_id.to_string(),
            kind: kind.to_string(),
            ..QuestionNotice::default()
        }),
        ..a_notification_of(session_id, request_id)
    }
}

fn an_end(session_id: &str) -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::SessionEnded,
        ended: Some(SessionEndNotice {
            outcome: "exit code 0".to_string(),
            exit_code: Some(0),
        }),
        ..a_notification_of(session_id, "ended")
    }
}

/// The record the log keeps for `notification`.
fn a_record_of(notification: SessionNotification) -> ProjectNotificationRecord {
    ProjectNotificationRecord {
        seq: 1,
        session_id: notification.session_id,
        label: notification.label,
        kind: notification.kind,
        source: notification.source,
        text: notification.text,
        agent: notification.agent,
        question: notification.question,
        ended: notification.ended,
        activity_status: notification.activity_status,
        at_unix_ms: notification.at_unix_ms,
        orchestrator_session_id: None,
        daemon_instance_id: None,
        origin_seq: None,
        relay_lost: 0,
        message: None,
    }
}

fn texts(records: &[ProjectNotificationRecord]) -> Vec<String> {
    records.iter().map(|r| r.text.clone()).collect()
}

// ── kinds ────────────────────────────────────────────────────────────────────────────────────

#[test]
fn each_kind_matches_only_its_records() {
    // Given one record of each sort a session raises
    let presenter = SessionNotification {
        source: SessionNotificationSource::Presenter,
        ..a_notification_of(CHILD, "elicitation")
    };
    let agent = SessionNotification {
        source: SessionNotificationSource::Agent,
        ..a_notification_of(CHILD, "agent")
    };
    let status_with_its_question = SessionNotification {
        question: a_question(CHILD, "select", "q-1").question,
        ..a_status(CHILD, "WaitingForInput")
    };
    let records = vec![
        a_question(CHILD, "select", "q-1"),
        a_question(CHILD, "permission", "p-1"),
        presenter,
        a_status(CHILD, "Done"),
        a_status(CHILD, "WaitingForInput"),
        agent,
        an_end(CHILD),
        status_with_its_question,
        a_notification_of(CHILD, "a status record written before records carried one"),
    ];

    // When each is classified
    let kinds: Vec<Option<WatchKind>> = records
        .into_iter()
        .map(|n| WatchKind::of(&a_record_of(n)))
        .collect();

    // Then each is its kind; a status repeating a question, and one with no status, are none
    assert_eq!(
        kinds,
        vec![
            Some(WatchKind::Question),
            Some(WatchKind::Permission),
            Some(WatchKind::Question),
            Some(WatchKind::TurnFinished),
            Some(WatchKind::WaitingForInput),
            Some(WatchKind::AgentNotification),
            Some(WatchKind::Ended),
            None,
            None,
        ]
    );
}

#[test]
fn the_kinds_are_named_as_the_tools_name_them() {
    assert_eq!(
        WatchKind::ALL.map(WatchKind::as_str),
        tddy_tool_engine::project_home_watch_tools::WATCH_KINDS
    );
}

// ── delivery ─────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_new_watch_sees_only_what_happens_after_it() {
    // Given a record raised before the watch existed
    let host = AHost::of_one_project();
    host.publishes(a_status(CHILD, "Done")).await;

    // When the watch is created and another record is raised
    host.watch(&[CHILD], &WatchKind::ALL).await;
    host.publishes(a_status(CHILD, "WaitingForInput")).await;

    // Then only the later one is delivered
    assert_eq!(
        texts(&host.delivery_at(NOW).await.events),
        vec!["WaitingForInput"]
    );
}

#[tokio::test]
async fn two_watches_matching_one_record_deliver_it_once() {
    // Given two watches that both match the child's finished turn
    let host = AHost::of_one_project();
    host.watch(&[CHILD], &[WatchKind::TurnFinished]).await;
    host.watch(&[CHILD, OTHER], &WatchKind::ALL).await;

    // When the child finishes a turn
    host.publishes(a_status(CHILD, "Done")).await;

    // Then it is delivered once, and not again
    assert_eq!(texts(&host.delivery_at(NOW).await.events), vec!["Done"]);
    assert!(host.delivery_at(NOW).await.is_empty());
}

#[tokio::test]
async fn concurrent_deliveries_never_hand_out_the_same_event_twice() {
    // Given a watch with twenty events waiting
    let host = Arc::new(AHost::of_one_project());
    host.watch(&[CHILD], &WatchKind::ALL).await;
    for n in 0..20 {
        host.publishes(a_finished_turn(CHILD, &format!("Done-{n}")))
            .await;
    }
    // When eight calls take their deliveries at once
    let takers: Vec<_> = (0..8)
        .map(|_| {
            let host = Arc::clone(&host);
            tokio::spawn(async move { host.delivery_at(NOW).await.events })
        })
        .collect();
    let mut delivered = Vec::new();
    for taker in takers {
        delivered.extend(texts(&taker.await.unwrap()));
    }

    // Then every event was handed out exactly once between them
    delivered.sort();
    let mut expected: Vec<String> = (0..20).map(|n| format!("Done-{n}")).collect();
    expected.sort();
    assert_eq!(delivered, expected);
}

#[tokio::test]
async fn a_delivery_holds_at_most_fifty_and_the_rest_wait() {
    // Given sixty events waiting
    let host = AHost::of_one_project();
    host.watch(&[CHILD], &WatchKind::ALL).await;
    for n in 0..60 {
        host.publishes(a_finished_turn(CHILD, &format!("Done-{n}")))
            .await;
    }

    // When two deliveries are taken
    let first = host.delivery_at(NOW).await.events.len();
    let second = host.delivery_at(NOW).await.events.len();

    // Then the first holds fifty and the second the remaining ten
    assert_eq!((first, second), (MAX_EVENTS_PER_ANSWER, 10));
}

#[tokio::test]
async fn events_the_log_evicted_before_delivery_are_counted_lost() {
    // Given a watch, and more matched records than the log keeps
    let host = AHost::of_one_project();
    host.watch(&[CHILD], &WatchKind::ALL).await;
    for n in 0..(PROJECT_NOTIFICATION_CAPACITY + 5) {
        host.publishes(a_finished_turn(CHILD, &format!("Done-{n}")))
            .await;
    }

    // When a delivery is taken
    let delivery = host.delivery_at(NOW).await;

    // Then the five evicted are counted lost, and the oldest kept is delivered first
    assert_eq!(
        (
            delivery.lost,
            delivery.events.first().map(|r| r.text.clone())
        ),
        (5, Some("Done-5".to_string()))
    );
}

// ── endings ──────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_expired_watch_ends_as_expired() {
    // Given a watch an hour old
    let host = AHost::of_one_project();
    let watch_id = host.watch(&[CHILD], &WatchKind::ALL).await;

    // When a delivery is taken past its expiry
    let later = NOW + AN_HOUR.as_millis() as u64 + 1;
    let delivery = host.delivery_at(later).await;

    // Then it is reported ended, once, and no longer live
    assert_eq!(
        delivery.ended,
        vec![WatchEnd {
            watch_id,
            reason: WatchEndReason::Expired,
        }]
    );
    assert!(host.delivery_at(later).await.is_empty());
    assert!(
        !host
            .watches
            .has_live_watches(&the_orchestrator(), &host.log, later)
            .await
    );
}

#[tokio::test]
async fn a_watch_ends_when_its_orchestrators_agent_ends() {
    // Given a live watch
    let host = AHost::of_one_project();
    let watch_id = host.watch(&[CHILD], &WatchKind::ALL).await;
    assert!(
        host.watches
            .has_live_watches(&the_orchestrator(), &host.log, NOW)
            .await
    );

    // When the orchestrator's own agent ends
    host.publishes(an_end(ORCHESTRATOR)).await;

    // Then its watch has ended for that reason
    assert!(
        !host
            .watches
            .has_live_watches(&the_orchestrator(), &host.log, NOW)
            .await
    );
    assert_eq!(
        host.delivery_at(NOW).await.ended,
        vec![WatchEnd {
            watch_id,
            reason: WatchEndReason::OrchestratorEnded,
        }]
    );
}

// ── persistence ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn watches_and_their_positions_survive_reopening_the_store() {
    // Given a watch that has delivered one event
    let mut host = AHost::of_one_project();
    host.watch(&[CHILD], &WatchKind::ALL).await;
    host.publishes(a_status(CHILD, "Done")).await;
    assert_eq!(texts(&host.delivery_at(NOW).await.events), vec!["Done"]);

    // When the daemon restarts, and the child raises another
    host.restarts();
    host.publishes(a_status(CHILD, "WaitingForInput")).await;

    // Then the watch is still live and delivers only the new one
    assert_eq!(
        texts(&host.delivery_at(NOW).await.events),
        vec!["WaitingForInput"]
    );
}

#[test]
fn a_status_notification_carries_the_status_it_was_raised_for() {
    // When a finished turn and a wait for input are turned into notifications
    let raised = ["Done", "WaitingForInput"].map(|status| {
        tddy_session_activity::session_notifications::notification_for_activity_status(
            CHILD,
            ADA,
            "fix-login",
            "claude-cli",
            status,
            NOW,
        )
        .expect("both statuses raise a notification")
        .activity_status
    });

    // Then each carries its status, so a watch can tell them apart
    assert_eq!(
        raised,
        [
            Some("Done".to_string()),
            Some("WaitingForInput".to_string())
        ]
    );
}

// ── committed delivery (`#activity-wake` 3/3) ────────────────────────────────────────────────

#[tokio::test]
async fn a_failed_delivery_moves_no_position() {
    // Given a watch with a finished turn waiting
    let host = AHost::of_one_project();
    host.waking_watch(&[CHILD], &WatchKind::ALL).await;
    host.publishes(a_finished_turn(CHILD, "turn 1")).await;

    // When a delivery is handed to a paste that fails
    let outcome = host
        .watches
        .deliver_wake_with(&the_orchestrator(), &host.log, NOW, |delivery| async move {
            assert_eq!(texts(&delivery.events), vec!["turn 1"]);
            Err("the terminal is gone".to_string())
        })
        .await;

    // Then the failure is reported and the event is still waiting
    assert_eq!(outcome, Err("the terminal is gone".to_string()));
    assert_eq!(texts(&host.delivery_at(NOW).await.events), vec!["turn 1"]);
}

#[tokio::test]
async fn a_successful_delivery_moves_positions_as_take_delivery_does() {
    // Given a watch with a finished turn waiting
    let host = AHost::of_one_project();
    host.waking_watch(&[CHILD], &WatchKind::ALL).await;
    host.publishes(a_finished_turn(CHILD, "turn 1")).await;

    // When a delivery is handed to a paste that succeeds
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let outcome = {
        let seen = Arc::clone(&seen);
        host.watches
            .deliver_wake_with(&the_orchestrator(), &host.log, NOW, |delivery| async move {
                seen.lock().unwrap().extend(texts(&delivery.events));
                Ok(())
            })
            .await
    };

    // Then it was delivered once, and nothing waits any more
    assert_eq!(
        (outcome, seen.lock().unwrap().clone()),
        (Ok(true), vec!["turn 1".to_string()])
    );
    assert!(host.delivery_at(NOW).await.is_empty());
}

#[tokio::test]
async fn nothing_waiting_never_calls_the_delivery() {
    // Given a watch with nothing waiting
    let host = AHost::of_one_project();
    host.waking_watch(&[CHILD], &WatchKind::ALL).await;

    // When a delivery is attempted
    let outcome = host
        .watches
        .deliver_wake_with(&the_orchestrator(), &host.log, NOW, |_| async {
            panic!("an empty delivery was handed over")
        })
        .await;

    // Then nothing was delivered
    assert_eq!(outcome, Ok(false));
}

// ── relay gaps (n5, cross-host watches) ──────────────────────────────────────────────────────

/// A remote host's batch: `count` turn ends of a child it holds whose orchestrator is ours, after
/// `lost` records it evicted before relaying them.
fn a_relayed_batch(
    from_seq: u64,
    count: u64,
    lost: u64,
) -> tddy_session_activity::project_notification_log::RelayedBatch {
    let records: Vec<ProjectNotificationRecord> = (from_seq..from_seq + count)
        .map(|seq| ProjectNotificationRecord {
            seq,
            session_id: "01900000-0000-7000-8000-AABB00000699".to_string(),
            label: "remote child".to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::ActivityStatus,
            text: format!("turn {seq}"),
            agent: None,
            question: None,
            ended: None,
            activity_status: Some("Done".to_string()),
            message: None,
            at_unix_ms: NOW,
            orchestrator_session_id: Some(ORCHESTRATOR.to_string()),
            daemon_instance_id: None,
            origin_seq: None,
            relay_lost: 0,
        })
        .collect();
    tddy_session_activity::project_notification_log::RelayedBatch {
        origin: "host-b".to_string(),
        project_id: PROJECT.to_string(),
        os_user: ADA.to_string(),
        records,
        lost,
        through_seq: from_seq + count - 1,
    }
}

impl AHost {
    async fn started_by_me_watch(&self) {
        self.watches
            .create(
                &the_orchestrator(),
                WatchFilter {
                    session_ids: Vec::new(),
                    started_by_me: true,
                    kinds: vec![WatchKind::TurnFinished],
                },
                WatchOptions::default(),
                AN_HOUR,
                &self.log,
                NOW,
            )
            .await
            .expect("the watch is created");
    }

    /// Every delivery until one is empty: the events' count and the lost summed.
    async fn drained(&self) -> (usize, u64) {
        let (mut events, mut lost) = (0, 0);
        loop {
            let delivery = self.delivery_at(NOW).await;
            if delivery.is_empty() {
                return (events, lost);
            }
            events += delivery.events.len();
            lost += delivery.lost;
        }
    }
}

#[tokio::test]
async fn a_relay_gap_is_reported_lost_once() {
    // Given a watch over the sessions the orchestrator started
    let host = AHost::of_one_project();
    host.started_by_me_watch().await;

    // When another host relays two records after evicting four
    host.log.append_relayed(a_relayed_batch(10, 2, 4)).unwrap();

    // Then the watch delivers the two and reports the four lost, once
    assert_eq!(host.drained().await, (2, 4));
}

#[tokio::test]
async fn a_relay_gap_this_hosts_window_evicted_is_still_reported() {
    // Given a watch over the sessions the orchestrator started
    let host = AHost::of_one_project();
    host.started_by_me_watch().await;

    // When another host relays, after evicting five, more records than this host's window keeps
    let capacity = PROJECT_NOTIFICATION_CAPACITY as u64;
    host.log
        .append_relayed(a_relayed_batch(6, capacity, 5))
        .unwrap();

    // Then every record is accounted for: delivered, or counted lost — the five the other host
    // evicted among them
    let (events, lost) = host.drained().await;
    assert_eq!(
        events as u64 + lost,
        capacity + 5,
        "{events} delivered, {lost} lost"
    );
}
