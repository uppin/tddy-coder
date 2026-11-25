//! Messages between sessions as activity records: a message record that survives a restart, an
//! inbox that is a read position over the log, an inbox watch that matches the messages addressed
//! to its owner, a message delivered once across reads and watches, the watch kind `message`, and
//! the size limit (`#activity-wake` 5/5).
//!
//! The log is the real one over a temporary data directory, fed through a real bus; the watch store
//! — which keeps the inbox positions beside the watches — is opened over the same directory. Time
//! is passed in.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

mod project_notification_log_support;

use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use project_notification_log_support::{a_log_over, write_session};
use tddy_daemon_kernel::session_notification::SessionMessage;
use tddy_session_activity::project_notification_log::{
    ProjectNotificationLog, ProjectNotificationRecord, PROJECT_NOTIFICATION_CAPACITY,
};
use tddy_session_activity::session_messages::{
    refuse_a_bad_message_text, InboxMessage, InboxRead, MAX_MESSAGE_BYTES,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::session_watches::{
    SessionWatchStore, WatchFilter, WatchKind, WatchOptions, WatchOwner,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const ORCHESTRATOR: &str = "01900000-0000-7000-8000-AABB00000900";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000901";
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
    /// A host with an orchestrator and the child it started, its log and its watch store.
    fn of_one_project() -> Self {
        let data_dir = tempfile::tempdir().unwrap();
        for session in [ORCHESTRATOR, CHILD] {
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

    /// The child sends the orchestrator `text` as message `message_id`, as `PostSessionMessage`
    /// publishes it.
    async fn the_child_messages_the_orchestrator(&self, message_id: &str, text: &str) {
        self.bus
            .publish(a_message(CHILD, ORCHESTRATOR, message_id, text))
            .await;
    }

    /// The log reopened over the same directory, as a restarted daemon opens it.
    fn the_log_after_a_restart(&self) -> Arc<ProjectNotificationLog> {
        a_log_over(self.data_dir.path()).0
    }

    /// A watch of the orchestrator over the messages the child sends.
    async fn a_watch_of_the_childs_messages(&self) -> String {
        self.watches
            .create(
                &the_orchestrator(),
                WatchFilter {
                    session_ids: vec![CHILD.to_string()],
                    started_by_me: false,
                    kinds: vec![WatchKind::Message],
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

    /// The orchestrator sends the child `text` as message `message_id`.
    async fn the_orchestrator_messages_the_child(&self, message_id: &str, text: &str) {
        self.bus
            .publish(a_message(ORCHESTRATOR, CHILD, message_id, text))
            .await;
    }

    /// The child's inbox watch: the messages addressed to it.
    async fn the_childs_inbox_watch(&self) -> String {
        self.watches
            .create(
                &the_child(),
                WatchFilter::inbox_of(CHILD),
                WatchOptions::default(),
                AN_HOUR,
                &self.log,
                NOW,
            )
            .await
            .expect("the inbox watch is created")
            .watch_id
    }

    /// `ReadSessionMessages` for `reader` at `now`, which must be served.
    async fn reads_the_messages_of(&self, reader: &WatchOwner, now: u64) -> InboxRead {
        self.watches
            .read_messages(reader, None, None, &self.log, now)
            .await
            .unwrap_or_else(|refusal| panic!("the read was refused: {refusal}"))
    }
}

fn the_orchestrator() -> WatchOwner {
    WatchOwner {
        project_id: PROJECT.to_string(),
        session_id: ORCHESTRATOR.to_string(),
        os_user: ADA.to_string(),
    }
}

fn the_child() -> WatchOwner {
    WatchOwner {
        session_id: CHILD.to_string(),
        ..the_orchestrator()
    }
}

/// `from`'s message `message_id` to `to`, as the bus carries it.
fn a_message(from: &str, to: &str, message_id: &str, text: &str) -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::Message,
        text: text.to_string(),
        message: Some(SessionMessage {
            to_session_id: to.to_string(),
            message_id: message_id.to_string(),
            reply_to: None,
            text: text.to_string(),
            to_daemon_instance_id: None,
        }),
        ..a_notification_of(from)
    }
}

/// An attention notification of `session_id` with nothing in it yet.
fn a_notification_of(session_id: &str) -> SessionNotification {
    SessionNotification {
        session_id: session_id.to_string(),
        os_user: ADA.to_string(),
        label: "fix-login".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: String::new(),
        agent: None,
        question: None,
        ended: None,
        activity_status: None,
        message: None,
        at_unix_ms: NOW,
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
        message: notification.message,
        at_unix_ms: notification.at_unix_ms,
        orchestrator_session_id: None,
        daemon_instance_id: None,
        origin_seq: None,
        relay_lost: 0,
    }
}

fn message_ids_of(messages: &[InboxMessage]) -> Vec<String> {
    messages.iter().map(|m| m.message_id.clone()).collect()
}

// ── the record ───────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_message_round_trips_through_the_log() {
    // Given a message the child sent the orchestrator, recorded in the log
    let host = AHost::of_one_project();
    host.the_child_messages_the_orchestrator("m-1", "Tests pass; pushed 3f2a1c.")
        .await;

    // When the daemon restarts and the log is read back from disk
    let (records, _) = host
        .the_log_after_a_restart()
        .records_after(PROJECT, ADA, 0);

    // Then the record is the child's message, addressed to the orchestrator, text whole
    assert_eq!(
        records
            .iter()
            .map(|r| (r.source, r.session_id.clone(), r.message.clone()))
            .collect::<Vec<_>>(),
        vec![(
            SessionNotificationSource::Message,
            CHILD.to_string(),
            Some(SessionMessage {
                to_session_id: ORCHESTRATOR.to_string(),
                message_id: "m-1".to_string(),
                reply_to: None,
                text: "Tests pass; pushed 3f2a1c.".to_string(),
                to_daemon_instance_id: None,
            }),
        )]
    );
}

// ── the inbox: a read position over the log ──────────────────────────────────────────────────

#[tokio::test]
async fn the_inbox_position_advances_past_what_is_read() {
    // Given two messages the child sent the orchestrator, read once
    let host = AHost::of_one_project();
    host.the_child_messages_the_orchestrator("m-1", "first")
        .await;
    host.the_child_messages_the_orchestrator("m-2", "second")
        .await;
    let first_read = host.reads_the_messages_of(&the_orchestrator(), NOW).await;

    // When a third arrives, and the orchestrator reads again
    host.the_child_messages_the_orchestrator("m-3", "third")
        .await;
    let second_read = host
        .reads_the_messages_of(&the_orchestrator(), NOW + 1)
        .await;

    // Then each read answers what arrived after the last, and the position stands at the newest
    // message read
    let newest = second_read.messages.last().map(|m| m.seq);
    assert_eq!(
        (
            message_ids_of(&first_read.messages),
            message_ids_of(&second_read.messages),
            Some(host.watches.inbox_position(&the_orchestrator()).await),
        ),
        (
            vec!["m-1".to_string(), "m-2".to_string()],
            vec!["m-3".to_string()],
            newest,
        )
    );
}

/// Findings: docs/dev/changesets/2026-11-22-coder-target-back-channel.md, problem 1
/// — on 2026-11-22 an orchestrator that had never read its inbox was answered `lost: 934`, then
/// `lost: 935`: every activity record the project log had evicted, none of them a message to it.
#[tokio::test]
async fn a_read_counts_as_lost_only_evicted_messages_addressed_to_the_reader() {
    // Given a project busy enough that its log evicted records — none of them messages — before
    // the child's one message to the orchestrator, which never read its inbox
    let host = AHost::of_one_project();
    for _ in 0..PROJECT_NOTIFICATION_CAPACITY + 5 {
        host.bus.publish(a_notification_of(CHILD)).await;
    }
    host.the_child_messages_the_orchestrator("m-1", "Step 1 done.")
        .await;

    // When the orchestrator reads its messages
    let read = host.reads_the_messages_of(&the_orchestrator(), NOW).await;

    // Then it gets the message, and nothing addressed to it was lost
    assert_eq!(
        (message_ids_of(&read.messages), read.lost),
        (vec!["m-1".to_string()], 0)
    );
}

#[tokio::test]
async fn an_inbox_watch_matches_only_the_messages_addressed_to_its_owner() {
    // Given the child's inbox watch
    let host = AHost::of_one_project();
    host.the_childs_inbox_watch().await;

    // When the orchestrator messages the child, and the child messages the orchestrator
    host.the_orchestrator_messages_the_child("m-1", "Rebase on main.")
        .await;
    host.the_child_messages_the_orchestrator("m-2", "Done.")
        .await;
    let delivered = host
        .watches
        .take_delivery(&the_child(), &host.log, NOW + 1)
        .await;

    // Then the child's watch delivers the message addressed to it, and not the one it sent
    assert_eq!(
        delivered
            .events
            .iter()
            .filter_map(|record| record.message.as_ref().map(|m| m.message_id.clone()))
            .collect::<Vec<_>>(),
        vec!["m-1".to_string()]
    );
}

#[tokio::test]
async fn a_message_read_through_the_inbox_is_not_delivered_by_a_watch() {
    // Given an orchestrator watching the messages its child sends, and a message the child sent it
    let host = AHost::of_one_project();
    host.a_watch_of_the_childs_messages().await;
    host.the_child_messages_the_orchestrator("m-1", "Want me to open the PR?")
        .await;

    // When the orchestrator reads its messages, and then its watches deliver
    let read = host
        .reads_the_messages_of(&the_orchestrator(), NOW + 1)
        .await;
    let delivered = host
        .watches
        .take_delivery(&the_orchestrator(), &host.log, NOW + 2)
        .await;

    // Then the message was read once, and the watch has nothing more to deliver
    assert_eq!(
        (message_ids_of(&read.messages), delivered.events),
        (vec!["m-1".to_string()], Vec::new())
    );
}

#[tokio::test]
async fn a_message_a_watch_delivered_is_not_read_again() {
    // Given an orchestrator watching the messages its child sends, whose watch delivered the
    // child's message to it
    let host = AHost::of_one_project();
    host.a_watch_of_the_childs_messages().await;
    host.the_child_messages_the_orchestrator("m-1", "Want me to open the PR?")
        .await;
    let delivered = host
        .watches
        .take_delivery(&the_orchestrator(), &host.log, NOW + 1)
        .await;

    // When the orchestrator then reads its messages
    let read = host
        .reads_the_messages_of(&the_orchestrator(), NOW + 2)
        .await;

    // Then the watch delivered it, and the read does not answer it again
    assert_eq!(
        (delivered.events.len(), message_ids_of(&read.messages)),
        (1, Vec::<String>::new())
    );
}

// ── the watch kind ───────────────────────────────────────────────────────────────────────────

#[test]
fn the_message_kind_matches_only_message_records() {
    // Given one record of every source a session raises
    let records = [
        SessionNotificationSource::ActivityStatus,
        SessionNotificationSource::AgentToolCall,
        SessionNotificationSource::Presenter,
        SessionNotificationSource::Agent,
        SessionNotificationSource::PendingQuestion,
        SessionNotificationSource::SessionEnded,
    ]
    .map(|source| {
        a_record_of(SessionNotification {
            source,
            activity_status: Some("Done".to_string()),
            ..a_notification_of(CHILD)
        })
    })
    .into_iter()
    .chain([a_record_of(a_message(CHILD, ORCHESTRATOR, "m-1", "done"))]);

    // When each record's kind is read
    let is_a_message: Vec<bool> = records
        .map(|record| WatchKind::of(&record) == Some(WatchKind::Message))
        .collect();

    // Then only the message record is of the kind `message`
    assert_eq!(
        is_a_message,
        vec![false, false, false, false, false, false, true]
    );
}

// ── the text ─────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_message_over_eight_kib_is_refused() {
    // Given a text of exactly 8 KiB, one a byte longer, and an empty one
    let at_the_limit = "a".repeat(MAX_MESSAGE_BYTES);
    let over_the_limit = "a".repeat(MAX_MESSAGE_BYTES + 1);

    // When each is checked
    let verdicts = (
        refuse_a_bad_message_text(&at_the_limit).is_ok(),
        refuse_a_bad_message_text(&over_the_limit),
        refuse_a_bad_message_text("").is_err(),
    );

    // Then 8 KiB is accepted, a byte more is refused naming the limit, and nothing is refused
    assert!(verdicts.0, "a text of exactly 8 KiB is accepted");
    assert!(
        verdicts
            .1
            .as_ref()
            .is_err_and(|refusal| refusal.contains("8 KiB")),
        "a text over 8 KiB is refused naming the limit: {:?}",
        verdicts.1
    );
    assert!(verdicts.2, "an empty text is refused");
}
