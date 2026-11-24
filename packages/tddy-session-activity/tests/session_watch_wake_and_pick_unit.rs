//! A watch wakes its idle orchestrator only when it opts in (`wake_when_idle`), and a stream call
//! may pick a subset of what its watches matched — at most `max_events`, of some kinds, of some
//! sessions — leaving everything it did not pick pending, and never handing again what it
//! acknowledged.
//!
//! The log is the real one over a temporary data directory, fed through a real bus; the store is
//! opened over the same directory. Time is passed in.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

mod project_notification_log_support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use project_notification_log_support::{a_log_over, write_session};
use tddy_daemon_kernel::session_notification::QuestionNotice;
use tddy_session_activity::project_notification_log::{
    ProjectNotificationLog, ProjectNotificationRecord,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::session_watches::stream_call::StreamPick;
use tddy_session_activity::session_watches::{
    SessionWatchStore, StreamBatch, WatchFilter, WatchKind, WatchOptions, WatchOwner,
    STREAM_LEASE_GRACE,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const ORCHESTRATOR: &str = "01900000-0000-7000-8000-AABB00000800";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000801";
const SIBLING: &str = "01900000-0000-7000-8000-AABB00000802";
const NOW: u64 = 1_790_000_000_000;
const AN_HOUR: Duration = Duration::from_secs(3_600);
/// Past [`STREAM_LEASE_GRACE`] in the store's milliseconds: a stream that stopped calling this long
/// ago holds nothing.
const PAST_THE_LEASE: u64 = STREAM_LEASE_GRACE.as_millis() as u64 + 1;

// ── the world ────────────────────────────────────────────────────────────────────────────────

struct AHost {
    data_dir: tempfile::TempDir,
    log: Arc<ProjectNotificationLog>,
    bus: SessionNotificationBus,
    watches: SessionWatchStore,
}

impl AHost {
    /// A host with the orchestrator and two child sessions of the project, its log and watches.
    fn of_one_project() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        for session in [ORCHESTRATOR, CHILD, SIBLING] {
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

    /// The same host's watches as a restarted daemon opens them.
    fn reopened_watches(&self) -> SessionWatchStore {
        SessionWatchStore::open(self.data_dir.path()).expect("the store reopens")
    }

    /// A watch of the orchestrator over `sessions`, matching `kinds`, with `options`.
    async fn a_watch(
        &self,
        sessions: &[&str],
        kinds: &[WatchKind],
        options: WatchOptions,
    ) -> String {
        self.watches
            .create(
                &the_orchestrator(),
                WatchFilter {
                    session_ids: sessions.iter().map(|s| s.to_string()).collect(),
                    started_by_me: false,
                    kinds: kinds.to_vec(),
                },
                options,
                AN_HOUR,
                &self.log,
                NOW,
            )
            .await
            .expect("the watch is created")
            .watch_id
    }

    /// A watch over the child's permission prompts that does not wake its orchestrator.
    async fn a_quiet_watch_of_the_childs_prompts(&self) -> String {
        self.a_watch(&[CHILD], &[WatchKind::Permission], WatchOptions::default())
            .await
    }

    /// A watch over the child's permission prompts that wakes its idle orchestrator.
    async fn a_waking_watch_of_the_childs_prompts(&self) -> String {
        self.a_watch(
            &[CHILD],
            &[WatchKind::Permission],
            WatchOptions {
                wake_when_idle: true,
            },
        )
        .await
    }

    /// `session` asks permission; answered by `request_id`.
    async fn asks_permission(&self, session: &str, request_id: &str) {
        self.bus
            .publish(SessionNotification {
                source: SessionNotificationSource::PendingQuestion,
                text: format!("wants to run Bash ({request_id})"),
                question: Some(QuestionNotice {
                    request_id: request_id.to_string(),
                    kind: "permission".to_string(),
                    ..QuestionNotice::default()
                }),
                ..a_notification_of(session)
            })
            .await;
    }

    /// `session` finishes its turn.
    async fn finishes_its_turn(&self, session: &str) {
        self.bus
            .publish(SessionNotification {
                text: "Session fix-login: Done".to_string(),
                activity_status: Some("Done".to_string()),
                ..a_notification_of(session)
            })
            .await;
    }

    /// One call of a stream over `watch_id`, picking `pick`, at `now`.
    async fn picks(
        &self,
        stream_id: Option<&str>,
        watch_id: &str,
        ack_seq: Option<u64>,
        pick: &StreamPick,
        now: u64,
    ) -> StreamBatch {
        self.watches
            .take_stream_batch_picking(
                &the_orchestrator(),
                stream_id,
                &[watch_id.to_string()],
                ack_seq,
                pick,
                &self.log,
                now,
            )
            .await
            .expect("the stream is served")
    }

    /// What the wake would be handed now, and whether its delivery was called at all.
    async fn the_wake_is_handed(&self, now: u64) -> (bool, Vec<ProjectNotificationRecord>) {
        let handed: Arc<Mutex<Vec<ProjectNotificationRecord>>> = Arc::default();
        let called = Arc::new(AtomicBool::new(false));
        let delivered = self
            .watches
            .deliver_wake_with(&the_orchestrator(), &self.log, now, |delivery| {
                let handed = Arc::clone(&handed);
                let called = Arc::clone(&called);
                async move {
                    called.store(true, Ordering::SeqCst);
                    handed.lock().unwrap().extend(delivery.events);
                    Ok(())
                }
            })
            .await
            .expect("the wake's delivery is not refused");
        assert_eq!(delivered, called.load(Ordering::SeqCst));
        let events = handed.lock().unwrap().clone();
        (delivered, events)
    }
}

fn the_orchestrator() -> WatchOwner {
    WatchOwner {
        project_id: PROJECT.to_string(),
        session_id: ORCHESTRATOR.to_string(),
        os_user: ADA.to_string(),
    }
}

fn a_notification_of(session: &str) -> SessionNotification {
    SessionNotification {
        session_id: session.to_string(),
        os_user: ADA.to_string(),
        label: "fix-login".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: String::new(),
        agent: None,
        question: None,
        ended: None,
        activity_status: None,
        at_unix_ms: NOW,
        message: None,
    }
}

/// Each record's request id, or its session and kind when it carries no question.
fn described(records: &[&ProjectNotificationRecord]) -> Vec<String> {
    records
        .iter()
        .map(|record| match &record.question {
            Some(question) => question.request_id.clone(),
            None => format!(
                "{}:{}",
                if record.session_id == CHILD {
                    "child"
                } else {
                    "sibling"
                },
                WatchKind::of(record).map_or("event", WatchKind::as_str)
            ),
        })
        .collect()
}

fn described_batch(batch: &StreamBatch) -> Vec<String> {
    described(&batch.events.iter().map(|e| &e.record).collect::<Vec<_>>())
}

fn described_records(records: &[ProjectNotificationRecord]) -> Vec<String> {
    described(&records.iter().collect::<Vec<_>>())
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

// ── the wake is opt-in ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_watch_wakes_nobody_unless_it_opts_in() {
    // Given a watch created with no options, and the child's prompt it matched
    let host = AHost::of_one_project();
    host.a_quiet_watch_of_the_childs_prompts().await;
    host.asks_permission(CHILD, "req-1").await;

    // When the wake looks for something to deliver, and then the orchestrator's next answer does
    let wake = host.the_wake_is_handed(NOW + 1).await;
    let on_the_answer = host
        .watches
        .take_delivery(&the_orchestrator(), &host.log, NOW + 2)
        .await;

    // Then the wake is handed nothing and no paste is attempted, and the prompt waits for the
    // orchestrator to collect it
    assert_eq!(
        (wake, described_records(&on_the_answer.events)),
        ((false, Vec::new()), strings(&["req-1"]))
    );
}

#[tokio::test]
async fn a_watch_that_opts_in_is_delivered_to_the_wake() {
    // Given a watch that wakes its idle orchestrator, and the child's prompt it matched
    let host = AHost::of_one_project();
    host.a_waking_watch_of_the_childs_prompts().await;
    host.asks_permission(CHILD, "req-1").await;

    // When the wake looks for something to deliver
    let (delivered, events) = host.the_wake_is_handed(NOW + 1).await;

    // Then it is handed the prompt
    assert_eq!(
        (delivered, described_records(&events)),
        (true, strings(&["req-1"]))
    );
}

#[tokio::test]
async fn the_wake_carries_only_the_events_of_watches_that_opt_in() {
    // Given a waking watch over the child and a quiet one over its sibling, each with a prompt
    let host = AHost::of_one_project();
    host.a_waking_watch_of_the_childs_prompts().await;
    host.a_watch(
        &[SIBLING],
        &[WatchKind::Permission],
        WatchOptions::default(),
    )
    .await;
    host.asks_permission(CHILD, "req-child").await;
    host.asks_permission(SIBLING, "req-sibling").await;

    // When the wake delivers, and then the orchestrator's next answer does
    let (_, woken_with) = host.the_wake_is_handed(NOW + 1).await;
    let on_the_answer = host
        .watches
        .take_delivery(&the_orchestrator(), &host.log, NOW + 2)
        .await;

    // Then the wake names the child's prompt alone, and the sibling's waits for the answer
    assert_eq!(
        (
            described_records(&woken_with),
            described_records(&on_the_answer.events)
        ),
        (strings(&["req-child"]), strings(&["req-sibling"]))
    );
}

#[tokio::test]
async fn only_a_live_watch_that_opts_in_concerns_the_wake() {
    // Given an orchestrator holding a quiet watch
    let host = AHost::of_one_project();
    let quiet = host.a_quiet_watch_of_the_childs_prompts().await;
    let before = host
        .watches
        .has_live_waking_watches(&the_orchestrator(), &host.log, NOW)
        .await;

    // When it renews that watch turning the wake on
    let renewed = host
        .watches
        .renew(&the_orchestrator(), &quiet, AN_HOUR, Some(true), NOW + 1)
        .await
        .expect("the watch is renewed");
    let after = host
        .watches
        .has_live_waking_watches(&the_orchestrator(), &host.log, NOW + 1)
        .await;

    // Then it concerned the wake only once renewed, and the renewal says so
    assert_eq!((before, after, renewed.wake_when_idle), (false, true, true));
}

#[tokio::test]
async fn a_renewal_naming_no_wake_keeps_the_watchs_choice() {
    // Given a waking watch
    let host = AHost::of_one_project();
    let waking = host.a_waking_watch_of_the_childs_prompts().await;

    // When it is renewed with no word on the wake
    let renewed = host
        .watches
        .renew(&the_orchestrator(), &waking, AN_HOUR, None, NOW + 1)
        .await
        .expect("the watch is renewed");

    // Then it still wakes
    assert!(renewed.wake_when_idle);
}

// ── a stream call picks ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_stream_call_hands_at_most_max_events_oldest_first() {
    // Given a stream over the child's prompts, and three prompts waiting
    let host = AHost::of_one_project();
    let watch_id = host.a_quiet_watch_of_the_childs_prompts().await;
    let started = host
        .picks(None, &watch_id, None, &StreamPick::default(), NOW)
        .await;
    for request_id in ["req-1", "req-2", "req-3"] {
        host.asks_permission(CHILD, request_id).await;
    }
    let two = StreamPick {
        max_events: 2,
        ..StreamPick::default()
    };

    // When a call picks at most two, and the next acknowledges them
    let first = host
        .picks(Some(&started.stream_id), &watch_id, None, &two, NOW + 1)
        .await;
    let second = host
        .picks(
            Some(&started.stream_id),
            &watch_id,
            Some(first.next_ack_seq),
            &two,
            NOW + 2,
        )
        .await;

    // Then the first is handed the two oldest, and the second the one left
    assert_eq!(
        (described_batch(&first), described_batch(&second)),
        (strings(&["req-1", "req-2"]), strings(&["req-3"]))
    );
}

#[tokio::test]
async fn a_kind_pick_leaves_the_other_kinds_pending_and_never_hands_twice() {
    // Given a stream over the child's prompts and finished turns, and the child finishing a turn
    // before it asks permission
    let host = AHost::of_one_project();
    let watch_id = host
        .a_watch(
            &[CHILD],
            &[WatchKind::Permission, WatchKind::TurnFinished],
            WatchOptions::default(),
        )
        .await;
    let started = host
        .picks(None, &watch_id, None, &StreamPick::default(), NOW)
        .await;
    let stream = Some(started.stream_id.as_str());
    host.finishes_its_turn(CHILD).await;
    host.asks_permission(CHILD, "req-1").await;

    // When a call picks permission prompts alone, the next acknowledges it picking anything, and
    // the one after acknowledges that
    let prompts = host
        .picks(
            stream,
            &watch_id,
            None,
            &StreamPick {
                kinds: vec![WatchKind::Permission],
                ..StreamPick::default()
            },
            NOW + 1,
        )
        .await;
    let the_rest = host
        .picks(
            stream,
            &watch_id,
            Some(prompts.next_ack_seq),
            &StreamPick::default(),
            NOW + 2,
        )
        .await;
    let after_both = host
        .picks(
            stream,
            &watch_id,
            Some(the_rest.next_ack_seq),
            &StreamPick::default(),
            NOW + 3,
        )
        .await;

    // Then the prompt comes first, the earlier finished turn was left pending for the next call,
    // and nothing is handed twice
    assert_eq!(
        (
            described_batch(&prompts),
            described_batch(&the_rest),
            described_batch(&after_both)
        ),
        (
            strings(&["req-1"]),
            strings(&["child:turn_finished"]),
            Vec::<String>::new()
        )
    );
}

#[tokio::test]
async fn a_session_pick_leaves_the_other_sessions_pending() {
    // Given a stream over both children's prompts, the sibling asking before the child
    let host = AHost::of_one_project();
    let watch_id = host
        .a_watch(
            &[CHILD, SIBLING],
            &[WatchKind::Permission],
            WatchOptions::default(),
        )
        .await;
    let started = host
        .picks(None, &watch_id, None, &StreamPick::default(), NOW)
        .await;
    let stream = Some(started.stream_id.as_str());
    host.asks_permission(SIBLING, "req-sibling").await;
    host.asks_permission(CHILD, "req-child").await;

    // When a call picks the child alone, and the next acknowledges it picking anything
    let the_childs = host
        .picks(
            stream,
            &watch_id,
            None,
            &StreamPick {
                session_ids: vec![CHILD.to_string()],
                ..StreamPick::default()
            },
            NOW + 1,
        )
        .await;
    let the_rest = host
        .picks(
            stream,
            &watch_id,
            Some(the_childs.next_ack_seq),
            &StreamPick::default(),
            NOW + 2,
        )
        .await;

    // Then the child's prompt is handed first and the sibling's is still there for the next call
    assert_eq!(
        (described_batch(&the_childs), described_batch(&the_rest)),
        (strings(&["req-child"]), strings(&["req-sibling"]))
    );
}

#[tokio::test]
async fn what_a_pick_left_pending_rides_on_an_answer_once_the_stream_stops_and_what_it_acknowledged_does_not(
) {
    // Given a stream that picked and acknowledged the child's prompt, leaving its earlier finished
    // turn pending
    let host = AHost::of_one_project();
    let watch_id = host
        .a_watch(
            &[CHILD],
            &[WatchKind::Permission, WatchKind::TurnFinished],
            WatchOptions::default(),
        )
        .await;
    let started = host
        .picks(None, &watch_id, None, &StreamPick::default(), NOW)
        .await;
    let stream = Some(started.stream_id.as_str());
    host.finishes_its_turn(CHILD).await;
    host.asks_permission(CHILD, "req-1").await;
    let only_prompts = StreamPick {
        kinds: vec![WatchKind::Permission],
        ..StreamPick::default()
    };
    let prompts = host
        .picks(stream, &watch_id, None, &only_prompts, NOW + 1)
        .await;
    host.picks(
        stream,
        &watch_id,
        Some(prompts.next_ack_seq),
        &only_prompts,
        NOW + 2,
    )
    .await;

    // When the stream stops calling, its lease lapses, and the daemon restarts before the
    // orchestrator's next answer
    let later = NOW + 2 + PAST_THE_LEASE;
    let restarted = host.reopened_watches();
    let on_the_answer = restarted
        .take_delivery(&the_orchestrator(), &host.log, later)
        .await;

    // Then the answer carries the finished turn alone
    assert_eq!(
        described_records(&on_the_answer.events),
        strings(&["child:turn_finished"])
    );
}

// ── a wake being delivered holds nothing up ──────────────────────────────────────────────────
//
// A wake's delivery pastes into the orchestrator's terminal and waits for its echo, up to 5 s for
// an agent that never echoes. The project's watch state is not held for that long: watch
// operations go on while the paste is in flight, and only the woken owner's own deliveries wait,
// so nothing it is being pasted is handed to it twice.

/// How long a watch operation may take while a wake is in flight before it counts as held up.
const NOT_HELD_UP: Duration = Duration::from_secs(1);

fn another_orchestrator() -> WatchOwner {
    WatchOwner {
        session_id: SIBLING.to_string(),
        ..the_orchestrator()
    }
}

#[tokio::test]
async fn watch_operations_are_not_held_up_while_a_wake_is_being_delivered() {
    // Given a waking watch with the child's prompt waiting, and a wake delivering it to an
    // orchestrator that has not echoed yet
    let host = AHost::of_one_project();
    let watch_id = host.a_waking_watch_of_the_childs_prompts().await;
    host.asks_permission(CHILD, "req-1").await;
    let (delivering, delivery_started) = tokio::sync::oneshot::channel::<()>();
    let (echoed, until_echoed) = tokio::sync::oneshot::channel::<()>();
    let orchestrator = the_orchestrator();
    let wake =
        host.watches
            .deliver_wake_with(&orchestrator, &host.log, NOW + 1, |_delivery| async move {
                delivering.send(()).ok();
                until_echoed.await.ok();
                Ok(())
            });

    // When watch operations are made while the wake is in flight
    let meanwhile = async {
        delivery_started
            .await
            .expect("the wake reaches its delivery");
        let created = tokio::time::timeout(
            NOT_HELD_UP,
            host.watches.create(
                &another_orchestrator(),
                WatchFilter {
                    session_ids: vec![CHILD.to_string()],
                    started_by_me: false,
                    kinds: vec![WatchKind::Permission],
                },
                WatchOptions::default(),
                AN_HOUR,
                &host.log,
                NOW + 2,
            ),
        )
        .await
        .is_ok();
        let listed = tokio::time::timeout(
            NOT_HELD_UP,
            host.watches.list(&the_orchestrator(), &host.log, NOW + 2),
        )
        .await
        .is_ok();
        let renewed = tokio::time::timeout(
            NOT_HELD_UP,
            host.watches
                .renew(&the_orchestrator(), &watch_id, AN_HOUR, None, NOW + 2),
        )
        .await
        .is_ok();
        let others_delivery = tokio::time::timeout(
            NOT_HELD_UP,
            host.watches
                .take_delivery(&another_orchestrator(), &host.log, NOW + 2),
        )
        .await
        .is_ok();
        echoed.send(()).ok();
        (created, listed, renewed, others_delivery)
    };
    let (woke, held_up) = tokio::join!(wake, meanwhile);

    // Then none of them waited for the wake, and the wake still delivered
    assert_eq!(held_up, (true, true, true, true));
    assert_eq!(woke, Ok(true));
}

#[tokio::test]
async fn what_a_wake_is_delivering_is_not_handed_to_its_owner_again_meanwhile() {
    // Given a wake delivering the child's prompt to an orchestrator that has not echoed yet
    let host = AHost::of_one_project();
    host.a_waking_watch_of_the_childs_prompts().await;
    host.asks_permission(CHILD, "req-1").await;
    let (delivering, delivery_started) = tokio::sync::oneshot::channel::<()>();
    let (echoed, until_echoed) = tokio::sync::oneshot::channel::<()>();
    let orchestrator = the_orchestrator();
    let wake =
        host.watches
            .deliver_wake_with(&orchestrator, &host.log, NOW + 1, |_delivery| async move {
                delivering.send(()).ok();
                until_echoed.await.ok();
                Ok(())
            });

    // When the orchestrator collects its watches' events while the wake is in flight
    let meanwhile = async {
        delivery_started
            .await
            .expect("the wake reaches its delivery");
        let collect = host
            .watches
            .take_delivery(&orchestrator, &host.log, NOW + 2);
        tokio::pin!(collect);
        let early = tokio::time::timeout(Duration::from_millis(100), &mut collect).await;
        echoed.send(()).ok();
        match early {
            Ok(delivery) => delivery,
            Err(_) => collect.await,
        }
    };
    let (woke, collected) = tokio::join!(wake, meanwhile);

    // Then the prompt reached it once — by the wake — and the collection hands nothing
    assert_eq!(woke, Ok(true));
    assert_eq!(described_records(&collected.events), Vec::<String>::new());
}

#[tokio::test]
async fn a_stream_call_while_a_wake_is_pasting_is_not_handed_what_the_wake_pastes() {
    // Given a wake delivering the child's prompt to an orchestrator that has not echoed yet
    let host = AHost::of_one_project();
    let watch_id = host.a_waking_watch_of_the_childs_prompts().await;
    host.asks_permission(CHILD, "req-1").await;
    let (delivering, delivery_started) = tokio::sync::oneshot::channel::<()>();
    let (echoed, until_echoed) = tokio::sync::oneshot::channel::<()>();
    let orchestrator = the_orchestrator();
    let wake =
        host.watches
            .deliver_wake_with(&orchestrator, &host.log, NOW + 1, |_delivery| async move {
                delivering.send(()).ok();
                until_echoed.await.ok();
                Ok(())
            });

    // When a stream over the same watch calls while the wake is in flight
    let meanwhile = async {
        delivery_started
            .await
            .expect("the wake reaches its delivery");
        let pick = StreamPick::default();
        let call = host.picks(None, &watch_id, None, &pick, NOW + 2);
        tokio::pin!(call);
        let early = tokio::time::timeout(Duration::from_millis(100), &mut call).await;
        echoed.send(()).ok();
        match early {
            Ok(batch) => batch,
            Err(_) => call.await,
        }
    };
    let (woke, batch) = tokio::join!(wake, meanwhile);

    // Then the prompt reached the orchestrator once — by the wake
    assert_eq!(woke, Ok(true));
    assert_eq!(described_batch(&batch), Vec::<String>::new());
}

#[tokio::test]
async fn a_watch_cancelled_while_a_wake_is_pasting_stays_cancelled() {
    // Given a wake delivering the child's prompt to an orchestrator that has not echoed yet
    let host = AHost::of_one_project();
    let watch_id = host.a_waking_watch_of_the_childs_prompts().await;
    host.asks_permission(CHILD, "req-1").await;
    let (delivering, delivery_started) = tokio::sync::oneshot::channel::<()>();
    let (echoed, until_echoed) = tokio::sync::oneshot::channel::<()>();
    let orchestrator = the_orchestrator();
    let wake =
        host.watches
            .deliver_wake_with(&orchestrator, &host.log, NOW + 1, |_delivery| async move {
                delivering.send(()).ok();
                until_echoed.await.ok();
                Ok(())
            });

    // When the orchestrator cancels the watch while the wake is in flight
    let meanwhile = async {
        delivery_started
            .await
            .expect("the wake reaches its delivery");
        let cancelled = host.watches.cancel(&orchestrator, &watch_id).await;
        echoed.send(()).ok();
        cancelled
    };
    let (woke, cancelled) = tokio::join!(wake, meanwhile);

    // Then the wake still delivered, and committing it did not bring the watch back
    assert_eq!((woke, cancelled), (Ok(true), Ok(())));
    assert_eq!(
        host.watches
            .list(&orchestrator, &host.log, NOW + 2)
            .await
            .len(),
        0
    );
}
