//! The project notification log's own rules, one at a time: the wait cap, which notifications it
//! wants, cursor parsing, the cursor of an empty page, paging and the `lost` count
//! (`#home-inbox` 7/8). The end-to-end behaviour is `project_notification_log_acceptance.rs`'s.
//!
//! Changeset: docs/dev/1-WIP/2026-10-25-home-inbox-home-wait.md

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_session_activity::project_notification_log::{
    clamp_wait, ProjectNotificationLog, ProjectNotificationScope, MAX_NOTIFICATION_WAIT,
    PROJECT_NOTIFICATION_CAPACITY,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind,
    SessionNotificationSource, SessionNotificationSubscriber,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const GRACE: &str = "grace";
const HOME: &str = "01900000-0000-7000-8000-AABB00000400";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000401";
const GRACES_CHILD: &str = "01900000-0000-7000-8000-AABB00000402";
const BOUND: Duration = Duration::from_secs(10);

// ── the world ────────────────────────────────────────────────────────────────────────────────

struct ALog {
    _data_dir: tempfile::TempDir,
    log: Arc<ProjectNotificationLog>,
    bus: SessionNotificationBus,
}

fn a_log_holding_two_users_children() -> ALog {
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    for (user, session) in [(ADA, HOME), (ADA, CHILD), (GRACE, GRACES_CHILD)] {
        write_session(data_dir.path(), user, session);
    }
    let root = data_dir.path().to_path_buf();
    let log = Arc::new(
        ProjectNotificationLog::open(
            data_dir.path(),
            Arc::new(move |user: &str| Some(root.join("users").join(user))),
        )
        .expect("the log opens"),
    );
    let bus = SessionNotificationBus::new().with_subscriber(Arc::clone(&log));
    ALog {
        _data_dir: data_dir,
        log,
        bus,
    }
}

fn write_session(data_dir: &Path, os_user: &str, session_id: &str) {
    let dir = data_dir
        .join("users")
        .join(os_user)
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&dir).expect("the session dir");
    let meta = SessionMetadata {
        session_id: session_id.to_string(),
        project_id: PROJECT.to_string(),
        created_at: "2026-10-25T10:00:00Z".to_string(),
        updated_at: "2026-10-25T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/{os_user}/{PROJECT}")),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: Some("Running".to_string()),
        hook_token: None,
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
        ssh_config_host: None,
        managed_codebase: false,
        session_role: None,
    };
    write_session_metadata(&dir, &meta).expect("the session's .session.yaml");
}

fn a_notification(
    kind: SessionNotificationKind,
    os_user: &str,
    session: &str,
    text: &str,
) -> SessionNotification {
    SessionNotification {
        session_id: session.to_string(),
        os_user: os_user.to_string(),
        label: "my-feature-branch".to_string(),
        kind,
        source: SessionNotificationSource::ActivityStatus,
        text: text.to_string(),
        agent: None,
        question: None,
        at_unix_ms: 1_790_000_000_000,
    }
}

fn attention_from_the_child(text: &str) -> SessionNotification {
    a_notification(SessionNotificationKind::AttentionRequired, ADA, CHILD, text)
}

fn ada_home_scope() -> ProjectNotificationScope {
    ProjectNotificationScope {
        project_id: PROJECT.to_string(),
        os_user: ADA.to_string(),
        exclude_session_id: HOME.to_string(),
    }
}

impl ALog {
    async fn holds_records_from_the_child(&self, count: usize) {
        for n in 1..=count {
            self.bus
                .publish(attention_from_the_child(&format!("turn {n}")))
                .await;
        }
    }
}

// ── the wait cap ─────────────────────────────────────────────────────────────────────────────

#[rstest]
#[case::nothing(0, 0)]
#[case::shorter_than_the_cap(100, 100)]
#[case::exactly_the_cap(20_000, 20_000)]
#[case::just_above_the_cap(20_001, 20_000)]
#[case::absurd(u64::MAX, 20_000)]
fn a_wait_is_held_no_longer_than_20_seconds(#[case] asked_ms: u64, #[case] held_ms: u64) {
    assert_eq!(MAX_NOTIFICATION_WAIT, Duration::from_millis(20_000));
    assert_eq!(
        clamp_wait(Duration::from_millis(asked_ms)),
        Duration::from_millis(held_ms)
    );
}

// ── what the log wants ───────────────────────────────────────────────────────────────────────

#[test]
fn the_log_wants_attention_notifications_and_declines_activity() {
    // Given a log
    let log = a_log_holding_two_users_children().log;

    // Then it wants what asks for the operator, and declines what is merely progress
    assert_eq!(
        (
            log.wants(&attention_from_the_child("needs you")),
            log.wants(&a_notification(
                SessionNotificationKind::Activity,
                ADA,
                CHILD,
                "running"
            )),
        ),
        (true, false)
    );
}

// ── cursors and the cursor of an empty page ──────────────────────────────────────────────────

#[rstest]
#[case::words("abc")]
#[case::negative("-1")]
#[case::fraction("1.5")]
fn a_cursor_that_is_not_a_decimal_seq_is_refused_naming_it(#[case] cursor: &str) {
    // Given a log
    let log = a_log_holding_two_users_children().log;

    // When it is read from a cursor no page ever returned
    let refusal = log
        .read(&ada_home_scope(), cursor, None)
        .expect_err("the cursor is refused");

    // Then the refusal names the cursor
    assert!(refusal.contains(cursor), "{refusal}");
}

