//! Acceptance: a project's attention notifications are kept in a durable log, addressed by cursor,
//! that the project-home agent waits on and reads (`#home-inbox` 7/8).
//!
//! The path under test is the real log registered on a real bus, over a real data directory:
//! publishing is what a daemon's publish sites do (`SessionNotificationBus::publish`), and the
//! assertions are what a wait or a read then answers. "A restart" is a second log opened over the
//! first one's directory. Every wait here is bounded: a wait that never returns fails its test
//! instead of hanging it.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Waiting for notifications.
//! Changeset: docs/dev/changesets/2026-10-29-home-inbox-home-wait.md

mod project_notification_log_support;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use project_notification_log_support::{a_log_over, write_session};
use tddy_daemon_kernel::session_notification::QuestionNotice;
use tddy_session_activity::project_notification_log::{
    clamp_wait, NotificationRead, NotificationWait, ProjectNotificationLog,
    ProjectNotificationScope, MAX_NOTIFICATION_WAIT, PROJECT_NOTIFICATION_CAPACITY,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};

const PROJECT_A: &str = "project-a";
const PROJECT_B: &str = "project-b";
const ADA: &str = "ada";
const GRACE: &str = "grace";
const HOME: &str = "01900000-0000-7000-8000-AABB00000300";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000301";
const OTHER_PROJECTS_SESSION: &str = "01900000-0000-7000-8000-AABB00000302";
const OTHER_USERS_SESSION: &str = "01900000-0000-7000-8000-AABB00000303";

/// How long a test lets a wait run before it counts as hung.
const BOUND: Duration = Duration::from_secs(10);
/// A wait long enough that only a published record ends it.
const LONG_WAIT: Duration = Duration::from_secs(8);
/// How long after a wait begins its record is published.
const PUBLISH_DELAY: Duration = Duration::from_millis(100);

async fn bounded<T>(what: &str, future: impl Future<Output = T>) -> T {
    tokio::time::timeout(BOUND, future)
        .await
        .unwrap_or_else(|_| panic!("{what} did not finish within {BOUND:?}"))
}

// ── the world: a daemon's data directory, its sessions, and the log on its bus ───────────────

/// One daemon: a data directory holding each OS user's sessions, and the log registered on a bus.
struct ADaemon {
    data_dir: tempfile::TempDir,
    log: Arc<ProjectNotificationLog>,
    bus: SessionNotificationBus,
}

impl ADaemon {
    /// A daemon holding the home session of project A, a child of it, a session of project B and a
    /// session of another OS user in project A.
    fn holding_the_sessions_of_two_projects_and_two_users() -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        write_session(data_dir.path(), ADA, HOME, PROJECT_A);
        write_session(data_dir.path(), ADA, CHILD, PROJECT_A);
        write_session(data_dir.path(), ADA, OTHER_PROJECTS_SESSION, PROJECT_B);
        write_session(data_dir.path(), GRACE, OTHER_USERS_SESSION, PROJECT_A);
        let (log, bus) = a_log_over(data_dir.path());
        Self { data_dir, log, bus }
    }

    async fn publishes(&self, notification: SessionNotification) {
        self.bus.publish(notification).await;
    }

    /// The daemon restarts: its log is opened again over the same data directory, and the sessions
    /// publish onto the new bus.
    fn restarts(&mut self) {
        let (log, bus) = a_log_over(self.data_dir.path());
        self.log = log;
        self.bus = bus;
    }
}

// ── builders ─────────────────────────────────────────────────────────────────────────────────

/// An attention notification of `session_id`, owned by `os_user`, saying `text`.
fn an_attention_notification(os_user: &str, session_id: &str, text: &str) -> SessionNotification {
    SessionNotification {
        session_id: session_id.to_string(),
        os_user: os_user.to_string(),
        label: "my-feature-branch".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: text.to_string(),
        agent: None,
        question: None,
        at_unix_ms: 1_790_000_000_000,
    }
}

/// What the home agent of project A, as `ada`, may see: everything of the project but its own
/// session.
fn the_home_agents_scope() -> ProjectNotificationScope {
    ProjectNotificationScope {
        project_id: PROJECT_A.to_string(),
        os_user: ADA.to_string(),
        exclude_session_id: HOME.to_string(),
    }
}

