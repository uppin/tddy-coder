//! Acceptance: the durable file store a work item's attachments live in.
//!
//! PRD: `docs/ft/coder/work-item-files.md`
//! Changeset: `docs/dev/1-WIP/2026-11-28-work-item-files.md`
//!
//! The bytes live on the queue's owner host under the owning session's directory —
//! `<sessions>/<session_id>/work-queue-files/<item_id>/<basename>` — durable where the staged
//! root is restart-cleared. The store enforces the path rules (every segment pure) and writes
//! exclusively; the RPC layer owns the size and count caps.

use pretty_assertions::assert_eq;
use tddy_changeset::changeset::{WorkItem, WorkQueue};
use tddy_changeset::session_work_queue::SessionWorkQueues;
use tddy_changeset::session_work_queue_files::{SessionWorkQueueFiles, WorkQueueFileError};

const OWNER: &str = "sess-owner";
const ITEM: &str = "n1";

/// A host's data directory, removed with the world.
struct AHost {
    root: tempfile::TempDir,
    files: SessionWorkQueueFiles,
}

fn a_host() -> AHost {
    let root = tempfile::tempdir().expect("temp data dir");
    let files = SessionWorkQueueFiles::new(root.path().join("sessions"));
    AHost { root, files }
}

impl AHost {
    /// `owner` is started with a queue holding one item.
    fn starts_with_a_queue(&self) {
        SessionWorkQueues::new(self.root.path().join("sessions"))
            .create_for_session(OWNER, "proj-auth", 7)
            .expect("created");
        SessionWorkQueues::new(self.root.path().join("sessions"))
            .update(OWNER, |queue| {
                queue.nodes = vec![WorkItem {
                    node_id: ITEM.to_string(),
                    title: "Token store".to_string(),
                    ..WorkItem::default()
                }];
            })
            .expect("item added");
    }

    fn session_dir(&self) -> std::path::PathBuf {
        self.root.path().join("sessions").join(OWNER)
    }

    fn refusal_of(&self, result: Result<u64, WorkQueueFileError>) -> WorkQueueFileError {
        result.expect_err("the call must be refused")
    }
}

#[test]
fn storing_a_file_writes_the_bytes_under_the_items_own_directory_and_answers_their_size() {
    // Given
    let host = a_host();

    // When
    let size = host
        .files
        .store(OWNER, ITEM, "failure.log", b"# failure log\n")
        .expect("stored");

    // Then
    assert_eq!(size, 14, "the size answered is the size written");
    let stored = std::fs::read(
        host.session_dir()
            .join("work-queue-files")
            .join(ITEM)
            .join("failure.log"),
    )
    .expect("the bytes are on disk");
    assert_eq!(stored, b"# failure log\n");
}

#[test]
fn storing_the_same_basename_again_is_refused_and_the_first_file_is_untouched() {
    // Given
    let host = a_host();
    host.files
        .store(OWNER, ITEM, "spec.md", b"first")
        .expect("stored");

    // When
    let refusal = host.refusal_of(host.files.store(OWNER, ITEM, "spec.md", b"second"));

    // Then
    assert!(
        matches!(refusal, WorkQueueFileError::AlreadyExists { .. }),
        "got {refusal:?} — a duplicate is refused, never truncated or overwritten"
    );
    let stored = std::fs::read(
        host.session_dir()
            .join("work-queue-files")
            .join(ITEM)
            .join("spec.md"),
    )
    .expect("the first file is still there");
    assert_eq!(stored, b"first");
}

#[test]
fn a_basename_that_is_not_one_segment_is_refused_and_writes_nothing() {
    // Given
    let host = a_host();

    // When
    let refusal = host.refusal_of(host.files.store(OWNER, ITEM, "nested/spec.md", b"x"));

    // Then
    assert!(
        matches!(refusal, WorkQueueFileError::InvalidSegment { .. }),
        "got {refusal:?}"
    );
    assert!(
        !host
            .session_dir()
            .join("work-queue-files")
            .join(ITEM)
            .join("nested")
            .exists(),
        "no directory was created for the refused name"
    );
}

