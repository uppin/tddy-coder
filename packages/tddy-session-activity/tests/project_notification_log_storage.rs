//! The project notification log's file, as a restart sees it: it stays bounded once it outgrows its
//! window, sequence numbers continue across a restart after compaction, and a damaged line is
//! skipped rather than ending the log (`#home-inbox` 7/8).
//!
//! Changeset: docs/dev/1-WIP/2026-10-25-home-inbox-home-wait.md

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_session_activity::project_notification_log::{
    ProjectNotificationLog, ProjectNotificationScope, PROJECT_NOTIFICATION_CAPACITY,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationBus, SessionNotificationKind, SessionNotificationSource,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000500";
const HOME: &str = "01900000-0000-7000-8000-AABB00000501";

fn sessions_of(data_dir: &Path) -> PathBuf {
    data_dir.join("users").join(ADA)
}

fn write_child_session(data_dir: &Path) {
    let dir = sessions_of(data_dir).join("sessions").join(CHILD);
    std::fs::create_dir_all(&dir).expect("the session dir");
    let meta = SessionMetadata {
        session_id: CHILD.to_string(),
        project_id: PROJECT.to_string(),
        created_at: "2026-10-25T10:00:00Z".to_string(),
        updated_at: "2026-10-25T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/{ADA}/{PROJECT}")),
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

/// The log over `data_dir`, as a daemon starting over it opens it.
fn a_log_over(data_dir: &Path) -> (Arc<ProjectNotificationLog>, SessionNotificationBus) {
    let root = data_dir.to_path_buf();
    let log = Arc::new(
        ProjectNotificationLog::open(
            data_dir,
            Arc::new(move |os_user: &str| Some(root.join("users").join(os_user))),
        )
        .expect("the log opens over the data directory"),
    );
    let bus = SessionNotificationBus::new().with_subscriber(Arc::clone(&log));
    (log, bus)
}

fn an_attention_notification(text: &str) -> SessionNotification {
    SessionNotification {
        session_id: CHILD.to_string(),
        os_user: ADA.to_string(),
        label: "my-feature-branch".to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::ActivityStatus,
        text: text.to_string(),
        agent: None,
        question: None,
        at_unix_ms: 1_790_000_000_000,
    }
}

fn the_home_agents_scope() -> ProjectNotificationScope {
    ProjectNotificationScope {
        project_id: PROJECT.to_string(),
        os_user: ADA.to_string(),
        exclude_session_id: HOME.to_string(),
    }
}

fn lines_in(data_dir: &Path) -> usize {
    let file = data_dir
        .join("projects")
        .join(PROJECT)
        .join("notifications.jsonl");
    std::fs::read_to_string(file)
        .expect("the project's notification file")
        .lines()
        .count()
}

#[tokio::test]
async fn a_log_that_outgrows_its_window_on_disk_keeps_its_sequence_across_a_restart() {
    // Given a daemon whose child publishes twice the window's worth of notifications, so its file
    // is rewritten to the window once it has grown past twice the window
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    write_child_session(data_dir.path());
    let (_log, bus) = a_log_over(data_dir.path());
    let published = 2 * PROJECT_NOTIFICATION_CAPACITY + 1;
    for n in 1..=published {
        bus.publish(an_attention_notification(&format!("turn {n}")))
            .await;
    }

    // When the daemon restarts over the same data directory
    let (restarted, restarted_bus) = a_log_over(data_dir.path());

    // Then the file holds the window alone, not every notification ever published
    assert_eq!(lines_in(data_dir.path()), PROJECT_NOTIFICATION_CAPACITY);

    // Then the newest record is still the last one published, and a cursor older than the window
    // is told how many records it lost: record 1001, and only it
    let newest = restarted
        .read(&the_home_agents_scope(), "", Some(1))
        .expect("the log is read");
    let lost = restarted
        .read(&the_home_agents_scope(), "1000", None)
        .expect("the log is read")
        .lost;
    assert_eq!(
        (
            newest.page.items.first().map(|r| r.seq),
            newest.page.items.first().map(|r| r.text.clone()),
            lost,
        ),
        (Some(published as u64), Some(format!("turn {published}")), 1,),
        "the newest record before the restart, and what a cursor from before the window lost"
    );

    // And the first record published after the restart continues the count
    restarted_bus
        .publish(an_attention_notification("after the restart"))
        .await;
    let next = restarted
        .read(&the_home_agents_scope(), "", Some(1))
        .expect("the log is read");
    assert_eq!(
        next.page.items.first().map(|r| r.seq),
        Some(published as u64 + 1),
    );
}

#[tokio::test]
async fn a_damaged_line_is_skipped_and_the_records_around_it_are_kept() {
    // Given one record, then a line the file's writer never wrote
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    write_child_session(data_dir.path());
    let (_log, bus) = a_log_over(data_dir.path());
    bus.publish(an_attention_notification("before the damage"))
        .await;
    let file = data_dir
        .path()
        .join("projects")
        .join(PROJECT)
        .join("notifications.jsonl");
    let mut damaged = std::fs::read_to_string(&file).expect("the file");
    damaged.push_str("{ this line was cut off\n");
    std::fs::write(&file, damaged).expect("the damage");

    // When the daemon restarts and a second record is published
    let (restarted, restarted_bus) = a_log_over(data_dir.path());
    restarted_bus
        .publish(an_attention_notification("after the damage"))
        .await;

    // Then both records are read, numbered in order, and the damaged line is not one of them
    let read = restarted
        .read(&the_home_agents_scope(), "", None)
        .expect("the log is read");
    let texts: Vec<String> = read.page.items.iter().map(|r| r.text.clone()).collect();
    assert_eq!(
        (texts, read.page.next_cursor),
        (
            vec![
                "before the damage".to_string(),
                "after the damage".to_string()
            ],
            "2".to_string()
        )
    );
}
