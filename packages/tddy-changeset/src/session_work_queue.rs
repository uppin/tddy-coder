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

use serde::{Deserialize, Serialize};

use crate::changeset::WorkQueue;

/// The file a session's queue is stored in, inside the session directory.
pub const WORK_QUEUE_FILE: &str = "work-queue.yaml";

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
        // TODO(n5 session-work-queue): implement
        let _ = (
            &self.sessions_dir,
            session_id,
            project_id,
            created_at_unix_ms,
        );
        todo!("TODO(n5): implement SessionWorkQueues::create_for_session")
    }

    /// `session_id`'s queue, or `None` when it owns none.
    pub fn read(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionWorkQueueRecord>, SessionWorkQueueError> {
        // TODO(n5 session-work-queue): implement
        let _ = session_id;
        todo!("TODO(n5): implement SessionWorkQueues::read")
    }

    /// Change `session_id`'s queue under its record lock; `NoQueue` when it owns none.
    pub fn update<F: FnOnce(&mut WorkQueue)>(
        &self,
        session_id: &str,
        change: F,
    ) -> Result<SessionWorkQueueRecord, SessionWorkQueueError> {
        // TODO(n5 session-work-queue): implement
        let _ = (session_id, change);
        todo!("TODO(n5): implement SessionWorkQueues::update")
    }

    /// Every queue owned by a session of `project_id` on this host.
    pub fn list_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<SessionWorkQueueRecord>, SessionWorkQueueError> {
        // TODO(n5 session-work-queue): implement
        let _ = project_id;
        todo!("TODO(n5): implement SessionWorkQueues::list_for_project")
    }
}

/// Whether a session starts with a queue: asked for ("with work queue"), or an orchestrator start.
/// The only role-aware rule of the work queue; access never reads a role.
pub fn starts_with_work_queue(with_work_queue: bool, session_role: Option<&str>) -> bool {
    // TODO(n5 session-work-queue): implement
    let _ = (with_work_queue, session_role);
    todo!("TODO(n5): implement starts_with_work_queue")
}

/// Delete the pre-queue project stack records (`<projects_dir>/*/stacks/`, records and `.op.json`
/// journals). Returns how many project `stacks` directories were removed; idempotent.
pub fn drop_old_project_stack_records(projects_dir: &Path) -> std::io::Result<usize> {
    // TODO(n5 session-work-queue): implement
    let _ = projects_dir;
    todo!("TODO(n5): implement drop_old_project_stack_records")
}
