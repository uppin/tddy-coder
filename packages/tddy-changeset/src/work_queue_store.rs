//! Work queues addressed by id — **the id of a queue is the id of the session that owns it**
//! (`#workqueue` 5/14).
//!
//! The queues themselves live in their owning sessions' directories
//! ([`crate::session_work_queue::SessionWorkQueues`]); this view reads and updates one by that id and
//! lists a project's, for the callers (RPC handlers, the agent tools, the session listing) that hold a
//! `(project_id, queue_id)` pair. It creates nothing: a queue exists only because its session was
//! started with one.
//!
//! Every write is a read-modify-write of one record under a per-record lock finished by an atomic
//! rename, so two writers to the same queue both land and a reader never sees a half-written record.

use std::path::PathBuf;

use crate::changeset::WorkQueue;
use crate::session_work_queue::{SessionWorkQueueError, SessionWorkQueueRecord, SessionWorkQueues};

/// One queue as its owning session holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkQueueRecord {
    /// The owning session's id — the queue's address.
    pub queue_id: String,
    pub project_id: String,
    pub created_at_unix_ms: u64,
    /// What the queue is for. Absent when it was never titled.
    pub title: Option<String>,
    pub stack: WorkQueue,
}

impl WorkQueueRecord {
    /// The session that owns the queue (and is its id).
    pub fn owner_session_id(&self) -> &str {
        &self.queue_id
    }

    fn of(record: SessionWorkQueueRecord) -> Self {
        Self {
            queue_id: record.owner_session_id,
            project_id: record.project_id,
            created_at_unix_ms: record.created_at_unix_ms,
            title: record.title,
            stack: record.queue,
        }
    }
}

/// Why a store operation was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum WorkQueueStoreError {
    /// The project id cannot name a single directory (empty, `.`, `..`, or carrying a path separator).
    #[error("project id {0:?} cannot name a directory")]
    InvalidProjectId(String),
    /// The queue id cannot name a session directory.
    #[error("queue id {0:?} cannot name a session")]
    InvalidStackId(String),
    /// `queue_id` owns no queue of `project_id` on this host.
    #[error("project {project_id:?} holds no work queue {queue_id:?}")]
    NotFound {
        project_id: String,
        queue_id: String,
    },
    /// A record exists but does not parse.
    #[error("work queue record {path}: {message}")]
    Malformed { path: PathBuf, message: String },
    #[error("work queue store I/O on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Any other refusal of the session-keyed store, as it gave it.
    #[error(transparent)]
    Queue(SessionWorkQueueError),
}

/// The work queues of every session on this host, by queue id, under one sessions base
/// (`<base>/sessions/<id>/work-queue.yaml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkQueueStore {
    sessions_base: PathBuf,
}

impl WorkQueueStore {
    /// A store over `sessions_base` — the directory holding `sessions/` and `projects/`.
    pub fn new(sessions_base: impl Into<PathBuf>) -> Self {
        Self {
            sessions_base: sessions_base.into(),
        }
    }

    /// The host's projects directory, beside the sessions the queues live in.
    pub fn projects_dir(&self) -> PathBuf {
        self.sessions_base.join("projects")
    }

    fn queues(&self) -> SessionWorkQueues {
        SessionWorkQueues::new(self.sessions_base.join("sessions"))
    }

    /// The queue `queue_id` owns, when it belongs to `project_id`.
    pub fn read(
        &self,
        project_id: &str,
        queue_id: &str,
    ) -> Result<WorkQueueRecord, WorkQueueStoreError> {
        check_project(project_id)?;
        let not_found = || WorkQueueStoreError::NotFound {
            project_id: project_id.to_string(),
            queue_id: queue_id.to_string(),
        };
        let record = self
            .queues()
            .read(queue_id)
            .map_err(|e| store_error(e, project_id))?
            .filter(|record| record.project_id == project_id)
            .ok_or_else(not_found)?;
        Ok(WorkQueueRecord::of(record))
    }

    /// Apply `f` to the queue `queue_id` owns and write it back. Returns the record as written.
    pub fn update(
        &self,
        project_id: &str,
        queue_id: &str,
        f: impl FnOnce(&mut WorkQueue),
    ) -> Result<WorkQueueRecord, WorkQueueStoreError> {
        // Refuses a queue of another project before anything is written.
        self.read(project_id, queue_id)?;
        let record = self
            .queues()
            .update(queue_id, f)
            .map_err(|e| store_error(e, project_id))?;
        Ok(WorkQueueRecord::of(record))
    }

    /// Every queue owned by a session of `project_id` on this host, sorted by queue id.
    pub fn list(&self, project_id: &str) -> Result<Vec<WorkQueueRecord>, WorkQueueStoreError> {
        check_project(project_id)?;
        let mut records: Vec<WorkQueueRecord> = self
            .queues()
            .list_for_project(project_id)
            .map_err(|e| store_error(e, project_id))?
            .into_iter()
            .map(WorkQueueRecord::of)
            .collect();
        records.sort_by(|a, b| a.queue_id.cmp(&b.queue_id));
        Ok(records)
    }
}

fn check_project(project_id: &str) -> Result<(), WorkQueueStoreError> {
    let one_segment = !project_id.is_empty()
        && project_id != "."
        && project_id != ".."
        && !project_id.contains(['/', '\\', '\0']);
    if one_segment {
        Ok(())
    } else {
        Err(WorkQueueStoreError::InvalidProjectId(
            project_id.to_string(),
        ))
    }
}

/// `error` as this view's, for a call about `project_id`.
fn store_error(error: SessionWorkQueueError, project_id: &str) -> WorkQueueStoreError {
    match error {
        SessionWorkQueueError::InvalidSessionId(id) => WorkQueueStoreError::InvalidStackId(id),
        SessionWorkQueueError::NoQueue { session_id } => WorkQueueStoreError::NotFound {
            project_id: project_id.to_string(),
            queue_id: session_id,
        },
        SessionWorkQueueError::Malformed { path, message } => {
            WorkQueueStoreError::Malformed { path, message }
        }
        SessionWorkQueueError::Io { path, source } => WorkQueueStoreError::Io { path, source },
        other @ SessionWorkQueueError::AlreadyExists { .. } => WorkQueueStoreError::Queue(other),
    }
}