#[test]
fn an_item_id_that_is_not_one_segment_is_refused() {
    // Given
    let host = a_host();

    // When
    let refusal = host.refusal_of(host.files.store(OWNER, "../n1", "spec.md", b"x"));

    // Then
    assert!(
        matches!(refusal, WorkQueueFileError::InvalidSegment { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn a_session_id_that_is_not_one_segment_is_refused() {
    // Given
    let host = a_host();

    // When
    let refusal = host.refusal_of(host.files.store("a/b", ITEM, "spec.md", b"x"));

    // Then
    assert!(
        matches!(refusal, WorkQueueFileError::InvalidSegment { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn a_stored_file_reads_back_byte_for_byte() {
    // Given
    let host = a_host();
    host.files
        .store(OWNER, ITEM, "spec.md", b"# the spec\n")
        .expect("stored");

    // When
    let read = host.files.read(OWNER, ITEM, "spec.md").expect("read");

    // Then
    assert_eq!(read, b"# the spec\n");
}

#[test]
fn reading_a_file_that_was_never_stored_is_not_found() {
    // Given
    let host = a_host();

    // When
    let refusal = match host.files.read(OWNER, ITEM, "absent.md") {
        Err(e) => e,
        Ok(bytes) => panic!("expected a refusal, got {} bytes", bytes.len()),
    };

    // Then
    assert!(
        matches!(refusal, WorkQueueFileError::NotFound { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn removing_a_stored_file_removes_its_bytes_but_leaves_the_items_directory() {
    // Given
    let host = a_host();
    host.files
        .store(OWNER, ITEM, "spec.md", b"# the spec\n")
        .expect("stored");

    // When
    host.files.remove(OWNER, ITEM, "spec.md").expect("removed");

    // Then
    assert!(
        !host
            .session_dir()
            .join("work-queue-files")
            .join(ITEM)
            .join("spec.md")
            .exists(),
        "the bytes are gone"
    );
    assert!(
        host.session_dir()
            .join("work-queue-files")
            .join(ITEM)
            .exists(),
        "the item's directory stays — more files may follow"
    );
}

#[test]
fn removing_a_file_that_was_never_stored_is_not_found() {
    // Given
    let host = a_host();

    // When
    let refusal = match host.files.remove(OWNER, ITEM, "absent.md") {
        Err(e) => e,
        Ok(()) => panic!("expected a refusal, got success"),
    };

    // Then
    assert!(
        matches!(refusal, WorkQueueFileError::NotFound { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn removing_an_item_removes_its_files_and_leaves_another_items() {
    // Given — two items hold files
    let host = a_host();
    host.files.store(OWNER, "n1", "spec.md", b"n1").expect("n1");
    host.files.store(OWNER, "n2", "spec.md", b"n2").expect("n2");

    // When
    host.files.remove_item(OWNER, "n1").expect("removed");

    // Then
    assert!(
        !host
            .session_dir()
            .join("work-queue-files")
            .join("n1")
            .exists(),
        "n1's files are gone with the item"
    );
    let n2 = std::fs::read(
        host.session_dir()
            .join("work-queue-files")
            .join("n2")
            .join("spec.md"),
    )
    .expect("n2 keeps its file");
    assert_eq!(n2, b"n2");
}

#[test]
fn removing_an_item_that_stored_nothing_succeeds() {
    // Given
    let host = a_host();

    // When / Then — cleanup of a file-less item is not an error
    host.files.remove_item(OWNER, "n3").expect("removed");
}

#[test]
fn an_item_with_files_round_trips_through_the_queue_record() {
    // Given
    let host = a_host();
    host.starts_with_a_queue();
    let sessions = SessionWorkQueues::new(host.root.path().join("sessions"));
    sessions
        .update(OWNER, |queue| {
            queue.nodes[0].files = vec![
                tddy_changeset::changeset::WorkItemFile {
                    basename: "failure.log".to_string(),
                    size_bytes: 812,
                },
                tddy_changeset::changeset::WorkItemFile {
                    basename: "spec.md".to_string(),
                    size_bytes: 2048,
                },
            ];
        })
        .expect("files recorded");

    // When
    let read = sessions.read(OWNER).expect("read").expect("a queue");

    // Then
    let basenames: Vec<&str> = read.queue.nodes[0]
        .files
        .iter()
        .map(|file| file.basename.as_str())
        .collect();
    assert_eq!(basenames, vec!["failure.log", "spec.md"]);
    assert_eq!(read.queue.nodes[0].files[0].size_bytes, 812);
}

#[test]
fn an_item_without_files_writes_no_files_key_into_the_record() {
    // Given
    let host = a_host();
    host.starts_with_a_queue();

    // When
    let record =
        std::fs::read_to_string(host.session_dir().join("work-queue.yaml")).expect("the record");

    // Then — an old reader of the record sees nothing new
    assert!(
        !record.contains("files"),
        "an item without files must not name the field: {record}"
    );
}
