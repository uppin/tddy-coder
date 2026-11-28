//! Unit: a work item's file list — the pure model ops behind `#workqueue` work item files.
//!
//! PRD: `docs/ft/coder/work-item-files.md`
//! Changeset: `docs/dev/1-WIP/2026-11-28-work-item-files.md`
//!
//! The list is queue data (`WorkItem.files`), so recording an attachment is a pure model op beside
//! `item_annotations` — the bytes live in the file store (`tddy-changeset`), and the RPC layer runs
//! that first and records here after. Limits are refusals naming the limit, never truncation.

use tddy_core::changeset::{WorkItem, WorkItemFile, WorkQueue};
use tddy_work_queue::item_ops::item_files::{
    files_of, record_attached_file, record_detached_file, WorkItemFileError, MAX_FILES_PER_ITEM,
};

const ITEM: &str = "n1";

fn a_file(basename: &str, size_bytes: u64) -> WorkItemFile {
    WorkItemFile {
        basename: basename.to_string(),
        size_bytes,
    }
}

fn a_queue_with_one_item() -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: vec![WorkItem {
            node_id: ITEM.to_string(),
            title: "Token store".to_string(),
            ..WorkItem::default()
        }],
        ..WorkQueue::default()
    }
}

fn refusals_of(result: Result<(), WorkItemFileError>) -> WorkItemFileError {
    result.expect_err("the call must be refused")
}

#[test]
fn recording_an_attached_file_lists_it_with_its_size() {
    // Given
    let mut queue = a_queue_with_one_item();

    // When
    record_attached_file(&mut queue, ITEM, a_file("failure.log", 812)).expect("recorded");

    // Then
    assert_eq!(
        files_of(&queue, ITEM),
        vec![a_file("failure.log", 812)],
        "the recorded file is the item's whole list"
    );
}

#[test]
fn recorded_files_keep_the_order_they_were_attached_in() {
    // Given
    let mut queue = a_queue_with_one_item();

    // When
    record_attached_file(&mut queue, ITEM, a_file("failure.log", 812)).expect("first");
    record_attached_file(&mut queue, ITEM, a_file("spec.md", 2048)).expect("second");

    // Then
    let basenames: Vec<String> = files_of(&queue, ITEM)
        .iter()
        .map(|file| file.basename.clone())
        .collect();
    assert_eq!(
        basenames,
        vec!["failure.log".to_string(), "spec.md".to_string()]
    );
}

#[test]
fn a_second_file_with_the_same_basename_is_refused_and_the_list_is_untouched() {
    // Given
    let mut queue = a_queue_with_one_item();
    record_attached_file(&mut queue, ITEM, a_file("spec.md", 2048)).expect("first");

    // When
    let refusal = refusals_of(record_attached_file(&mut queue, ITEM, a_file("spec.md", 1)));

    // Then
    assert_eq!(
        refusal,
        WorkItemFileError::Duplicate {
            basename: "spec.md".to_string()
        },
        "a duplicate basename is refused, never silently renamed"
    );
    assert_eq!(
        files_of(&queue, ITEM),
        vec![a_file("spec.md", 2048)],
        "the refused attach changed nothing"
    );
}

#[test]
fn the_file_after_the_cap_is_refused_naming_the_cap() {
    // Given — the cap is exactly full
    let mut queue = a_queue_with_one_item();
    for i in 0..MAX_FILES_PER_ITEM {
        record_attached_file(&mut queue, ITEM, a_file(&format!("file-{i}.md"), i as u64))
            .expect("within the cap");
    }

    // When
    let refusal = refusals_of(record_attached_file(
        &mut queue,
        ITEM,
        a_file("one-too-many.md", 1),
    ));

    // Then
    assert_eq!(
        refusal,
        WorkItemFileError::TooMany {
            limit: MAX_FILES_PER_ITEM
        },
        "the refusal names the limit rather than truncating the list"
    );
    assert_eq!(files_of(&queue, ITEM).len(), MAX_FILES_PER_ITEM);
}

#[test]
fn detaching_a_file_removes_its_entry_and_leaves_the_rest() {
    // Given
    let mut queue = a_queue_with_one_item();
    record_attached_file(&mut queue, ITEM, a_file("failure.log", 812)).expect("first");
    record_attached_file(&mut queue, ITEM, a_file("spec.md", 2048)).expect("second");

    // When
    record_detached_file(&mut queue, ITEM, "failure.log").expect("detached");

    // Then
    assert_eq!(files_of(&queue, ITEM), vec![a_file("spec.md", 2048)]);
}

#[test]
fn detaching_a_file_the_item_does_not_carry_is_refused() {
    // Given
    let mut queue = a_queue_with_one_item();

    // When
    let refusal = refusals_of(record_detached_file(&mut queue, ITEM, "absent.md"));

    // Then
    assert_eq!(
        refusal,
        WorkItemFileError::NoSuchFile {
            basename: "absent.md".to_string()
        }
    );
}

#[test]
fn recording_a_file_on_an_unknown_item_is_refused() {
    // Given
    let mut queue = a_queue_with_one_item();

    // When
    let refusal = refusals_of(record_attached_file(&mut queue, "n9", a_file("spec.md", 1)));

    // Then
    assert_eq!(refusal, WorkItemFileError::NoSuchItem("n9".to_string()));
}

#[test]
fn the_files_of_an_item_without_any_is_an_empty_list() {
    // Given
    let queue = a_queue_with_one_item();

    // When
    let files = files_of(&queue, ITEM);

    // Then
    assert!(files.is_empty(), "got {files:?}");
}

#[test]
fn the_files_of_an_unknown_item_are_an_empty_list() {
    // Given
    let queue = a_queue_with_one_item();

    // When
    let files = files_of(&queue, "n9");

    // Then — a read of another item's list is an answer, not a leak
    assert!(files.is_empty(), "got {files:?}");
}
