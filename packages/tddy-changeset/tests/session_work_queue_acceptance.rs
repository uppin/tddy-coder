//! A session owns a work queue (`#workqueue` 5/14): stored under its owning session, created at start
//! (asked for, or any orchestrator start), and the pre-queue project stack records dropped on upgrade.

use pretty_assertions::assert_eq;
use tddy_changeset::changeset::WorkItem;
use tddy_changeset::session_work_queue::{
    drop_old_project_stack_records, starts_with_work_queue, SessionWorkQueueError,
    SessionWorkQueues, NO_WORK_QUEUE_HINT, WORK_QUEUE_FILE,
};

const T0: u64 = 1_795_000_000_000;

/// A host's data directory with one session per `(session_id, project_id)`.
struct AHost {
    root: tempfile::TempDir,
}

impl AHost {
    fn with_sessions(sessions: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().unwrap();
        for (session_id, project_id) in sessions {
            let dir = root.path().join("sessions").join(session_id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(".session.yaml"),
                format!("session_id: {session_id}\nproject_id: {project_id}\n"),
            )
            .unwrap();
        }
        Self { root }
    }

    fn queues(&self) -> SessionWorkQueues {
        SessionWorkQueues::new(self.root.path().join("sessions"))
    }
}

fn an_item(id: &str) -> WorkItem {
    WorkItem {
        node_id: id.to_string(),
        title: format!("item {id}"),
        ..WorkItem::default()
    }
}

#[test]
fn a_queue_is_stored_under_its_owning_session() {
    // Given
    let host = AHost::with_sessions(&[("sess-a", "proj-1")]);
    let queues = host.queues();

    // When
    queues
        .create_for_session("sess-a", "proj-1", T0)
        .expect("created");
    queues
        .update("sess-a", |queue| queue.nodes.push(an_item("n1")))
        .expect("updated");

    // Then
    assert!(host
        .root
        .path()
        .join("sessions/sess-a")
        .join(WORK_QUEUE_FILE)
        .is_file());
    let record = queues.read("sess-a").expect("read").expect("a queue");
    assert_eq!(
        (record.owner_session_id.as_str(), record.queue.nodes.len()),
        ("sess-a", 1)
    );
}

#[test]
fn a_session_without_a_queue_reads_as_none_and_refuses_an_update_with_the_hint() {
    // Given
    let host = AHost::with_sessions(&[("sess-plain", "proj-1")]);
    let queues = host.queues();

    // When
    let read = queues.read("sess-plain").expect("read");
    let refused = queues.update("sess-plain", |queue| queue.nodes.push(an_item("n1")));

    // Then
    assert!(read.is_none());
    match refused {
        Err(error @ SessionWorkQueueError::NoQueue { .. }) => assert_eq!(
            error.to_string(),
            format!("session sess-plain has no work queue — {NO_WORK_QUEUE_HINT}")
        ),
        other => panic!("an update without a queue is refused: {other:?}"),
    }
}

#[test]
fn a_session_creates_its_queue_once() {
    // Given
    let host = AHost::with_sessions(&[("sess-a", "proj-1")]);
    let queues = host.queues();
    queues
        .create_for_session("sess-a", "proj-1", T0)
        .expect("created");

    // When
    let again = queues.create_for_session("sess-a", "proj-1", T0 + 1);

    // Then
    assert!(matches!(
        again,
        Err(SessionWorkQueueError::AlreadyExists { .. })
    ));
}

#[test]
fn the_project_listing_finds_each_owning_sessions_queue() {
    // Given
    let host = AHost::with_sessions(&[
        ("sess-a", "proj-1"),
        ("sess-b", "proj-1"),
        ("sess-c", "proj-2"),
        ("sess-plain", "proj-1"),
    ]);
    let queues = host.queues();
    for (session, project) in [
        ("sess-a", "proj-1"),
        ("sess-b", "proj-1"),
        ("sess-c", "proj-2"),
    ] {
        queues
            .create_for_session(session, project, T0)
            .expect("created");
    }

    // When
    let mut owners: Vec<String> = queues
        .list_for_project("proj-1")
        .expect("listed")
        .into_iter()
        .map(|record| record.owner_session_id)
        .collect();
    owners.sort();

    // Then
    assert_eq!(owners, vec!["sess-a".to_string(), "sess-b".to_string()]);
}

#[test]
fn a_session_started_with_the_option_gets_a_queue_and_one_without_it_none() {
    assert_eq!(
        (
            starts_with_work_queue(true, None),
            starts_with_work_queue(false, None),
            starts_with_work_queue(false, Some("")),
        ),
        (true, false, false)
    );
}

#[test]
fn an_orchestrator_start_gets_a_queue_without_the_option() {
    assert_eq!(
        (
            starts_with_work_queue(false, Some("project-home")),
            starts_with_work_queue(false, Some("orchestrator")),
        ),
        (true, true)
    );
}

#[test]
fn old_project_stack_records_and_journals_are_dropped() {
    // Given — two projects' pre-queue stack records, one with a merge journal, beside other project data
    let root = tempfile::tempdir().unwrap();
    let projects = root.path().join("projects");
    for project in ["proj-1", "proj-2"] {
        let stacks = projects.join(project).join("stacks");
        std::fs::create_dir_all(&stacks).unwrap();
        std::fs::write(stacks.join("0190.yaml"), "stack_id: '0190'\n").unwrap();
        std::fs::write(projects.join(project).join("session-watches.json"), "{}").unwrap();
    }
    std::fs::write(projects.join("proj-1/stacks/0190.op.json"), "{}").unwrap();

    // When
    let dropped = drop_old_project_stack_records(&projects).expect("dropped");
    let again = drop_old_project_stack_records(&projects).expect("idempotent");

    // Then
    assert_eq!((dropped, again), (2, 0));
    assert!(!projects.join("proj-1/stacks").exists());
    assert!(
        projects.join("proj-1/session-watches.json").exists(),
        "other project data stays"
    );
}

#[test]
fn a_queues_operation_journal_sits_beside_it_and_survives_the_upgrade_drop() {
    // Given a session that owns a queue
    let host = AHost::with_sessions(&[("orch", "proj-a")]);
    let queues = host.queues();
    queues.create_for_session("orch", "proj-a", T0).unwrap();

    // When its operation journal is placed, and the old project stack records are dropped
    let journal = queues.op_journal_path("orch").unwrap();
    drop_old_project_stack_records(&host.root.path().join("projects")).unwrap();

    // Then the journal is in the owning session's directory, beside the queue, where the drop never reaches
    assert_eq!(
        journal,
        host.root
            .path()
            .join("sessions")
            .join("orch")
            .join(tddy_changeset::session_work_queue::WORK_QUEUE_OP_JOURNAL_FILE)
    );
    assert!(queues.op_journal_path("../escape").is_err());
}
