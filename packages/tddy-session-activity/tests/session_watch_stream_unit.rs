//! Streaming a session watch: positions that advance only on acknowledgement, the stream lease,
//! and the two other deliveries skipping a streamed watch (`#activity-wake` 4/5).
//!
//! The log is the real one over a temporary data directory, fed through a real bus; the store is
//! opened over the same directory. Time is passed in, so a lease lapses with no sleep.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md

mod project_notification_log_support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use project_notification_log_support::{a_log_over, write_session};
use tddy_daemon_kernel::session_notification::QuestionNotice;
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::session_watches::{
    SessionWatchStore, StreamBatch, StreamRefusal, WatchFilter, WatchKind, WatchOptions,
    WatchOwner, STREAM_LEASE_GRACE,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const ORCHESTRATOR: &str = "01900000-0000-7000-8000-AABB00000700";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000701";
const NOW: u64 = 1_790_000_000_000;
const AN_HOUR: Duration = Duration::from_secs(3_600);
/// [`STREAM_LEASE_GRACE`] in the store's milliseconds.
const GRACE_MS: u64 = STREAM_LEASE_GRACE.as_millis() as u64;

// ── the world ────────────────────────────────────────────────────────────────────────────────

struct AHost {
    _data_dir: tempfile::TempDir,
    log: Arc<ProjectNotificationLog>,
    bus: SessionNotificationBus,
    watches: SessionWatchStore,
}

impl AHost {
    /// A host with the orchestrator and a child session of the project, its log and its watches.
    fn of_one_project() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        for session in [ORCHESTRATOR, CHILD] {
            write_session(data_dir.path(), ADA, session, PROJECT);
        }
        let (log, bus) = a_log_over(data_dir.path());
        let watches = SessionWatchStore::open(data_dir.path()).expect("the store opens");
        Self {
            _data_dir: data_dir,
            log,
            bus,
            watches,
        }
    }

    /// A watch of the orchestrator over the child's permission prompts, which wakes it when idle:
    /// what a stream holds must reach neither the wake nor a tool answer.
    async fn a_watch_of_the_childs_prompts(&self) -> String {
        self.watches
            .create(
                &the_orchestrator(),
                WatchFilter {
                    session_ids: vec![CHILD.to_string()],
                    started_by_me: false,
                    kinds: vec![WatchKind::Permission],
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

    /// The child asks permission; answered by `request_id`.
    async fn the_child_asks_permission(&self, request_id: &str) {
        self.bus
            .publish(SessionNotification {
                session_id: CHILD.to_string(),
                os_user: ADA.to_string(),
                label: "fix-login".to_string(),
                kind: SessionNotificationKind::AttentionRequired,
                source: SessionNotificationSource::PendingQuestion,
                text: format!("wants to run Bash ({request_id})"),
                agent: None,
                question: Some(QuestionNotice {
                    request_id: request_id.to_string(),
                    kind: "permission".to_string(),
                    ..QuestionNotice::default()
                }),
                ended: None,
                activity_status: None,
                at_unix_ms: NOW,
                message: None,
            })
            .await;
    }

    /// One call of a stream over `watch_id` at `now`.
    async fn streams(
        &self,
        stream_id: Option<&str>,
        watch_id: &str,
        ack_seq: Option<u64>,
        now: u64,
    ) -> Result<StreamBatch, StreamRefusal> {
        self.watches
            .take_stream_batch(
                &the_orchestrator(),
                stream_id,
                &[watch_id.to_string()],
                ack_seq,
                &self.log,
                now,
            )
            .await
    }

    /// The first call of a new stream over `watch_id` at `now`, which must be served.
    async fn starts_a_stream(&self, watch_id: &str, now: u64) -> StreamBatch {
        self.streams(None, watch_id, None, now)
            .await
            .expect("the stream is served")
    }
}

fn the_orchestrator() -> WatchOwner {
    WatchOwner {
        project_id: PROJECT.to_string(),
        session_id: ORCHESTRATOR.to_string(),
        os_user: ADA.to_string(),
    }
}

/// The `request_id` of each event of `batch`, oldest first.
fn request_ids_of(batch: &StreamBatch) -> Vec<String> {
    batch
        .events
        .iter()
        .map(|event| {
            event
                .record
                .question
                .as_ref()
                .map(|q| q.request_id.clone())
                .unwrap_or_default()
        })
        .collect()
}

// ── acknowledgement ──────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_stream_batch_does_not_advance_until_acknowledged() {
    // Given a stream that was handed the child's prompt
    let host = AHost::of_one_project();
    let watch_id = host.a_watch_of_the_childs_prompts().await;
    let first = host.starts_a_stream(&watch_id, NOW).await;
    host.the_child_asks_permission("req-1").await;
    let handed = host
        .streams(Some(&first.stream_id), &watch_id, None, NOW + 1)
        .await
        .expect("the stream is served");

    // When its next call acknowledges nothing, and the one after acknowledges the batch
    let unacknowledged = host
        .streams(Some(&first.stream_id), &watch_id, None, NOW + 2)
        .await
        .expect("the stream is served");
    let acknowledged = host
        .streams(
            Some(&first.stream_id),
            &watch_id,
            Some(handed.next_ack_seq),
            NOW + 3,
        )
        .await
        .expect("the stream is served");

    // Then the prompt is handed again until it is acknowledged, and not after
    assert_eq!(
        (
            request_ids_of(&handed),
            request_ids_of(&unacknowledged),
            request_ids_of(&acknowledged),
        ),
        (
            vec!["req-1".to_string()],
            vec!["req-1".to_string()],
            Vec::<String>::new(),
        )
    );
}

#[tokio::test]
async fn acknowledging_past_what_was_handed_is_refused() {
    // Given a stream that was handed the child's prompt
    let host = AHost::of_one_project();
    let watch_id = host.a_watch_of_the_childs_prompts().await;
    let first = host.starts_a_stream(&watch_id, NOW).await;
    host.the_child_asks_permission("req-1").await;
    let handed = host
        .streams(Some(&first.stream_id), &watch_id, None, NOW + 1)
        .await
        .expect("the stream is served");

    // When its next call acknowledges a seq it was never handed
    let refused = host
        .streams(
            Some(&first.stream_id),
            &watch_id,
            Some(handed.next_ack_seq + 5),
            NOW + 2,
        )
        .await;

    // Then it is refused, naming both, and the prompt is still handed afterwards
    assert_eq!(
        refused,
        Err(StreamRefusal::AckPastHanded {
            ack_seq: handed.next_ack_seq + 5,
            handed: handed.next_ack_seq,
        })
    );
    let after = host
        .streams(Some(&first.stream_id), &watch_id, None, NOW + 3)
        .await
        .expect("the stream is served");
    assert_eq!(request_ids_of(&after), vec!["req-1".to_string()]);
}

// ── the other deliveries skip a streamed watch ───────────────────────────────────────────────

#[tokio::test]
async fn take_delivery_skips_a_streamed_watch() {
    // Given a watch a stream holds, and a prompt it matched
    let host = AHost::of_one_project();
    let watch_id = host.a_watch_of_the_childs_prompts().await;
    let stream = host.starts_a_stream(&watch_id, NOW).await;
    host.the_child_asks_permission("req-1").await;

    // When a tool answer takes its delivery inside the lease
    let delivery = host
        .watches
        .take_delivery(&the_orchestrator(), &host.log, NOW + 1)
        .await;

    // Then it is empty, and the stream is still handed the prompt
    assert!(delivery.is_empty(), "the answer took {delivery:?}");
    let batch = host
        .streams(Some(&stream.stream_id), &watch_id, None, NOW + 2)
        .await
        .expect("the stream is served");
    assert_eq!(request_ids_of(&batch), vec!["req-1".to_string()]);
}

#[tokio::test]
async fn deliver_wake_with_skips_a_streamed_watch() {
    // Given a watch a stream holds, and a prompt it matched
    let host = AHost::of_one_project();
    let watch_id = host.a_watch_of_the_childs_prompts().await;
    let stream = host.starts_a_stream(&watch_id, NOW).await;
    host.the_child_asks_permission("req-1").await;

    // When a wake delivers inside the lease
    let called = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&called);
    let delivered = host
        .watches
        .deliver_wake_with(&the_orchestrator(), &host.log, NOW + 1, |_| async move {
            seen.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

    // Then there was nothing to deliver, the wake was never handed anything, and the stream still is
    assert_eq!(
        (delivered, called.load(Ordering::SeqCst)),
        (Ok(false), false)
    );
    let batch = host
        .streams(Some(&stream.stream_id), &watch_id, None, NOW + 2)
        .await
        .expect("the stream is served");
    assert_eq!(request_ids_of(&batch), vec!["req-1".to_string()]);
}

// ── the lease ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_lease_lapses_after_its_grace_with_no_call() {
    // Given a stream that acknowledged nothing of the child's prompt and then stopped calling
    let host = AHost::of_one_project();
    let watch_id = host.a_watch_of_the_childs_prompts().await;
    let stream = host.starts_a_stream(&watch_id, NOW).await;
    host.the_child_asks_permission("req-1").await;
    host.streams(Some(&stream.stream_id), &watch_id, None, NOW + 1)
        .await
        .expect("the stream is served");
    let last_call = NOW + 1;

    // When the grace passes after its last call
    let streamed_just_before = host
        .watches
        .is_streamed(&the_orchestrator(), &watch_id, last_call + GRACE_MS - 1)
        .await;
    let streamed_after = host
        .watches
        .is_streamed(&the_orchestrator(), &watch_id, last_call + GRACE_MS)
        .await;
    let delivery = host
        .watches
        .take_delivery(&the_orchestrator(), &host.log, last_call + GRACE_MS)
        .await;

    // Then the watch is streamed until then, and its unacknowledged prompt returns to tool answers
    assert_eq!(
        (
            streamed_just_before,
            streamed_after,
            delivery
                .events
                .iter()
                .filter_map(|r| r.question.as_ref().map(|q| q.request_id.clone()))
                .collect::<Vec<_>>(),
        ),
        (true, false, vec!["req-1".to_string()])
    );
}

#[tokio::test]
async fn a_new_stream_takes_over_from_the_old_one() {
    // Given a stream that was handed the child's prompt and has not acknowledged it
    let host = AHost::of_one_project();
    let watch_id = host.a_watch_of_the_childs_prompts().await;
    let old = host.starts_a_stream(&watch_id, NOW).await;
    host.the_child_asks_permission("req-1").await;
    host.streams(Some(&old.stream_id), &watch_id, None, NOW + 1)
        .await
        .expect("the stream is served");

    // When a new stream of the same orchestrator starts on the watch inside the old one's lease
    let new = host.starts_a_stream(&watch_id, NOW + 2).await;
    let old_again = host
        .streams(Some(&old.stream_id), &watch_id, None, NOW + 3)
        .await;

    // Then the new stream holds the watch under its own id, handed the unacknowledged prompt again,
    // and the old stream is refused from its next call, naming the watch
    assert_ne!(new.stream_id, old.stream_id);
    assert_eq!(
        (request_ids_of(&new), old_again),
        (
            vec!["req-1".to_string()],
            Err(StreamRefusal::Superseded {
                watch_id: watch_id.clone()
            }),
        )
    );
}
