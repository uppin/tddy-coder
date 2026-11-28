//! A work item's **files** (`#workqueue` work item files): the pure model ops behind attaching and
//! detaching. The list is queue data on the item (`WorkItem.files`), so a recording is a pure
//! function beside `item_annotations` — the bytes live in the session directory's file store
//! (`tddy_changeset::session_work_queue_files`), which the RPC layer writes first and records
//! here after. Limits are refusals naming the limit, never truncation.

use tddy_core::changeset::{WorkItem, WorkItemFile, WorkQueue};

/// The most files one item carries. A refusal names the limit; nothing is truncated.
pub const MAX_FILES_PER_ITEM: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkItemFileError {
    NoSuchItem(String),
    /// More files than [`MAX_FILES_PER_ITEM`].
    TooMany {
        limit: usize,
    },
    /// The item already carries a file with this basename — refused, never renamed.
    Duplicate {
        basename: String,
    },
    /// The item carries no file with this basename.
    NoSuchFile {
        basename: String,
    },
}

impl std::fmt::Display for WorkItemFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchItem(item_id) => write!(f, "no work item '{item_id}'"),
            Self::TooMany { limit } => {
                write!(f, "an item carries at most {limit} files")
            }
            Self::Duplicate { basename } => {
                write!(f, "the item already carries a file named '{basename}'")
            }
            Self::NoSuchFile { basename } => {
                write!(f, "the item carries no file named '{basename}'")
            }
        }
    }
}

/// Record one file on `item_id`'s list — the entry the RPC layer records after its bytes landed.
pub fn record_attached_file(
    queue: &mut WorkQueue,
    item_id: &str,
    file: WorkItemFile,
) -> Result<(), WorkItemFileError> {
    let item = item_mut(queue, item_id)?;
    if item
        .files
        .iter()
        .any(|existing| existing.basename == file.basename)
    {
        return Err(WorkItemFileError::Duplicate {
            basename: file.basename,
        });
    }
    if item.files.len() >= MAX_FILES_PER_ITEM {
        return Err(WorkItemFileError::TooMany {
            limit: MAX_FILES_PER_ITEM,
        });
    }
    item.files.push(file);
    Ok(())
}

/// Drop one file from `item_id`'s list.
pub fn record_detached_file(
    queue: &mut WorkQueue,
    item_id: &str,
    basename: &str,
) -> Result<(), WorkItemFileError> {
    let item = item_mut(queue, item_id)?;
    if !item.files.iter().any(|file| file.basename == basename) {
        return Err(WorkItemFileError::NoSuchFile {
            basename: basename.to_string(),
        });
    }
    item.files.retain(|file| file.basename != basename);
    Ok(())
}

/// `item_id`'s files, in the order they were attached. An unknown item answers empty — a read of
/// another item's list is an answer, not a leak.
pub fn files_of(queue: &WorkQueue, item_id: &str) -> Vec<WorkItemFile> {
    item_of(queue, item_id)
        .map(|item| item.files.clone())
        .unwrap_or_default()
}

fn item_mut<'q>(
    queue: &'q mut WorkQueue,
    item_id: &str,
) -> Result<&'q mut WorkItem, WorkItemFileError> {
    queue
        .nodes
        .iter_mut()
        .find(|item| item.node_id == item_id)
        .ok_or_else(|| WorkItemFileError::NoSuchItem(item_id.to_string()))
}

fn item_of<'q>(queue: &'q WorkQueue, item_id: &str) -> Option<&'q WorkItem> {
    queue.nodes.iter().find(|item| item.node_id == item_id)
}
