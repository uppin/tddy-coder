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
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tddy_daemon_kernel::session_notification::SessionNotificationSource;

use crate::project_notification_log::{ProjectNotificationLog, ProjectNotificationRecord};

mod stream;

pub use stream::{StreamBatch, StreamRefusal, StreamedEvent, STREAM_LEASE_GRACE};

/// The longest a watch lives without being renewed: 24 hours.
pub const MAX_WATCH_TTL_SECS: u64 = 86_400;
/// How many live watches one orchestrator may hold.
pub const MAX_LIVE_WATCHES: usize = 16;
/// How many matched events one delivery carries; the rest wait for the next.
pub const MAX_EVENTS_PER_ANSWER: usize = 50;

/// What a store keeps per project, under the project's directory.
pub const SESSION_WATCHES_FILE: &str = "session-watches.json";

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
        match record.source {
            SessionNotificationSource::SessionEnded => Some(Self::Ended),
            SessionNotificationSource::Agent => Some(Self::AgentNotification),
            SessionNotificationSource::Presenter => Some(Self::Question),
            SessionNotificationSource::PendingQuestion => {
                match record.question.as_ref().map(|q| q.kind.as_str()) {
                    Some("permission") => Some(Self::Permission),
                    _ => Some(Self::Question),
                }
            }
            SessionNotificationSource::ActivityStatus => {
                if record.question.is_some() {
                    return None;
                }
                match record.activity_status.as_deref() {
                    Some("Done") => Some(Self::TurnFinished),
                    Some("WaitingForInput") => Some(Self::WaitingForInput),
                    _ => None,
                }
            }
            SessionNotificationSource::AgentToolCall => None,
        }
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
    /// The `seq` the log had when the watch was created: its endings are looked for after it,
    /// however far delivery has since advanced `position`.
    #[serde(default)]
    pub created_seq: u64,
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

/// A watch that ended and has not been reported yet, and whose it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PendingEnd {
    owner: WatchOwner,
    end: WatchEnd,
}

/// The watches of one project, as kept in memory and on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProjectWatches {
    #[serde(default)]
    watches: Vec<SessionWatch>,
    #[serde(default)]
    ended: Vec<PendingEnd>,
}

/// Every project's watches on this host.
pub struct SessionWatchStore {
    data_dir: PathBuf,
    /// One lock per project: every operation on a project's watches holds it, which is what makes a
    /// record delivered once across concurrent calls.
    projects: Mutex<HashMap<String, Arc<tokio::sync::Mutex<ProjectWatches>>>>,
    /// Which stream holds which watch, in memory only (`#activity-wake` 4/5): a daemon restart
    /// ends every stream's hold.
    leases: Mutex<stream::StreamLeases>,
    /// How long a watch stays streamed after its stream's last call.
    stream_lease_grace: Duration,
}

