//! An orchestrator's watches over the sessions it can see (`#activity-wake` 2/3).
//!
//! A watch is a filter — which sessions, which kinds of event — with its own position in the
//! project notification log and a time to live. It copies nothing: the log already keeps, scopes
//! and evicts every record a watch could want, so a watch only remembers how far it has delivered.
//! [`SessionWatchStore::take_delivery`] hands an orchestrator what its watches matched since it
//! last asked, each record once, and ends the watches that have ended.
//!
//! Persisted per project as `<data dir>/projects/<project_id>/session-watches.json`, so a watch
//! and its position survive a daemon restart, and — keyed by the orchestrator's session id, which
//! `/clear` keeps — its conversation rotating.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-16-session-watches.md

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::project_notification_log::{ProjectNotificationLog, ProjectNotificationRecord};

/// The longest a watch lives without being renewed: 24 hours.
pub const MAX_WATCH_TTL_SECS: u64 = 86_400;
/// How many live watches one orchestrator may hold.
pub const MAX_LIVE_WATCHES: usize = 16;
/// How many matched events one delivery carries; the rest wait for the next.
pub const MAX_EVENTS_PER_ANSWER: usize = 50;

/// What a store keeps per project, under the project's directory.
pub const SESSION_WATCHES_FILE: &str = "session-watches.json";

/// The refusal of a store method not yet implemented.
// TODO(session-watches): implement — drop this once every method is.
const NOT_YET: &str = "session watches are not implemented yet";

/// One kind of event a watch can ask for, over the log's records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchKind {
    /// A session asks a question: a pending `AskUserQuestion`, or a presenter's elicitation.
    Question,
    /// A session asks permission to run a tool.
    Permission,
    /// A session finished its turn (`Done`).
    TurnFinished,
    /// A session waits for its next prompt (`WaitingForInput` with no question).
    WaitingForInput,
    /// A session's agent wrote its operator (`tddy-tools notify`).
    AgentNotification,
    /// A session's agent ended (`#activity-wake` 1/3).
    Ended,
}

impl WatchKind {
    /// Every kind, in the order the tools list them.
    pub const ALL: [WatchKind; 6] = [
        WatchKind::Question,
        WatchKind::Permission,
        WatchKind::TurnFinished,
        WatchKind::WaitingForInput,
        WatchKind::AgentNotification,
        WatchKind::Ended,
    ];

    /// The kind's name, as the tools take it.
    pub fn as_str(self) -> &'static str {
        match self {
            WatchKind::Question => "question",
            WatchKind::Permission => "permission",
            WatchKind::TurnFinished => "turn_finished",
            WatchKind::WaitingForInput => "waiting_for_input",
            WatchKind::AgentNotification => "agent_notification",
            WatchKind::Ended => "ended",
        }
    }

    /// The kind named `name`, or `None` for a name no kind has.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }

    /// The kind `record` is, or `None` for a record no watch can ask for — among them an
    /// activity-status record carrying a question, which is the same prompt its
    /// `pending_question` record already announced.
    pub fn of(record: &ProjectNotificationRecord) -> Option<Self> {
        // TODO(session-watches): implement
        let _ = record;
        None
    }
}

/// Whose watch it is: an orchestrator session of a project, and the OS user it runs as.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WatchOwner {
    pub project_id: String,
    pub session_id: String,
    pub os_user: String,
}

/// Which records a watch matches: those of `session_ids`, or — with `started_by_me` — of the
/// sessions its owner started; and of `kinds` only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchFilter {
    pub session_ids: Vec<String>,
    pub started_by_me: bool,
    pub kinds: Vec<WatchKind>,
}

/// One watch, as the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWatch {
    pub watch_id: String,
    pub owner: WatchOwner,
    pub filter: WatchFilter,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    /// The `seq` of the last log record this watch has delivered past.
    pub position: u64,
}

/// One live watch as `ListSessionWatches` shows it: the watch, and how many matched events wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedWatch {
    pub watch: SessionWatch,
    pub waiting: usize,
}

/// Why a watch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchEndReason {
    /// Its time to live passed.
    Expired,
    /// Its orchestrator's agent ended.
    OrchestratorEnded,
    /// Every session it names ended.
    SessionsEnded,
}

/// A watch that ended, reported once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEnd {
    pub watch_id: String,
    pub reason: WatchEndReason,
}

