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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use tddy_rpc::Code;
use tddy_service::proto::session::start_phase::{Boundary, Step};
use tddy_service::proto::session::start_session_event::Event;
use tddy_service::proto::session::{StartPhase, StartSessionEvent, StartSessionResponse};

pub const GET_SESSION_START: &str = "GetSessionStart";
pub const CANCEL_SESSION_START: &str = "CancelSessionStart";

/// The reason a record still `running` at a daemon's restart is marked failed with.
const RESTARTED_DURING_START: &str = "the daemon restarted during the start";

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

/// Looks up a started session as the host running it lists it, for the record's branch and
/// worktree. The drain asks once, when the start succeeds, before the record is written as
/// succeeded, so a reader never sees a session without its branch.
#[async_trait]
pub trait StartedSessionDetails: Send + Sync {
    /// The branch and worktree of `session_id` on `daemon_instance_id`, or `None` when that host
    /// does not list it.
    async fn branch_and_worktree(
        &self,
        daemon_instance_id: &str,
        session_id: &str,
    ) -> Option<(String, String)>;
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
        let path = self.path_of(start_id)?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                anyhow::bail!("no start {start_id} is recorded for this session")
            }
            Err(e) => {
                return Err(e).with_context(|| format!("reading start {start_id}'s record"));
            }
        };
        serde_json::from_slice(&bytes)
            .with_context(|| format!("start {start_id}'s record is not readable"))
    }

    /// Mark every record still `running` as failed by a daemon restart; how many.
    pub fn recover_interrupted(&self) -> anyhow::Result<usize> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e).context("reading the session-start ledger"),
        };
        let mut recovered = 0;
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes =
                std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
            let mut record: StartLedgerRecord = serde_json::from_slice(&bytes)
                .with_context(|| format!("{} is not a start record", path.display()))?;
            if record.state != StartState::Running {
                continue;
            }
            record.state = StartState::Failed;
            record.error = Some(RESTARTED_DURING_START.to_string());
            record.updated_at_unix_ms = now_unix_ms();
            self.persist(&record)?;
            recovered += 1;
        }
        Ok(recovered)
    }

    /// The file a start's record is kept in. A start id names a file, so only the characters a
    /// generated id uses are accepted: a caller's id cannot reach outside this ledger.
    fn path_of(&self, start_id: &str) -> anyhow::Result<PathBuf> {
        let well_formed = !start_id.is_empty()
            && start_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !well_formed {
            anyhow::bail!("{start_id:?} is not a start id");
        }
        Ok(self.dir.join(format!("{start_id}.json")))
    }

    /// Write `record` in place of the one it replaces, atomically: a reader sees the old record or
    /// the new one, never a part of either.
    fn persist(&self, record: &StartLedgerRecord) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("creating {}", self.dir.display()))?;
        let path = self.path_of(&record.start_id)?;
        let tmp = self.dir.join(format!("{}.json.tmp", record.start_id));
        let bytes = serde_json::to_vec_pretty(record)?;
        std::fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }
}

/// Read `events` up to the start's first event and return its handle; keep recording the rest in
/// `ledger` in the background. A stream that fails before naming its task is the start's refusal,
/// returned as the error.
///
/// The session's branch and worktree are not looked up: use [`begin_recorded_start_with`] for a
/// start whose record should carry them.
pub async fn begin_recorded_start<S>(ledger: StartLedger, events: S) -> Result<StartHandle, String>
where
    S: Stream<Item = Result<StartSessionEvent, tddy_rpc::Status>> + Send + Unpin + 'static,
{
    begin_recorded_start_with(ledger, events, None).await
}

/// [`begin_recorded_start`], with `details` looking up the started session's branch and worktree.
pub async fn begin_recorded_start_with<S>(
    ledger: StartLedger,
    mut events: S,
    details: Option<Arc<dyn StartedSessionDetails>>,
) -> Result<StartHandle, String>
where
    S: Stream<Item = Result<StartSessionEvent, tddy_rpc::Status>> + Send + Unpin + 'static,
{
    let (task_id, daemon_instance_id) = match events.next().await {
        Some(Ok(StartSessionEvent {
            event: Some(Event::Started(started)),
        })) => (started.task_id, started.daemon_instance_id),
        Some(Err(status)) => return Err(status.message().to_string()),
        Some(Ok(_)) => {
            return Err("the start did not name its task before its first event".to_string())
        }
        None => return Err("the start ended before naming its task".to_string()),
    };
    let start_id = new_start_id();
    let record = StartLedgerRecord {
        start_id: start_id.clone(),
        task_id: task_id.clone(),
        daemon_instance_id: daemon_instance_id.clone(),
        state: StartState::Running,
        step: None,
        clone_progress: None,
        session_id: None,
        branch: None,
        worktree: None,
        error: None,
        updated_at_unix_ms: now_unix_ms(),
    };
    ledger
        .persist(&record)
        .map_err(|e| format!("the start could not be recorded: {e:#}"))?;
    tokio::spawn(record_until_end(ledger, record, events, details));
    Ok(StartHandle {
        start_id,
        task_id,
        daemon_instance_id,
    })
}

