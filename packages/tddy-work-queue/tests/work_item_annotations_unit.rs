//! A work item's labels and keyed badges (`#workqueue` 7/14).

use pretty_assertions::assert_eq;
use tddy_core::changeset::{WorkItem, WorkQueue};
use tddy_work_queue::item_ops::item_annotations::{
    badges_of, clear_badge, labels_of, set_badge, set_labels, AnnotationError, BadgeTone,
    WorkItemBadge, MAX_LABELS_PER_ITEM, MAX_LABEL_LEN,
};

fn a_queue_with_item(id: &str) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: vec![WorkItem {
            node_id: id.to_string(),
            ..WorkItem::default()
        }],
    }
}

fn a_badge(key: &str, text: &str, tone: BadgeTone) -> WorkItemBadge {
    WorkItemBadge {
        key: key.to_string(),
        text: text.to_string(),
        tone,
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| v.to_string()).collect()
}

#[test]
fn setting_labels_replaces_the_previous_set() {
    // Given
    let mut queue = a_queue_with_item("n1");
    set_labels(&mut queue, "n1", strings(&["api", "blocked-on-design"])).expect("set");

    // When
    set_labels(&mut queue, "n1", strings(&["api", "needs-review", "api"])).expect("set again");

    // Then
    assert_eq!(labels_of(&queue, "n1"), strings(&["api", "needs-review"]));
}

#[test]
fn setting_a_badge_with_an_existing_key_replaces_it() {
    // Given
    let mut queue = a_queue_with_item("n1");
    set_badge(&mut queue, "n1", a_badge("qa", "running", BadgeTone::Info)).expect("set");
    set_badge(
        &mut queue,
        "n1",
        a_badge("deploy", "waiting", BadgeTone::Warning),
    )
    .expect("set");

    // When
    set_badge(
        &mut queue,
        "n1",
        a_badge("qa", "passed", BadgeTone::Success),
    )
    .expect("replaced");

    // Then
    assert_eq!(
        badges_of(&queue, "n1"),
        vec![
            a_badge("qa", "passed", BadgeTone::Success),
            a_badge("deploy", "waiting", BadgeTone::Warning),
        ]
    );
}

#[test]
fn clearing_a_badge_removes_only_that_key() {
    // Given
    let mut queue = a_queue_with_item("n1");
    set_badge(
        &mut queue,
        "n1",
        a_badge("qa", "passed", BadgeTone::Success),
    )
    .expect("set");
    set_badge(
        &mut queue,
        "n1",
        a_badge("deploy", "waiting", BadgeTone::Warning),
    )
    .expect("set");

    // When
    clear_badge(&mut queue, "n1", "qa").expect("cleared");

    // Then
    assert_eq!(
        badges_of(&queue, "n1"),
        vec![a_badge("deploy", "waiting", BadgeTone::Warning)]
    );
}

#[test]
fn an_over_limit_label_set_is_refused_naming_the_limit() {
    // Given
    let mut queue = a_queue_with_item("n1");
    let too_many: Vec<String> = (0..=MAX_LABELS_PER_ITEM).map(|i| format!("l{i}")).collect();
    let too_long = vec!["x".repeat(MAX_LABEL_LEN + 1)];

    // When
    let many = set_labels(&mut queue, "n1", too_many);
    let long = set_labels(&mut queue, "n1", too_long);

    // Then
    assert_eq!(
        (many, long, labels_of(&queue, "n1")),
        (
            Err(AnnotationError::TooMany {
                limit: MAX_LABELS_PER_ITEM
            }),
            Err(AnnotationError::TooLong {
                limit: MAX_LABEL_LEN
            }),
            Vec::<String>::new()
        )
    );
}

#[test]
fn annotating_an_unknown_item_is_refused() {
    // Given
    let mut queue = a_queue_with_item("n1");

    // When
    let refused = set_labels(&mut queue, "n9", strings(&["api"]));

    // Then
    assert_eq!(refused, Err(AnnotationError::NoSuchItem("n9".to_string())));
}