impl SessionWatchStore {
    /// The store persisted under `data_dir`, holding what a previous run kept there. An unreadable
    /// watches file is an error.
    pub fn open(data_dir: &Path) -> anyhow::Result<Self> {
        let mut projects = HashMap::new();
        let dir = data_dir.join("projects");
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    data_dir: data_dir.to_path_buf(),
                    projects: Mutex::new(projects),
                    leases: Mutex::default(),
                    stream_lease_grace: STREAM_LEASE_GRACE,
                });
            }
            Err(e) => return Err(anyhow::anyhow!("read {}: {e}", dir.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|e| anyhow::anyhow!("read {}: {e}", dir.display()))?;
            let Ok(project_id) = entry.file_name().into_string() else {
                continue;
            };
            let path = entry.path().join(SESSION_WATCHES_FILE);
            if !path.is_file() {
                continue;
            }
            let body = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
            let stored: ProjectWatches = serde_json::from_str(&body)
                .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
            projects.insert(project_id, Arc::new(tokio::sync::Mutex::new(stored)));
        }
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            projects: Mutex::new(projects),
            leases: Mutex::default(),
            stream_lease_grace: STREAM_LEASE_GRACE,
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
        refuse_a_bad_ttl(ttl)?;
        if filter.session_ids.is_empty() && !filter.started_by_me {
            return Err(
                "name session_ids or started_by_me: a watch must select at least one session"
                    .to_string(),
            );
        }
        if filter.kinds.is_empty() {
            return Err("name at least one kind, or name none for all six".to_string());
        }
        let project = self.project_of(&owner.project_id);
        let mut state = project.lock().await;
        let ended_some = end_finished(&mut state, owner, log, now_unix_ms);
        let live = state
            .watches
            .iter()
            .filter(|watch| &watch.owner == owner)
            .count();
        if live >= MAX_LIVE_WATCHES {
            if ended_some {
                self.persist_best_effort(&owner.project_id, &state);
            }
            return Err(format!(
                "you already hold {MAX_LIVE_WATCHES} live session watches, the most one orchestrator may hold: cancel one with CancelSessionWatch before creating another"
            ));
        }
        let head = log.head_seq(&owner.project_id);
        let watch = SessionWatch {
            watch_id: uuid::Uuid::new_v4().to_string(),
            owner: owner.clone(),
            filter,
            created_at_unix_ms: now_unix_ms,
            expires_at_unix_ms: now_unix_ms
                .saturating_add(u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX)),
            position: head,
            created_seq: head,
        };
        state.watches.push(watch.clone());
        if let Err(e) = self.persist(&owner.project_id, &state) {
            state.watches.retain(|w| w.watch_id != watch.watch_id);
            return Err(format!("the watch was not kept: {e}"));
        }
        Ok(watch)
    }

    /// `owner`'s live watches, each with how many matched events wait for it.
    pub async fn list(
        &self,
        owner: &WatchOwner,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> Vec<ListedWatch> {
        let project = self.project_of(&owner.project_id);
        let mut state = project.lock().await;
        if end_finished(&mut state, owner, log, now_unix_ms) {
            self.persist_best_effort(&owner.project_id, &state);
        }
        state
            .watches
            .iter()
            .filter(|watch| &watch.owner == owner)
            .map(|watch| {
                let (records, _) =
                    log.records_after(&owner.project_id, &owner.os_user, watch.position);
                let waiting = records
                    .iter()
                    .filter(|record| watch_matches(watch, record))
                    .count();
                ListedWatch {
                    watch: watch.clone(),
                    waiting,
                }
            })
            .collect()
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
        refuse_a_bad_ttl(ttl)?;
        let project = self.project_of(&owner.project_id);
        let mut state = project.lock().await;
        if end_expired(&mut state, owner, now_unix_ms) {
            self.persist_best_effort(&owner.project_id, &state);
        }
        let Some(index) = state
            .watches
            .iter()
            .position(|watch| &watch.owner == owner && watch.watch_id == watch_id)
        else {
            return Err(format!(
                "no live watch {watch_id} of yours: list your watches with ListSessionWatches"
            ));
        };
        state.watches[index].expires_at_unix_ms =
            now_unix_ms.saturating_add(u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX));
        let renewed = state.watches[index].clone();
        self.persist(&owner.project_id, &state)
            .map_err(|e| format!("the renewal was not kept: {e}"))?;
        Ok(renewed)
    }

    /// End `owner`'s watch `watch_id`: nothing more is delivered for it. Refused for a watch that is
    /// not `owner`'s or has ended.
    pub async fn cancel(&self, owner: &WatchOwner, watch_id: &str) -> Result<(), String> {
        let project = self.project_of(&owner.project_id);
        let mut state = project.lock().await;
        let Some(index) = state
            .watches
            .iter()
            .position(|watch| &watch.owner == owner && watch.watch_id == watch_id)
        else {
            return Err(format!(
                "no live watch {watch_id} of yours: list your watches with ListSessionWatches"
            ));
        };
        state.watches.remove(index);
        self.persist(&owner.project_id, &state)
            .map_err(|e| format!("the cancellation was not kept: {e}"))?;
        Ok(())
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
        let project = self.project_of(&owner.project_id);
        let mut state = project.lock().await;
        let streamed = self.streamed_watch_ids(owner, now_unix_ms);
        let delivery = apply_delivery(&mut state, owner, log, now_unix_ms, &streamed);
        if !delivery.is_empty() {
            self.persist_best_effort(&owner.project_id, &state);
        }
        delivery
    }

    /// Hand `owner`'s waiting delivery to `deliver`, and advance past it only if `deliver`
    /// succeeds (`#activity-wake` 3/3): a wake whose paste failed delivers nothing, and what was
    /// waiting waits on. `Ok(true)` when something was delivered, `Ok(false)` when nothing was
    /// waiting (and `deliver` was not called); `Err` with `deliver`'s refusal, nothing moved.
    pub async fn deliver_with<F, Fut>(
        &self,
        owner: &WatchOwner,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
        deliver: F,
    ) -> Result<bool, String>
    where
        F: FnOnce(WatchDelivery) -> Fut + Send,
        Fut: std::future::Future<Output = Result<(), String>> + Send,
    {
        let project = self.project_of(&owner.project_id);
        let mut state = project.lock().await;
        let before = state.clone();
        let streamed = self.streamed_watch_ids(owner, now_unix_ms);
        let delivery = apply_delivery(&mut state, owner, log, now_unix_ms, &streamed);
        if delivery.is_empty() {
            return Ok(false);
        }
        match deliver(delivery).await {
            Ok(()) => {
                self.persist_best_effort(&owner.project_id, &state);
                Ok(true)
            }
            Err(e) => {
                *state = before;
                log::warn!(
                    target: "tddy_session_activity::session_watches",
                    "a delivery to {} was refused, so nothing was delivered: {e}",
                    owner.session_id
                );
                Err(e)
            }
        }
    }

    /// Whether `owner` holds a watch that has not ended at `now_unix_ms`.
    pub async fn has_live_watches(
        &self,
        owner: &WatchOwner,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> bool {
        let project = self.project_of(&owner.project_id);
        let mut state = project.lock().await;
        if end_finished(&mut state, owner, log, now_unix_ms) {
            self.persist_best_effort(&owner.project_id, &state);
        }
        state.watches.iter().any(|watch| &watch.owner == owner)
    }

    /// The project's watches, loaded the first time it is named.
    fn project_of(&self, project_id: &str) -> Arc<tokio::sync::Mutex<ProjectWatches>> {
        let mut projects = self
            .projects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(projects.entry(project_id.to_string()).or_default())
    }

    /// Rewrite `project_id`'s watches file whole, or the reason it could not be written.
    fn persist(&self, project_id: &str, state: &ProjectWatches) -> anyhow::Result<()> {
        let path = watches_file(&self.data_dir, project_id)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
        }
        let body = serde_json::to_string(state)
            .map_err(|e| anyhow::anyhow!("serialize the session watches: {e}"))?;
        let temporary = path.with_file_name(format!("{SESSION_WATCHES_FILE}.tmp"));
        let mut file = std::fs::File::create(&temporary)
            .map_err(|e| anyhow::anyhow!("create {}: {e}", temporary.display()))?;
        use std::io::Write as _;
        file.write_all(body.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| anyhow::anyhow!("write {}: {e}", temporary.display()))?;
        std::fs::rename(&temporary, &path)
            .map_err(|e| anyhow::anyhow!("replace {}: {e}", path.display()))?;
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|e| anyhow::anyhow!("sync {}: {e}", parent.display()))?;
        }
        Ok(())
    }

    /// [`Self::persist`], logging a failure: the in-memory watches still stand, and the next
    /// change rewrites the whole file.
    fn persist_best_effort(&self, project_id: &str, state: &ProjectWatches) {
        if let Err(e) = self.persist(project_id, state) {
            log::warn!(
                target: "tddy_session_activity::session_watches",
                "could not persist the session watches of {project_id}: {e:#}"
            );
        }
    }
}

