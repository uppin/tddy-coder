//! The start ledger: what an orchestrator polls while a session it asked for is being started
//! (`#start-parity` 4/9).
//!
//! A start runs as a `StreamStartSession` — on this host, or forwarded to another, whose events the
//! forward relays. [`begin_recorded_start`] reads the stream up to its first event, which names the
//! start's task and the host running it, hands that back as the caller's [`StartHandle`], and keeps
//! draining the rest into a [`StartLedgerRecord`] in the background: the step the start is in, the
//! clone's progress, and how it ended.
//!
//! The record is a file under the orchestrator's own session directory, so it is read on the
//! orchestrator's host whichever host runs the start, and it is still there after the daemon's task
//! list has dropped the finished task. A record still `running` when a daemon starts up is from a
//! start that daemon was draining when it stopped: [`StartLedger::recover_interrupted`] marks it
//! failed, saying so.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use std::path::{Path, PathBuf};

use futures_util::Stream;
use serde::{Deserialize, Serialize};
use tddy_service::proto::session::StartSessionEvent;

pub const GET_SESSION_START: &str = "GetSessionStart";
pub const CANCEL_SESSION_START: &str = "CancelSessionStart";

/// What the caller of an asynchronous start is handed back at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartHandle {
    /// The ledger key `GetSessionStart` and `CancelSessionStart` take.
    pub start_id: String,
    /// The background task the start runs as, on `daemon_instance_id`.
    pub task_id: String,
    pub daemon_instance_id: String,
}

/// Where a start is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// The step a start last reported (`StartPhase`), by its name: `clone`, `worktree`,
/// `semantic_index` or `agent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartStep {
    pub step: String,
    /// `running` between the step's begin and end, `done` after its end.
    pub status: String,
}

/// The latest `git clone --progress` line a start reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartCloneProgress {
    pub stage: String,
    pub percent: u32,
}

/// One start, as `GetSessionStart` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartLedgerRecord {
    pub start_id: String,
    pub task_id: String,
    pub daemon_instance_id: String,
    pub state: StartState,
    pub step: Option<StartStep>,
    pub clone_progress: Option<StartCloneProgress>,
    /// Set once the start succeeded.
    pub session_id: Option<String>,
    /// The started session's branch and worktree, as the host running it lists them; set with
    /// `session_id` (the start's own result carries neither).
    pub branch: Option<String>,
    pub worktree: Option<String>,
    /// Why the start failed or was cancelled.
    pub error: Option<String>,
    pub updated_at_unix_ms: u64,
}

/// The ledger of the starts one orchestrator session asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartLedger {
    dir: PathBuf,
}

impl StartLedger {
    /// The ledger kept in `session_dir` (`artifacts/session-starts/`).
    pub fn in_session_dir(session_dir: &Path) -> Self {
        Self {
            dir: session_dir.join("artifacts").join("session-starts"),
        }
    }

    /// The record of `start_id`, or an error naming it when this ledger has none.
    pub fn read(&self, start_id: &str) -> anyhow::Result<StartLedgerRecord> {
        // TODO(async-start): implement
        let _ = (&self.dir, start_id);
        anyhow::bail!("TODO(async-start): StartLedger::read")
    }

    /// Mark every record still `running` as failed by a daemon restart.
    pub fn recover_interrupted(&self) -> anyhow::Result<usize> {
        // TODO(async-start): implement
        let _ = &self.dir;
        anyhow::bail!("TODO(async-start): StartLedger::recover_interrupted")
    }
}

/// Read `events` up to the start's first event and return its handle; keep recording the rest in
/// `ledger` in the background. A stream that fails before naming its task is the start's refusal,
/// returned as the error.
pub async fn begin_recorded_start<S>(ledger: StartLedger, events: S) -> Result<StartHandle, String>
where
    S: Stream<Item = Result<StartSessionEvent, tddy_rpc::Status>> + Send + Unpin + 'static,
{
    // TODO(async-start): implement
    let _ = (ledger, events);
    Err("TODO(async-start): begin_recorded_start".to_string())
}
