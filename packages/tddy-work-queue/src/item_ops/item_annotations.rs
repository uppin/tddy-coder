//! A work item's annotations (`#workqueue` 7/14): **labels** (free-form tags, several per item, the
//! queue view filters by them) and **badges** (keyed markers with a short text and a tone; setting a
//! key again replaces it). The agent's and the user's own — tddy never derives or clears them, and the
//! derived internal-status badge stays apart. Limits are refusals naming the limit, never truncation.

use tddy_core::changeset::{WorkItem, WorkQueue};

pub const MAX_LABELS_PER_ITEM: usize = 16;
pub const MAX_LABEL_LEN: usize = 40;
pub const MAX_BADGES_PER_ITEM: usize = 8;
pub const MAX_BADGE_TEXT_LEN: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeTone {
    Info,
    Success,
    Warning,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkItemBadge {
    pub key: String,
    pub text: String,
    pub tone: BadgeTone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationError {
    NoSuchItem(String),
    /// More labels or badges than `limit`.
    TooMany {
        limit: usize,
    },
    /// A label or badge text longer than `limit` characters.
    TooLong {
        limit: usize,
    },
}

/// Replace `item_id`'s labels with `labels` (deduplicated, in the given order).
pub fn set_labels(
    queue: &mut WorkQueue,
    item_id: &str,
    labels: Vec<String>,
) -> Result<(), AnnotationError> {
    // TODO(n7 badges-labels): implement
    let _ = (queue, item_id, labels);
    todo!("TODO(n7): implement set_labels")
}

/// Set `badge` on `item_id`, replacing a badge with the same key.
pub fn set_badge(
    queue: &mut WorkQueue,
    item_id: &str,
    badge: WorkItemBadge,
) -> Result<(), AnnotationError> {
    // TODO(n7 badges-labels): implement
    let _ = (queue, item_id, badge);
    todo!("TODO(n7): implement set_badge")
}

/// Remove `item_id`'s badge `key` (a missing key changes nothing).
pub fn clear_badge(queue: &mut WorkQueue, item_id: &str, key: &str) -> Result<(), AnnotationError> {
    // TODO(n7 badges-labels): implement
    let _ = (queue, item_id, key);
    todo!("TODO(n7): implement clear_badge")
}

pub fn labels_of(queue: &WorkQueue, item_id: &str) -> Vec<String> {
    // TODO(n7 badges-labels): implement
    let _ = (queue, item_id);
    todo!("TODO(n7): implement labels_of")
}

pub fn badges_of(queue: &WorkQueue, item_id: &str) -> Vec<WorkItemBadge> {
    // TODO(n7 badges-labels): implement
    let _ = (queue, item_id);
    todo!("TODO(n7): implement badges_of")
}
