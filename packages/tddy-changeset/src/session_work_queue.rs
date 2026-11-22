//! A session's work queue (`#workqueue` 5/14): **one queue per owning session**, stored in that
//! session's directory (`<data>/sessions/<session_id>/work-queue.yaml`), so it lives and is removed with
//! the session and is read on the session's own daemon.
//!
//! A queue comes into being **at session start** only — a start "with work queue", or any orchestrator
//! start (main and every other orchestrator); [`starts_with_work_queue`] is the one place a role is
//! consulted. Addressing a session that owns no queue is refused with [`NO_WORK_QUEUE_HINT`].
//!
//! The project stack records that preceded this (`<data>/projects/<project_id>/stacks/`) are **dropped**
//! on upgrade, not migrated (operator decision, 2026-11-22): [`drop_old_project_stack_records`].
//!
//! The queue is a `WorkQueue` of `WorkItem`s (named by `#workqueue`
//! 3/14); this module's own names are final.

use std::path::{Path, PathBuf};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::changeset::WorkQueue;

/// The file a session's queue is stored in, inside the session directory.
pub const WORK_QUEUE_FILE: &str = "work-queue.yaml";

/// The in-flight operation journal of a session's queue, beside the queue in the session directory.
pub const WORK_QUEUE_OP_JOURNAL_FILE: &str = "work-queue.op.json";

/// How a queue comes to exist — the tail of every "no queue" refusal.
pub const NO_WORK_QUEUE_HINT: &str =
    "a queue is created when a session is started with a work queue";

/// A session's queue on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionWorkQueueRecord {
    pub owner_session_id: String,
    pub project_id: String,
    pub created_at_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub queue: WorkQueue,
}