/// Where `project_id`'s watches are kept under `data_dir`. An id that cannot name a directory is
/// refused rather than joined onto the path.
fn watches_file(data_dir: &Path, project_id: &str) -> anyhow::Result<PathBuf> {
    let names_a_directory = !project_id.is_empty()
        && project_id != "."
        && project_id != ".."
        && !project_id.contains(['/', '\\', '\0']);
    anyhow::ensure!(
        names_a_directory,
        "project id {project_id:?} cannot name a directory"
    );
    Ok(data_dir
        .join("projects")
        .join(project_id)
        .join(SESSION_WATCHES_FILE))
}

/// The refusal of a `ttl` of zero or above [`MAX_WATCH_TTL_SECS`], saying what to do instead.
fn refuse_a_bad_ttl(ttl: Duration) -> Result<(), String> {
    if ttl.is_zero() || ttl > Duration::from_secs(MAX_WATCH_TTL_SECS) {
        return Err(format!(
            "ttl_seconds must be between 1 and {MAX_WATCH_TTL_SECS} (24 hours): give CreateSessionWatch or RenewSessionWatch a ttl_seconds in that range"
        ));
    }
    Ok(())
}

/// Whether `watch` matches `record`: its session is named, or it started the session, the record
/// is of a kind it asks for, and it is never the owner's own session. The record's project and OS
/// user are the log read's, which is the owner's.
fn watch_matches(watch: &SessionWatch, record: &ProjectNotificationRecord) -> bool {
    if record.session_id == watch.owner.session_id {
        return false;
    }
    let selected = watch
        .filter
        .session_ids
        .iter()
        .any(|id| id == &record.session_id)
        || (watch.filter.started_by_me
            && record.orchestrator_session_id.as_deref() == Some(watch.owner.session_id.as_str()));
    selected && WatchKind::of(record).is_some_and(|kind| watch.filter.kinds.contains(&kind))
}

