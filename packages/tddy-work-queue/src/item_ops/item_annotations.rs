//! A work item's annotations (`#workqueue` 7/14): **labels** (free-form tags, several per item, the
//! queue view filters by them) and **badges** (keyed markers with a short text and a tone; setting a
//! key again replaces it). The agent's and the user's own — tddy never derives or clears them, and the
//! derived internal-status badge stays apart. Limits are refusals naming the limit, never truncation.

use tddy_core::changeset::{WorkItem, WorkQueue};

pub const MAX_LABELS_PER_ITEM: usize = 16;
pub const MAX_LABEL_LEN: usize = 40;
pub const MAX_BADGES_PER_ITEM: usize = 8;
pub const MAX_BADGE_TEXT_LEN: usize = 60;

// The annotations live on the item, so their records are the changeset's types; re-exported as this
// module's published surface.
pub use tddy_core::changeset::{BadgeTone, WorkItemBadge};

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

fn item_mut<'q>(
    queue: &'q mut WorkQueue,
    item_id: &str,
) -> Result<&'q mut WorkItem, AnnotationError> {
    queue
        .nodes
        .iter_mut()
        .find(|item| item.node_id == item_id)
        .ok_or_else(|| AnnotationError::NoSuchItem(item_id.to_string()))
}

/// Replace `item_id`'s labels with `labels` (deduplicated, in the given order).
pub fn set_labels(
    queue: &mut WorkQueue,
    item_id: &str,
    labels: Vec<String>,
) -> Result<(), AnnotationError> {
    let item = item_mut(queue, item_id)?;
    let mut unique: Vec<String> = Vec::with_capacity(labels.len());
    for label in labels {
        if label.chars().count() > MAX_LABEL_LEN {
            return Err(AnnotationError::TooLong {
                limit: MAX_LABEL_LEN,
            });
        }
        if !unique.contains(&label) {
            unique.push(label);
        }
    }
    if unique.len() > MAX_LABELS_PER_ITEM {
        return Err(AnnotationError::TooMany {
            limit: MAX_LABELS_PER_ITEM,
        });
    }
    item.labels = unique;
    Ok(())
}

/// Set `badge` on `item_id`, replacing a badge with the same key.
pub fn set_badge(
    queue: &mut WorkQueue,
    item_id: &str,
    badge: WorkItemBadge,
) -> Result<(), AnnotationError> {
    let item = item_mut(queue, item_id)?;
    if badge.text.chars().count() > MAX_BADGE_TEXT_LEN {
        return Err(AnnotationError::TooLong {
            limit: MAX_BADGE_TEXT_LEN,
        });
    }
    if let Some(existing) = item.badges.iter_mut().find(|b| b.key == badge.key) {
        *existing = badge;
    } else if item.badges.len() >= MAX_BADGES_PER_ITEM {
        return Err(AnnotationError::TooMany {
            limit: MAX_BADGES_PER_ITEM,
        });
    } else {
        item.badges.push(badge);
    }
    Ok(())
}

/// Remove `item_id`'s badge `key` (a missing key changes nothing).
pub fn clear_badge(queue: &mut WorkQueue, item_id: &str, key: &str) -> Result<(), AnnotationError> {
    item_mut(queue, item_id)?.badges.retain(|b| b.key != key);
    Ok(())
}

fn item_of<'q>(queue: &'q WorkQueue, item_id: &str) -> Option<&'q WorkItem> {
    queue.nodes.iter().find(|item| item.node_id == item_id)
}

pub fn labels_of(queue: &WorkQueue, item_id: &str) -> Vec<String> {
    item_of(queue, item_id)
        .map(|item| item.labels.clone())
        .unwrap_or_default()
}

pub fn badges_of(queue: &WorkQueue, item_id: &str) -> Vec<WorkItemBadge> {
    item_of(queue, item_id)
        .map(|item| item.badges.clone())
        .unwrap_or_default()
}
