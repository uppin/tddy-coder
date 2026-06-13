//! Registry for background shell jobs spawned by the `Shell` tool (block_until_ms=0).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{watch, RwLock};

/// A single background shell job.
pub struct ShellJob {
    pub job_id: String,
    pub session_id: String,
    stdout_buf: Arc<std::sync::Mutex<Vec<u8>>>,
    done_tx: watch::Sender<bool>,
    done_rx: watch::Receiver<bool>,
    exit_code: Arc<std::sync::Mutex<Option<i32>>>,
}

impl ShellJob {
    pub fn new(job_id: String, session_id: String) -> Self {
        let (done_tx, done_rx) = watch::channel(false);
        Self {
            job_id,
            session_id,
            stdout_buf: Arc::new(std::sync::Mutex::new(Vec::new())),
            done_tx,
            done_rx,
            exit_code: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn done_receiver(&self) -> watch::Receiver<bool> {
        self.done_rx.clone()
    }

    pub fn stdout_clone(&self) -> Arc<std::sync::Mutex<Vec<u8>>> {
        self.stdout_buf.clone()
    }

    pub fn exit_code_clone(&self) -> Arc<std::sync::Mutex<Option<i32>>> {
        self.exit_code.clone()
    }

    pub fn done_sender(&self) -> watch::Sender<bool> {
        self.done_tx.clone()
    }
}

/// Thread-safe registry of background shell jobs, keyed by job ID.
#[derive(Clone)]
pub struct ShellJobRegistry {
    jobs: Arc<RwLock<HashMap<String, Arc<ShellJob>>>>,
}

impl Default for ShellJobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellJobRegistry {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new job in the registry.
    pub async fn register(&self, job: Arc<ShellJob>) {
        self.jobs.write().await.insert(job.job_id.clone(), job);
    }

    /// Look up a job by ID.
    pub async fn get(&self, job_id: &str) -> Option<Arc<ShellJob>> {
        self.jobs.read().await.get(job_id).cloned()
    }

    /// Remove a completed job from the registry to prevent unbounded memory growth.
    pub async fn remove(&self, job_id: &str) {
        self.jobs.write().await.remove(job_id);
    }
}