/// Why `watch` has ended at `now_unix_ms`, or `None` while it lives.
fn end_reason(
    watch: &SessionWatch,
    log: &ProjectNotificationLog,
    now_unix_ms: u64,
) -> Option<WatchEndReason> {
    if now_unix_ms >= watch.expires_at_unix_ms {
        return Some(WatchEndReason::Expired);
    }
    let (records, _) = log.records_after(
        &watch.owner.project_id,
        &watch.owner.os_user,
        watch.created_seq,
    );
    let ended: HashSet<&str> = records
        .iter()
        .filter(|r| r.source == SessionNotificationSource::SessionEnded)
        .map(|r| r.session_id.as_str())
        .collect();
    if ended.contains(watch.owner.session_id.as_str()) {
        return Some(WatchEndReason::OrchestratorEnded);
    }
    if !watch.filter.started_by_me
        && !watch.filter.session_ids.is_empty()
        && watch
            .filter
            .session_ids
            .iter()
            .all(|id| ended.contains(id.as_str()))
    {
        return Some(WatchEndReason::SessionsEnded);
    }
    None
}

/// Compute `owner`'s delivery from `state` and advance it past what the delivery hands over:
/// positions move past the delivered records (and past what the log evicted), and the watches
/// that ended are taken to be reported. Shared by `take_delivery`, which commits at once, and
/// `deliver_with`, which rolls `state` back when its delivery is refused.
fn apply_delivery(
    state: &mut ProjectWatches,
    owner: &WatchOwner,
    log: &ProjectNotificationLog,
    now_unix_ms: u64,
    streamed: &HashSet<String>,
) -> WatchDelivery {
    end_finished(state, owner, log, now_unix_ms);
    // A watch a stream's lease holds is skipped entirely: its events are the stream's alone,
    // and its position is the stream's to advance.
    let mut matched: BTreeMap<u64, ProjectNotificationRecord> = BTreeMap::new();
    let mut lost = 0_u64;
    let mut scans: Vec<(String, u64, u64)> = Vec::new();
    for watch in state
        .watches
        .iter()
        .filter(|watch| &watch.owner == owner && !streamed.contains(&watch.watch_id))
    {
        let (records, watch_lost) =
            log.records_after(&owner.project_id, &owner.os_user, watch.position);
        lost = lost.max(watch_lost);
        for record in records.iter().filter(|r| watch_matches(watch, r)) {
            matched.entry(record.seq).or_insert_with(|| record.clone());
        }
        scans.push((watch.watch_id.clone(), watch.position, watch_lost));
    }
    let events: Vec<ProjectNotificationRecord> = matched
        .values()
        .take(MAX_EVENTS_PER_ANSWER)
        .cloned()
        .collect();
    let delivered: HashSet<u64> = events.iter().map(|r| r.seq).collect();
    for watch in state
        .watches
        .iter_mut()
        .filter(|watch| &watch.owner == owner && !streamed.contains(&watch.watch_id))
    {
        let mut position = watch.position;
        if let Some((_, _, watch_lost)) = scans.iter().find(|(id, _, _)| id == &watch.watch_id) {
            position = position.max(watch.position.saturating_add(*watch_lost));
        }
        for record in events
            .iter()
            .filter(|r| delivered.contains(&r.seq) && watch_matches(watch, r))
        {
            position = position.max(record.seq);
        }
        watch.position = position;
    }
    let mut ended = Vec::new();
    state.ended.retain(|pending| {
        if &pending.owner == owner && !streamed.contains(&pending.end.watch_id) {
            ended.push(pending.end.clone());
            false
        } else {
            true
        }
    });
    WatchDelivery {
        events,
        lost,
        ended,
    }
}

/// Move `owner`'s watches that have ended out of the live set, to be reported once by its next
/// delivery. Whether any moved.
fn end_finished(
    state: &mut ProjectWatches,
    owner: &WatchOwner,
    log: &ProjectNotificationLog,
    now_unix_ms: u64,
) -> bool {
    let mut kept = Vec::with_capacity(state.watches.len());
    let mut moved = false;
    for watch in state.watches.drain(..) {
        if &watch.owner == owner {
            if let Some(reason) = end_reason(&watch, log, now_unix_ms) {
                state.ended.push(PendingEnd {
                    owner: watch.owner.clone(),
                    end: WatchEnd {
                        watch_id: watch.watch_id.clone(),
                        reason,
                    },
                });
                moved = true;
                continue;
            }
        }
        kept.push(watch);
    }
    state.watches = kept;
    moved
}

/// [`end_finished`] for expiry alone, for the operations handed no log.
fn end_expired(state: &mut ProjectWatches, owner: &WatchOwner, now_unix_ms: u64) -> bool {
    let mut kept = Vec::with_capacity(state.watches.len());
    let mut moved = false;
    for watch in state.watches.drain(..) {
        if &watch.owner == owner && now_unix_ms >= watch.expires_at_unix_ms {
            state.ended.push(PendingEnd {
                owner: watch.owner.clone(),
                end: WatchEnd {
                    watch_id: watch.watch_id.clone(),
                    reason: WatchEndReason::Expired,
                },
            });
            moved = true;
            continue;
        }
        kept.push(watch);
    }
    state.watches = kept;
    moved
}
