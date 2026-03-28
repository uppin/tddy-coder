//! Session and SessionStorage — workflow state persistence.
//!
//! Mirrors graph-flow session concepts.
//!
//! JSON snapshots (`*.session.json`) live under [`workflow_engine_storage_dir`] — typically
//! `<session_dir>/.workflow/` so they stay with session documents/changeset/logs under one artifact tree.

use crate::workflow::context::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Subdirectory of the artifact session directory used only for workflow engine JSON
/// (`FileSessionStorage`). Not for user-facing session markdown, changeset, or agent files.
pub const WORKFLOW_ENGINE_STORAGE_SUBDIR: &str = ".workflow";

/// Directory for [`FileSessionStorage`] under the given artifact session directory.
#[inline]
pub fn workflow_engine_storage_dir(session_dir: &Path) -> PathBuf {
    session_dir.join(WORKFLOW_ENGINE_STORAGE_SUBDIR)
}

/// Session state for a workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub graph_id: String,
    pub current_task_id: String,
    pub status_message: Option<String>,
    pub context: Context,
}

impl Session {
    pub fn new_from_task(id: String, graph_id: String, task_id: String) -> Self {
        Self {
            id,
            graph_id,
            current_task_id: task_id,
            status_message: None,
            context: Context::new(),
        }
    }
}

/// Storage backend for sessions.
#[async_trait::async_trait]
pub trait SessionStorage: Send + Sync {
    async fn save(&self, session: &Session)
        -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn get(
        &self,
        session_id: &str,
    ) -> Result<Option<Session>, Box<dyn std::error::Error + Send + Sync>>;
    async fn delete(
        &self,
        session_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// File-based session storage (JSON on disk).
pub struct FileSessionStorage {
    base_dir: PathBuf,
}

impl FileSessionStorage {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn path(&self, session_id: &str) -> PathBuf {
        self.base_dir.join(format!("{}.session.json", session_id))
    }
}

#[async_trait::async_trait]
impl SessionStorage for FileSessionStorage {
    async fn save(
        &self,
        session: &Session,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path(&session.id);
        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        let json = serde_json::to_string_pretty(session)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    async fn get(
        &self,
        session_id: &str,
    ) -> Result<Option<Session>, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path(session_id);
        if !path.exists() {
            return Ok(None);
        }
        let json = tokio::fs::read_to_string(path).await?;
        let session: Session = serde_json::from_str(&json)?;
        Ok(Some(session))
    }

    async fn delete(
        &self,
        session_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path(session_id);
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }
        Ok(())
    }
}