// ── reading what came back ───────────────────────────────────────────────────────────────────

trait TheRecordsReturned {
    fn texts(&self) -> Vec<String>;
    fn seqs(&self) -> Vec<u64>;
}

impl TheRecordsReturned for NotificationWait {
    fn texts(&self) -> Vec<String> {
        self.page.items.iter().map(|r| r.text.clone()).collect()
    }
    fn seqs(&self) -> Vec<u64> {
        self.page.items.iter().map(|r| r.seq).collect()
    }
}

impl TheRecordsReturned for NotificationRead {
    fn texts(&self) -> Vec<String> {
        self.page.items.iter().map(|r| r.text.clone()).collect()
    }
    fn seqs(&self) -> Vec<u64> {
        self.page.items.iter().map(|r| r.seq).collect()
    }
}

/// The cursor of everything `daemon`'s log holds now, for project A as the home agent sees it.
fn the_cursor_after_everything_so_far(daemon: &ADaemon) -> String {
    daemon
        .log
        .read(&the_home_agents_scope(), "", None)
        .expect("the log is read")
        .page
        .next_cursor
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_notification_published_after_the_wait_began_returns_the_wait_with_it_and_a_new_cursor() {
    // Given a project whose log already holds one record, and the cursor that read it
    let daemon = ADaemon::holding_the_sessions_of_two_projects_and_two_users();
    daemon
        .publishes(an_attention_notification(
            ADA,
            CHILD,
            "an earlier turn finished",
        ))
        .await;
    let cursor = the_cursor_after_everything_so_far(&daemon);

    // When the home agent begins waiting from that cursor, and a child then asks for input
    let waiting = {
        let log = Arc::clone(&daemon.log);
        let from = cursor.clone();
        tokio::spawn(async move { log.wait(&the_home_agents_scope(), &from, LONG_WAIT).await })
    };
    tokio::time::sleep(PUBLISH_DELAY).await;
    daemon
        .publishes(an_attention_notification(ADA, CHILD, "needs your input"))
        .await;

    // Then the wait returns with that record alone, not timed out
    let woken = bounded("the wait", waiting)
        .await
        .expect("the wait task ran to completion")
        .expect("the wait is answered");
    assert_eq!(
        (woken.texts(), woken.timed_out),
        (vec!["needs your input".to_string()], false),
        "what the wait returned"
    );
    // And its cursor moved past the record, so waiting from it returns only what comes later
    assert!(
        woken.page.next_cursor != cursor,
        "the cursor did not move: {}",
        woken.page.next_cursor
    );
    assert_eq!(
        woken.page.next_cursor,
        woken.seqs()[0].to_string(),
        "the new cursor is the returned record's seq"
    );
}

#[tokio::test]
async fn a_wait_with_nothing_new_times_out_and_a_timeout_above_20000_ms_is_capped() {
    // Given a project whose log holds nothing
    let daemon = ADaemon::holding_the_sessions_of_two_projects_and_two_users();

    // When the home agent waits briefly with no cursor
    let timed_out = bounded(
        "the wait",
        daemon
            .log
            .wait(&the_home_agents_scope(), "", Duration::from_millis(100)),
    )
    .await
    .expect("the wait is answered");

    // Then it returns empty, saying it timed out, with a cursor that skips nothing published since
    assert_eq!(
        (timed_out.page.items.len(), timed_out.timed_out),
        (0, true),
        "what the wait returned"
    );
    assert_eq!(
        timed_out.page.next_cursor, "0",
        "the cursor of a log that never held a record"
    );
    // And a timeout above 20,000 ms is held no longer than 20,000 ms, one below it is kept
    assert_eq!(
        (
            MAX_NOTIFICATION_WAIT,
            clamp_wait(Duration::from_millis(60_000)),
            clamp_wait(Duration::from_millis(100)),
        ),
        (
            Duration::from_millis(20_000),
            Duration::from_millis(20_000),
            Duration::from_millis(100),
        ),
        "the cap, a timeout above it, and a timeout below it"
    );
}

#[tokio::test]
async fn waiting_from_a_returned_cursor_never_repeats_or_skips_across_a_restart() {
    // Given three records, and the cursor of the first two
    let mut daemon = ADaemon::holding_the_sessions_of_two_projects_and_two_users();
    for text in ["first", "second", "third"] {
        daemon
            .publishes(an_attention_notification(ADA, CHILD, text))
            .await;
    }
    let first_two = daemon
        .log
        .read(&the_home_agents_scope(), "", Some(3))
        .expect("the log is read")
        .page
        .items[1]
        .seq
        .to_string();

    // When the daemon restarts and a fourth notification is published
    daemon.restarts();
    daemon
        .publishes(an_attention_notification(ADA, CHILD, "fourth"))
        .await;
    let woken = bounded(
        "the wait",
        daemon
            .log
            .wait(&the_home_agents_scope(), &first_two, LONG_WAIT),
    )
    .await
    .expect("the wait is answered");

    // Then waiting from the old cursor returns the third and the fourth, in order: none repeated,
    // none skipped, and the sequence continued where the first run stopped
    assert_eq!(
        (woken.texts(), woken.seqs(), woken.timed_out),
        (
            vec!["third".to_string(), "fourth".to_string()],
            vec![3, 4],
            false
        ),
        "what the wait returned after the restart"
    );
}

#[tokio::test]
async fn another_projects_another_os_users_and_the_callers_own_records_never_appear() {
    // Given one notification each from the project's child, the home session itself, a session of
    // another project, and a session of another OS user in the same project
    let daemon = ADaemon::holding_the_sessions_of_two_projects_and_two_users();
    for (os_user, session, text) in [
        (ADA, CHILD, "the child finished"),
        (ADA, HOME, "the home's own"),
        (ADA, OTHER_PROJECTS_SESSION, "another project's"),
        (GRACE, OTHER_USERS_SESSION, "another user's"),
    ] {
        daemon
            .publishes(an_attention_notification(os_user, session, text))
            .await;
    }

    // When the home agent of project A, as ada, reads its log
    let read = daemon
        .log
        .read(&the_home_agents_scope(), "", None)
        .expect("the log is read");

    // Then it sees the child's record alone
    assert_eq!(
        read.texts(),
        vec!["the child finished".to_string()],
        "the records the home agent saw"
    );
}

#[tokio::test]
async fn reading_from_an_evicted_cursor_reports_how_many_records_were_lost() {
    // Given a log that has been written five records past its capacity
    let daemon = ADaemon::holding_the_sessions_of_two_projects_and_two_users();
    let published = PROJECT_NOTIFICATION_CAPACITY + 5;
    for n in 1..=published {
        daemon
            .publishes(an_attention_notification(ADA, CHILD, &format!("turn {n}")))
            .await;
    }

    // When the home agent reads from the cursor of the second record, which is long evicted
    let read = daemon
        .log
        .read(&the_home_agents_scope(), "2", None)
        .expect("the log is read");

    // Then the read says the three records between that cursor and the oldest kept were lost, and
    // continues from the oldest kept — it is not silently trimmed
    assert_eq!(
        (read.lost, read.seqs().first().copied()),
        (3, Some(6)),
        "records lost, and where the page starts"
    );
}

#[tokio::test]
async fn an_question_record_carries_its_id_and_content() {
    // Given a child that asks for a permission, as the pending-question capture publishes it
    let daemon = ADaemon::holding_the_sessions_of_two_projects_and_two_users();
    let question = QuestionNotice {
        request_id: "req-cargo-publish".to_string(),
        kind: "permission".to_string(),
        tool_name: "Bash".to_string(),
        summary: "Run Bash: cargo publish".to_string(),
        questions: Vec::new(),
    };
    daemon
        .publishes(SessionNotification {
            source: SessionNotificationSource::PendingQuestion,
            question: Some(question.clone()),
            ..an_attention_notification(
                ADA,
                CHILD,
                "my-feature-branch is asking: Run Bash: cargo publish",
            )
        })
        .await;

    // When the home agent reads its log
    let read = daemon
        .log
        .read(&the_home_agents_scope(), "", None)
        .expect("the log is read");

    // Then the record carries the question's id and content, so the agent can answer it
    let record = read.page.items.first().expect("the question was recorded");
    assert_eq!(
        (record.source, record.question.clone()),
        (SessionNotificationSource::PendingQuestion, Some(question)),
        "the recorded source and question"
    );
}