/// How a start ended, as its record is written.
enum Ending {
    Succeeded {
        session_id: String,
        branch: Option<String>,
        worktree: Option<String>,
    },
    Failed(String),
    Cancelled(String),
}

/// Apply the stream's events to `record` until the stream gives its result, fails, or ends, and
/// write the outcome.
async fn record_until_end<S>(
    ledger: StartLedger,
    mut record: StartLedgerRecord,
    mut events: S,
    details: Option<Arc<dyn StartedSessionDetails>>,
) where
    S: Stream<Item = Result<StartSessionEvent, tddy_rpc::Status>> + Send + Unpin,
{
    let ending = loop {
        match events.next().await {
            None => break Ending::Failed("the start's stream ended without a result".to_string()),
            Some(Ok(StartSessionEvent {
                event: Some(Event::Result(response)),
            })) => break ending_of_result(response, &record, details.as_deref()).await,
            Some(Ok(event)) => {
                if apply_progress(&mut record, event) {
                    persist_progress(&ledger, &record);
                }
            }
            Some(Err(status)) if status.code == Code::Cancelled => {
                break Ending::Cancelled(status.message().to_string());
            }
            Some(Err(status)) => break Ending::Failed(status.message().to_string()),
        }
    };
    apply_ending(&mut record, ending);
    if let Err(e) = ledger.persist(&record) {
        log::error!(
            "start {}: its outcome could not be recorded; it will read running: {e:#}",
            record.start_id
        );
    }
}

/// The outcome a start's own result gives, with its session's branch and worktree looked up.
async fn ending_of_result(
    response: StartSessionResponse,
    record: &StartLedgerRecord,
    details: Option<&dyn StartedSessionDetails>,
) -> Ending {
    if let Some(conflict) = response.branch_conflict {
        return Ending::Failed(format!(
            "the session was not started: branch {} is owned",
            conflict.branch
        ));
    }
    if response.session_id.is_empty() {
        return Ending::Failed("the start ended without a session".to_string());
    }
    let (branch, worktree) = match details {
        Some(details) => match details
            .branch_and_worktree(&record.daemon_instance_id, &response.session_id)
            .await
        {
            Some((branch, worktree)) => (Some(branch), Some(worktree)),
            None => (None, None),
        },
        None => (None, None),
    };
    Ending::Succeeded {
        session_id: response.session_id,
        branch,
        worktree,
    }
}

fn apply_ending(record: &mut StartLedgerRecord, ending: Ending) {
    match ending {
        Ending::Succeeded {
            session_id,
            branch,
            worktree,
        } => {
            record.state = StartState::Succeeded;
            record.session_id = Some(session_id);
            record.branch = branch;
            record.worktree = worktree;
        }
        Ending::Failed(message) => {
            record.state = StartState::Failed;
            record.error = Some(message);
        }
        Ending::Cancelled(message) => {
            record.state = StartState::Cancelled;
            record.error = Some(message);
        }
    }
    record.updated_at_unix_ms = now_unix_ms();
}

/// Fold one progress event into `record`; whether anything the record shows changed.
fn apply_progress(record: &mut StartLedgerRecord, event: StartSessionEvent) -> bool {
    match event.event {
        Some(Event::Phase(phase)) => match step_of(&phase) {
            Some(step) => {
                record.step = Some(step);
                true
            }
            None => false,
        },
        Some(Event::CloneProgress(progress)) => {
            record.clone_progress = Some(StartCloneProgress {
                stage: progress.stage,
                percent: progress.percent,
            });
            true
        }
        _ => false,
    }
}

fn step_of(phase: &StartPhase) -> Option<StartStep> {
    let name = match Step::try_from(phase.step).ok()? {
        Step::Worktree => "worktree",
        Step::SemanticIndex => "semantic_index",
        Step::Agent => "agent",
        Step::Clone => "clone",
        Step::Unspecified => return None,
    };
    let status = match Boundary::try_from(phase.boundary).ok()? {
        Boundary::Begin => "running",
        Boundary::End => "done",
        Boundary::Unspecified => return None,
    };
    Some(StartStep {
        step: name.to_string(),
        status: status.to_string(),
    })
}

/// A progress write that fails is logged: the start is still running, and the next write or the
/// outcome carries the record forward.
fn persist_progress(ledger: &StartLedger, record: &StartLedgerRecord) {
    if let Err(e) = ledger.persist(record) {
        log::warn!("start {}: progress not recorded: {e:#}", record.start_id);
    }
}

/// A start id unique across this process's starts and across daemon restarts on this host.
fn new_start_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("start-{}-{}-{}", now_unix_ms(), std::process::id(), seq)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
