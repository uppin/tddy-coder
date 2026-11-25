use serde_json::{json, Value};

use tddy_core::changeset::WorkQueue;
use tddy_work_queue::item_ops::{item_annotations, item_links};

use super::OpEnv;
use tddy_work_queue::item_ops::WorkQueueHome;

/// A refused annotation edit as the tool reports it — the same words the RPC uses.
fn annotation_refusal_message(error: item_annotations::AnnotationError) -> String {
    use item_annotations::AnnotationError::{NoSuchItem, TooLong, TooMany};
    match error {
        NoSuchItem(item) => format!("item '{item}' not found"),
        TooMany { limit } => format!("too many: the limit is {limit}"),
        TooLong { limit } => format!("too long: the limit is {limit} characters"),
    }
}

/// Apply an annotation `edit` to the queue under its record lock; a refused edit writes nothing and
/// is the error, and the answer is the item's labels and badges as stored.
pub(crate) fn edit_annotations(
    env: &OpEnv,
    item_id: &str,
    edit: impl FnOnce(&mut WorkQueue) -> Result<(), item_annotations::AnnotationError>,
) -> Result<Value, String> {
    let mut refused = None;
    env.home()?
        .update_stack(|queue| {
            let mut edited = queue.clone();
            match edit(&mut edited) {
                Ok(()) => *queue = edited,
                Err(error) => refused = Some(error),
            }
        })
        .map_err(|e| format!("annotate: {e}"))?;
    if let Some(error) = refused {
        return Err(annotation_refusal_message(error));
    }
    let stack = env.stack()?;
    Ok(json!({
        "item_id": item_id,
        "labels": item_annotations::labels_of(&stack, item_id),
        "badges": item_annotations::badges_of(&stack, item_id)
            .into_iter()
            .map(|b| json!({ "key": b.key, "text": b.text, "tone": b.tone }))
            .collect::<Vec<_>>(),
    }))
}

/// Apply a link `edit` to the queue under its record lock; a refused edit writes nothing and is the
/// error, and the answer is the item's links as stored.
pub(crate) fn edit_links(
    env: &OpEnv,
    item_id: &str,
    edit: impl FnOnce(&mut WorkQueue) -> Result<(), item_links::ItemLinkError>,
) -> Result<Value, String> {
    let mut refused = None;
    env.home()?
        .update_stack(|queue| {
            let mut edited = queue.clone();
            match edit(&mut edited) {
                Ok(()) => *queue = edited,
                Err(error) => refused = Some(error),
            }
        })
        .map_err(|e| format!("link: {e}"))?;
    if let Some(error) = refused {
        return Err(error.to_string());
    }
    let stack = env.stack()?;
    Ok(json!({
        "item_id": item_id,
        "pr": item_links::assigned_pr_of(&stack, item_id).map(|pr| json!({
            "number": pr.number, "url": pr.url, "head_branch": pr.head_branch,
        })),
        "linked_sessions": item_links::linked_sessions_of(&stack, item_id)
            .into_iter()
            .map(|s| json!({ "session_id": s.session_id, "daemon_instance_id": s.daemon_instance_id }))
            .collect::<Vec<_>>(),
    }))
}
