//! The project notification log's file, as a restart sees it: it stays bounded once it outgrows its
//! window, sequence numbers continue across a restart after compaction, and a damaged line is
//! skipped rather than ending the log (`#home-inbox` 7/8).
//!
//! Changeset: docs/dev/changesets/2026-10-29-home-inbox-home-wait.md

mod project_notification_log_support;

use std::path::Path;

use pretty_assertions::assert_eq;
use project_notification_log_support::{a_log_over, write_session};
use tddy_daemon_kernel::session_notification::SessionEndNotice;
use tddy_session_activity::project_notification_log::{
    ProjectNotificationScope, PROJECT_NOTIFICATION_CAPACITY,
};
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};

const PROJECT: &str = "project-a";
const ADA: &str = "ada";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000500";
const HOME: &str = "01900000-0000-7000-8000-AABB00000501";

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
        ended: None,
    }
}

fn the_home_agents_scope() -> ProjectNotificationScope {
    ProjectNotificationScope {
        project_id: PROJECT.to_string(),
        os_user: ADA.to_string(),
        exclude_session_id: HOME.to_string(),
        started_by: None,
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
    write_session(data_dir.path(), ADA, CHILD, PROJECT);
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
    write_session(data_dir.path(), ADA, CHILD, PROJECT);
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

#[tokio::test]
async fn a_line_torn_by_a_crash_does_not_swallow_the_next_record() {
    // Given one record, then the start of a second that a crash cut off before its line break
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    write_session(data_dir.path(), ADA, CHILD, PROJECT);
    let (_log, bus) = a_log_over(data_dir.path());
    bus.publish(an_attention_notification("before the crash"))
        .await;
    let file = data_dir
        .path()
        .join("projects")
        .join(PROJECT)
        .join("notifications.jsonl");
    let mut torn = std::fs::read_to_string(&file).expect("the file");
    torn.push_str("{\"os_user\":\"ada\",\"seq\":2,\"sess");
    std::fs::write(&file, torn).expect("the torn line");

    // When the daemon restarts and a record is published
    let (restarted, restarted_bus) = a_log_over(data_dir.path());
    restarted_bus
        .publish(an_attention_notification("after the crash"))
        .await;

    // Then the new record survives another restart: it did not run on into the torn line
    let (reopened, _) = a_log_over(data_dir.path());
    for log in [&restarted, &reopened] {
        let texts: Vec<String> = log
            .read(&the_home_agents_scope(), "", None)
            .expect("the log is read")
            .page
            .items
            .iter()
            .map(|r| r.text.clone())
            .collect();
        assert_eq!(
            texts,
            vec![
                "before the crash".to_string(),
                "after the crash".to_string()
            ]
        );
    }
}

/// The notification the daemon publishes when the child's agent exits with `code`.
fn the_childs_agent_exited_with(code: i32) -> SessionNotification {
    SessionNotification {
        source: SessionNotificationSource::SessionEnded,
        ended: Some(SessionEndNotice {
            outcome: format!("exit code {code}"),
            exit_code: Some(code),
        }),
        ..an_attention_notification(&format!(
            "⏹ Session my-feature-branch ended: exit code {code}"
        ))
    }
}

/// The JSON lines of the project's notification file.
fn json_lines_in(data_dir: &Path) -> Vec<serde_json::Value> {
    let file = data_dir
        .join("projects")
        .join(PROJECT)
        .join("notifications.jsonl");
    std::fs::read_to_string(file)
        .expect("the project's notification file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect()
}

#[tokio::test]
async fn an_ended_record_round_trips_through_the_log_file() {
    // Given a daemon whose child's agent exited with code 3
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    write_session(data_dir.path(), ADA, CHILD, PROJECT);
    let (_log, bus) = a_log_over(data_dir.path());
    bus.publish(the_childs_agent_exited_with(3)).await;

    // When the daemon restarts over the same data directory
    let (restarted, _bus) = a_log_over(data_dir.path());

    // Then the record read back is a session end, with how it ended
    let read = restarted
        .read(&the_home_agents_scope(), "", None)
        .expect("the log is read");
    assert_eq!(
        read.page
            .items
            .iter()
            .map(|r| (r.source, r.ended.clone()))
            .collect::<Vec<_>>(),
        vec![(
            SessionNotificationSource::SessionEnded,
            Some(SessionEndNotice {
                outcome: "exit code 3".to_string(),
                exit_code: Some(3),
            }),
        )]
    );
}

#[tokio::test]
async fn a_record_without_an_ended_part_writes_no_ended_key() {
    // Given a daemon whose child asked for attention, which is not an end
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    write_session(data_dir.path(), ADA, CHILD, PROJECT);
    let (_log, bus) = a_log_over(data_dir.path());

    // When the notification is logged
    bus.publish(an_attention_notification("needs your input"))
        .await;

    // Then its line carries no `ended` key at all
    assert_eq!(
        json_lines_in(data_dir.path())
            .iter()
            .map(|line| line.get("ended").is_some())
            .collect::<Vec<_>>(),
        vec![false]
    );
}
