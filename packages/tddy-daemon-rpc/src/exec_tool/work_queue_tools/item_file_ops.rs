//! The work-item file ops behind `work_item_attach_file`, `work_item_detach_file` and
//! `work_item_list_files` — served here rather than in the blocking dispatcher, because an attach
//! resolves its bytes through the host-document gates and may fetch a staged file from a peer.
//!
//! An agent attaches a file the queue-owner host can already read: a path under the owning
//! session's own artifacts (the queue's id *is* the owner's session id), or one inside the
//! project's main repository. The browser's uploads ride the staged path in the RPC instead —
//! no agent stages bytes.

use serde_json::{json, Value};
use std::path::PathBuf;
use tddy_core::changeset::WorkItemFile;
use tddy_fs::Fs;
use tddy_service::proto::types::HostDocumentScope;
use tddy_session_files::host_documents::resolve_host_document;
use tddy_tool_engine::work_queue_tools::{FileSource, WorkQueueOp};
use tddy_work_queue::item_ops::item_files;

use super::OpEnv;
use tddy_work_queue::item_ops::WorkQueueHome;

/// What the file ops need beyond the queue itself: the gates and caps of the host that holds it.
pub(super) struct FileEnv {
    pub(super) os_user: String,
    pub(super) tddy_data_dir: PathBuf,
    pub(super) staging_base_dir: PathBuf,
    pub(super) max_attachment_bytes: u64,
}

/// Run one file op against `env`'s queue. The queue id is the owner session's, which is what roots
/// the artifact arm and names the files' directory.
pub(super) async fn run(env: &OpEnv, files: &FileEnv, op: WorkQueueOp) -> Result<Value, String> {
    let queue_id = env.queue_id.clone().ok_or_else(|| {
        "the work item's files are addressed by the queue they belong to: name queue_id".to_string()
    })?;
    match op {
        WorkQueueOp::ListFiles { item_id } => {
            let stack = env.stack()?;
            let item = stack
                .node(&item_id)
                .ok_or_else(|| format!("item '{item_id}' not found"))?;
            Ok(json!({
                "item_id": item_id,
                "files": item.files.iter().map(|file| json!({
                    "basename": file.basename,
                    "size_bytes": file.size_bytes,
                })).collect::<Vec<_>>(),
            }))
        }
        WorkQueueOp::AttachFile {
            item_id,
            basename,
            source,
        } => {
            let (bytes, size_bytes) =
                read_source(env, files, &queue_id, &source).map_err(|e| format!("attach: {e}"))?;
            if size_bytes > files.max_attachment_bytes {
                return Err(format!(
                    "attach: '{basename}' is {size_bytes} bytes; a work item file is at most {}",
                    files.max_attachment_bytes
                ));
            }
            // `sessions_base` is the data root; the files store takes the sessions directory
            // under it, exactly as `SessionWorkQueues` does.
            let store = tddy_core::session_work_queue_files::SessionWorkQueueFiles::new(
                env.sessions_base.join("sessions"),
            );
            store
                .store(&queue_id, &item_id, &basename, &bytes)
                .map_err(|e| format!("attach: {e}"))?;
            let mut refused = None;
            env.home()?.update_stack(|queue: &mut tddy_core::changeset::WorkQueue| {
                    let mut edited = queue.clone();
                    match item_files::record_attached_file(
                        &mut edited,
                        &item_id,
                        WorkItemFile {
                            basename: basename.clone(),
                            size_bytes,
                        },
                    ) {
                        Ok(()) => *queue = edited,
                        Err(error) => refused = Some(error),
                    }
                })
                .map_err(|e| format!("attach: {e}"))?;
            if let Some(error) = refused {
                // The bytes were written for an entry that was never recorded — remove them, so a
                // retry is not blocked and no orphan outlives the refusal.
                let _ = store.remove(&queue_id, &item_id, &basename);
                return Err(format!("attach: {error}"));
            }
            files_answer(&env.stack()?, &item_id)
        }
        WorkQueueOp::DetachFile { item_id, basename } => {
            let store = tddy_core::session_work_queue_files::SessionWorkQueueFiles::new(
                env.sessions_base.join("sessions"),
            );
            // The bytes go first: a refusal here leaves the queue exactly as it was.
            store
                .remove(&queue_id, &item_id, &basename)
                .map_err(|e| format!("detach: {e}"))?;
            let mut refused = None;
            env.home()?.update_stack(|queue: &mut tddy_core::changeset::WorkQueue| {
                    let mut edited = queue.clone();
                    match item_files::record_detached_file(&mut edited, &item_id, &basename) {
                        Ok(()) => *queue = edited,
                        Err(error) => refused = Some(error),
                    }
                })
                .map_err(|e| format!("detach: {e}"))?;
            if let Some(error) = refused {
                return Err(format!("detach: {error}"));
            }
            files_answer(&env.stack()?, &item_id)
        }
        other => Err(format!(
            "{other:?} is not a work item file op — served by the blocking dispatcher or the GitHub \
             inbox"
        )),
    }
}

fn files_answer(stack: &tddy_core::changeset::WorkQueue, item_id: &str) -> Result<Value, String> {
    let item = stack
        .node(item_id)
        .ok_or_else(|| format!("item '{item_id}' not found"))?;
    Ok(json!({
        "item_id": item_id,
        "files": item.files.iter().map(|file| json!({
            "basename": file.basename,
            "size_bytes": file.size_bytes,
        })).collect::<Vec<_>>(),
    }))
}

/// The bytes one source names, resolved through the host-document gates on this host — the owner
/// host, wherever the agent sits.
fn read_source(
    env: &OpEnv,
    files: &FileEnv,
    queue_id: &str,
    source: &FileSource,
) -> Result<(Vec<u8>, u64), String> {
    let (scope, session_id, project_id, relative_path) = match source {
        FileSource::ArtifactRelativePath(relative_path) => (
            HostDocumentScope::SessionArtifact,
            queue_id.to_string(),
            String::new(),
            relative_path.clone(),
        ),
        FileSource::RepoFilePath(relative_path) => (
            HostDocumentScope::ProjectRepo,
            String::new(),
            env.project_id.clone(),
            relative_path.clone(),
        ),
    };
    let resolved = resolve_host_document(
        &files.os_user,
        &files.tddy_data_dir,
        &files.staging_base_dir,
        scope,
        &session_id,
        &project_id,
        &relative_path,
    )
    .map_err(|e| e.message().to_string())?;
    if resolved.byte_size > files.max_attachment_bytes {
        return Err(format!(
            "the file is {} bytes; a work item file is at most {}",
            resolved.byte_size, files.max_attachment_bytes
        ));
    }
    let bytes = tddy_fs::DiskFs
        .read(&resolved.path)
        .map_err(|e| format!("failed to read the file: {e}"))?;
    Ok((bytes, resolved.byte_size))
}