/// What one delivery hands an orchestrator.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchDelivery {
    /// The records its watches matched, oldest first, each once; at most
    /// [`MAX_EVENTS_PER_ANSWER`].
    pub events: Vec<ProjectNotificationRecord>,
    /// Matched records the log evicted before they could be delivered.
    pub lost: u64,
    /// The watches that ended since the last delivery.
    pub ended: Vec<WatchEnd>,
}

impl WatchDelivery {
    /// Whether there is nothing to hand over.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.lost == 0 && self.ended.is_empty()
    }
}

/// The watches of one project, as kept in memory and on disk.
#[derive(Debug, Default)]
// TODO(session-watches): implement — read by every store method; drop the allow.
#[allow(dead_code)]
struct ProjectWatches {
    watches: Vec<SessionWatch>,
}

/// Every project's watches on this host.
pub struct SessionWatchStore {
    data_dir: PathBuf,
    /// One lock per project: every operation on a project's watches holds it, which is what makes a
    /// record delivered once across concurrent calls.
    projects: Mutex<HashMap<String, Arc<tokio::sync::Mutex<ProjectWatches>>>>,
}

impl SessionWatchStore {
    /// The store persisted under `data_dir`, holding what a previous run kept there. An unreadable
    /// watches file is an error.
    pub fn open(data_dir: &Path) -> anyhow::Result<Self> {
        // TODO(session-watches): implement — replay every project's watches file.
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            projects: Mutex::new(HashMap::new()),
        })
    }

    /// A new watch for `owner`, matching `filter` for `ttl`, starting at the log's head — so it
    /// sees only what happens after it. Refused, saying what to do, for a `ttl` of zero or above
    /// [`MAX_WATCH_TTL_SECS`], a filter that selects no session, a filter with no kind, or an owner
    /// already holding [`MAX_LIVE_WATCHES`]. Whether the owner may watch the sessions it names is
    /// the caller's to decide first.
    pub async fn create(
        &self,
        owner: &WatchOwner,
        filter: WatchFilter,
        ttl: Duration,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> Result<SessionWatch, String> {
        // TODO(session-watches): implement
        let _ = (
            owner,
            filter,
            ttl,
            log,
            now_unix_ms,
            self.project_of(&owner.project_id),
        );
        Err(NOT_YET.to_string())
    }

    /// `owner`'s live watches, each with how many matched events wait for it.
    pub async fn list(
        &self,
        owner: &WatchOwner,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> Vec<ListedWatch> {
        // TODO(session-watches): implement
        let _ = (owner, log, now_unix_ms, &self.data_dir);
        Vec::new()
    }

    /// `owner`'s watch `watch_id` with its expiry moved to `ttl` from now. Refused for a watch
    /// that is not `owner`'s, has ended, or a `ttl` out of bounds.
    pub async fn renew(
        &self,
        owner: &WatchOwner,
        watch_id: &str,
        ttl: Duration,
        now_unix_ms: u64,
    ) -> Result<SessionWatch, String> {
        // TODO(session-watches): implement
        let _ = (owner, watch_id, ttl, now_unix_ms);
        Err(NOT_YET.to_string())
    }

    /// End `owner`'s watch `watch_id`: nothing more is delivered for it. Refused for a watch that is
    /// not `owner`'s or has ended.
    pub async fn cancel(&self, owner: &WatchOwner, watch_id: &str) -> Result<(), String> {
        // TODO(session-watches): implement
        let _ = (owner, watch_id);
        Err(NOT_YET.to_string())
    }

    /// What `owner`'s watches matched since the last delivery — each record once, across its
    /// watches and across concurrent calls — with the watches that ended since. Positions advance
    /// past what is handed over, and the change is persisted; nothing waiting is an empty
    /// delivery and no write.
    pub async fn take_delivery(
        &self,
        owner: &WatchOwner,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> WatchDelivery {
        // TODO(session-watches): implement
        let _ = (owner, log, now_unix_ms);
        WatchDelivery::default()
    }

    /// Whether `owner` holds a watch that has not ended at `now_unix_ms`.
    pub async fn has_live_watches(
        &self,
        owner: &WatchOwner,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> bool {
        // TODO(session-watches): implement
        let _ = (owner, log, now_unix_ms);
        false
    }

    /// The project's watches, loaded the first time it is named.
    fn project_of(&self, project_id: &str) -> Arc<tokio::sync::Mutex<ProjectWatches>> {
        let mut projects = self
            .projects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(projects.entry(project_id.to_string()).or_default())
    }
}