#[derive(Debug)]
pub enum SessionWorkQueueError {
    /// The session owns no queue.
    NoQueue {
        session_id: String,
    },
    /// The session already owns one (creation is once, at start).
    AlreadyExists {
        session_id: String,
    },
    InvalidSessionId(String),
    Malformed {
        path: PathBuf,
        message: String,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for SessionWorkQueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoQueue { session_id } => {
                write!(
                    f,
                    "session {session_id} has no work queue — {NO_WORK_QUEUE_HINT}"
                )
            }
            Self::AlreadyExists { session_id } => {
                write!(f, "session {session_id} already owns a work queue")
            }
            Self::InvalidSessionId(id) => write!(f, "invalid session id {id:?}"),
            Self::Malformed { path, message } => {
                write!(f, "malformed work queue {}: {message}", path.display())
            }
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for SessionWorkQueueError {}

/// Every session's queue on one host, addressed by session id.
pub struct SessionWorkQueues {
    sessions_dir: PathBuf,
}

impl SessionWorkQueues {
    /// `sessions_dir` is `<data>/sessions`.
    pub fn new(sessions_dir: impl Into<PathBuf>) -> Self {
        Self {
            sessions_dir: sessions_dir.into(),
        }
    }

    /// Create `session_id`'s empty queue — at its start, once.
    pub fn create_for_session(
        &self,
        session_id: &str,
        project_id: &str,
        created_at_unix_ms: u64,
    ) -> Result<SessionWorkQueueRecord, SessionWorkQueueError> {
        let path = self.queue_path(session_id)?;
        with_record_lock(&path, || {
            if path.exists() {
                return Err(SessionWorkQueueError::AlreadyExists {
                    session_id: session_id.to_string(),
                });
            }
            let record = SessionWorkQueueRecord {
                owner_session_id: session_id.to_string(),
                project_id: project_id.to_string(),
                created_at_unix_ms,
                title: None,
                queue: WorkQueue {
                    version: 1,
                    nodes: Vec::new(),
                    ..Default::default()
                },
            };
            write_record(&path, &record)?;
            Ok(record)
        })
    }

    /// `session_id`'s queue, or `None` when it owns none.
    pub fn read(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionWorkQueueRecord>, SessionWorkQueueError> {
        read_record(&self.queue_path(session_id)?)
    }

    /// Change `session_id`'s queue under its record lock; `NoQueue` when it owns none.
    pub fn update<F: FnOnce(&mut WorkQueue)>(
        &self,
        session_id: &str,
        change: F,
    ) -> Result<SessionWorkQueueRecord, SessionWorkQueueError> {
        let path = self.queue_path(session_id)?;
        with_record_lock(&path, || {
            let mut record = read_record(&path)?.ok_or_else(|| SessionWorkQueueError::NoQueue {
                session_id: session_id.to_string(),
            })?;
            change(&mut record.queue);
            write_record(&path, &record)?;
            Ok(record)
        })
    }

    /// Every queue owned by a session of `project_id` on this host.
    pub fn list_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<SessionWorkQueueRecord>, SessionWorkQueueError> {
        let entries = match std::fs::read_dir(&self.sessions_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(SessionWorkQueueError::Io {
                    path: self.sessions_dir.clone(),
                    source,
                })
            }
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            if let Some(record) = read_record(&entry.path().join(WORK_QUEUE_FILE))? {
                if record.project_id == project_id {
                    found.push(record);
                }
            }
        }
        Ok(found)
    }

    /// Where `session_id`'s queue journals an in-flight merge or repoint, beside the queue.
    pub fn op_journal_path(&self, session_id: &str) -> Result<PathBuf, SessionWorkQueueError> {
        Ok(self
            .queue_path(session_id)?
            .with_file_name(WORK_QUEUE_OP_JOURNAL_FILE))
    }

    fn queue_path(&self, session_id: &str) -> Result<PathBuf, SessionWorkQueueError> {
        let one_segment = !session_id.is_empty()
            && session_id != "."
            && session_id != ".."
            && !session_id.contains(['/', '\\', '\0']);
        if !one_segment {
            return Err(SessionWorkQueueError::InvalidSessionId(
                session_id.to_string(),
            ));
        }
        Ok(self.sessions_dir.join(session_id).join(WORK_QUEUE_FILE))
    }
}

/// Whether a session starts with a queue: asked for ("with work queue"), or an orchestrator start.
/// The only role-aware rule of the work queue; access never reads a role.
pub fn starts_with_work_queue(with_work_queue: bool, session_role: Option<&str>) -> bool {
    with_work_queue || session_role.is_some_and(crate::session_metadata::is_orchestrator_role)
}

/// Delete the pre-queue project stack records (`<projects_dir>/*/stacks/`, records and `.op.json`
/// journals). Returns how many project `stacks` directories were removed; idempotent.
pub fn drop_old_project_stack_records(projects_dir: &Path) -> std::io::Result<usize> {
    let entries = match std::fs::read_dir(projects_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut dropped = 0;
    for entry in entries {
        let stacks = entry?.path().join("stacks");
        if stacks.is_dir() {
            std::fs::remove_dir_all(&stacks)?;
            dropped += 1;
        }
    }
    Ok(dropped)
}

fn read_record(path: &Path) -> Result<Option<SessionWorkQueueRecord>, SessionWorkQueueError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(SessionWorkQueueError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    };
    serde_yaml::from_str(&text)
        .map(Some)
        .map_err(|e| SessionWorkQueueError::Malformed {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
}

fn write_record(path: &Path, record: &SessionWorkQueueRecord) -> Result<(), SessionWorkQueueError> {
    let text = serde_yaml::to_string(record).map_err(|e| SessionWorkQueueError::Malformed {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    tddy_session_store::atomic_file::write_atomic(path, text).map_err(|source| {
        SessionWorkQueueError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// The in-process lock of one record, shared by every handle in this process.
fn with_record_lock<T>(path: &Path, f: impl FnOnce() -> T) -> T {
    static LOCKS: std::sync::OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
        std::sync::OnceLock::new();
    let lock = {
        let mut locks = LOCKS
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(locks.entry(path.to_path_buf()).or_default())
    };
    let _held = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f()
}