#[tokio::test]
async fn a_wait_from_a_cursor_that_is_not_a_decimal_seq_is_refused_naming_it() {
    // Given a log
    let log = a_log_holding_two_users_children().log;

    // When it is waited on from a cursor no page ever returned
    let refusal = log
        .wait(&ada_home_scope(), "abc", Duration::from_millis(50))
        .await
        .expect_err("the cursor is refused");

    // Then the refusal names the cursor
    assert!(refusal.contains("abc"), "{refusal}");
}

#[test]
fn a_log_that_never_held_a_record_reads_an_empty_page_at_cursor_zero() {
    // Given a log that never held a record
    let log = a_log_holding_two_users_children().log;

    // When it is read
    let read = log.read(&ada_home_scope(), "", None).expect("a read");

    // Then the page is empty and its cursor is 0, not the empty cursor it was asked from
    assert_eq!(
        (
            read.page.items.len(),
            read.page.next_cursor.as_str(),
            read.page.has_more,
            read.lost
        ),
        (0, "0", false, 0)
    );
}

#[tokio::test]
async fn an_empty_page_after_the_newest_record_keeps_that_records_seq_as_its_cursor() {
    // Given a log with three records
    let world = a_log_holding_two_users_children();
    world.holds_records_from_the_child(3).await;

    // When it is read from the newest record's cursor
    let read = world
        .log
        .read(&ada_home_scope(), "3", None)
        .expect("a read");

    // Then nothing is newer, and the cursor stays at 3
    assert_eq!(
        (read.page.items.len(), read.page.next_cursor.as_str()),
        (0, "3")
    );
}

#[tokio::test]
async fn records_filtered_out_by_scope_still_advance_the_cursor_of_an_empty_page() {
    // Given a log whose only record is another OS user's
    let world = a_log_holding_two_users_children();
    world
        .bus
        .publish(a_notification(
            SessionNotificationKind::AttentionRequired,
            GRACE,
            GRACES_CHILD,
            "another user's",
        ))
        .await;

    // When ada's home agent reads it
    let read = world.log.read(&ada_home_scope(), "", None).expect("a read");

    // Then it sees nothing, and its cursor is past the record it was not shown
    assert_eq!(
        (read.page.items.len(), read.page.next_cursor.as_str()),
        (0, "1")
    );
}

// ── paging and lost ──────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn records_are_numbered_from_one_in_the_order_they_were_published() {
    // Given three records published in order
    let world = a_log_holding_two_users_children();
    world.holds_records_from_the_child(3).await;

    // When the log is read
    let read = world.log.read(&ada_home_scope(), "", None).expect("a read");

    // Then they are 1, 2, 3, oldest first
    assert_eq!(
        read.page
            .items
            .iter()
            .map(|r| (r.seq, r.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "turn 1"), (2, "turn 2"), (3, "turn 3")]
    );
}

#[tokio::test]
async fn a_read_without_a_cursor_returns_the_newest_default_page_oldest_first() {
    // Given 120 records
    let world = a_log_holding_two_users_children();
    world.holds_records_from_the_child(120).await;

    // When the log is read without a cursor or limit
    let read = world.log.read(&ada_home_scope(), "", None).expect("a read");

    // Then it is the newest 50, oldest first, ending at the log's end
    let seqs: Vec<u64> = read.page.items.iter().map(|r| r.seq).collect();
    assert_eq!(
        (seqs.first().copied(), seqs.last().copied(), seqs.len()),
        (Some(71), Some(120), 50)
    );
    assert_eq!(read.page.next_cursor, "120");
}

#[tokio::test]
async fn a_limit_above_the_follow_maximum_is_clamped_and_says_so() {
    // Given 250 records
    let world = a_log_holding_two_users_children();
    world.holds_records_from_the_child(250).await;

    // When the log is read from the start with a limit of 900
    let read = world
        .log
        .read(&ada_home_scope(), "0", Some(900))
        .expect("a read");

    // Then 200 come back, clamped_to says 200, and there is more
    assert_eq!(
        (
            read.page.items.len(),
            read.page.clamped_to,
            read.page.has_more
        ),
        (200, Some(200), true)
    );
}

#[rstest]
#[case::from_the_start(0, 3, 4)]
#[case::one_record_in(1, 2, 4)]
#[case::at_the_last_evicted(3, 0, 4)]
#[case::inside_the_window(500, 0, 501)]
#[tokio::test]
async fn lost_counts_the_records_evicted_after_the_cursor(
    #[case] cursor: u64,
    #[case] lost: u64,
    #[case] first_seq: u64,
) {
    // Given a log written three records past its capacity, so records 1 to 3 are evicted
    let world = a_log_holding_two_users_children();
    world
        .holds_records_from_the_child(PROJECT_NOTIFICATION_CAPACITY + 3)
        .await;

    // When it is read from `cursor`
    let read = world
        .log
        .read(&ada_home_scope(), &cursor.to_string(), None)
        .expect("a read");

    // Then it says how many were lost and continues from the oldest it kept
    assert_eq!(
        (read.lost, read.page.items.first().map(|r| r.seq)),
        (lost, Some(first_seq))
    );
}

// ── waiting ──────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_wait_returns_at_once_when_records_newer_than_its_cursor_exist() {
    // Given two records
    let world = a_log_holding_two_users_children();
    world.holds_records_from_the_child(2).await;

    // When the home agent waits from cursor 1 for as long as the cap allows
    let woken = tokio::time::timeout(
        BOUND,
        world
            .log
            .wait(&ada_home_scope(), "1", MAX_NOTIFICATION_WAIT),
    )
    .await
    .expect("the wait returned within its bound")
    .expect("the wait is answered");

    // Then it returns the second record without holding, not timed out
    assert_eq!(
        (
            woken.page.items.iter().map(|r| r.seq).collect::<Vec<_>>(),
            woken.timed_out
        ),
        (vec![2], false)
    );
}
