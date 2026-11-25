//! Relaying this host's sessions' notification-log records to the connected hosts they concern
//! (n5, cross-host watches and messages).
//!
//! A host's project notification log is the source of truth for its own sessions. Some of its
//! records concern a session held on another host:
//!
//! - every record of a session whose **orchestrator runs on another host** — its changeset names
//!   that host (`orchestrator_daemon_instance_id`), so the orchestrator's `started_by_me` watch
//!   sees its remote children;
//! - a **message addressed to a session on another host** ([`SessionMessage::to_daemon_instance_id`]),
//!   so the recipient's inbox gets it;
//! - every record of a session a **watch on another host follows** by name ([`FollowRequest`]).
//!
//! [`NotificationRelay`] pushes those records to each such host over the transport its
//! [`RelayPorts`] provide (the daemon's existing peer forward), where they are appended to that
//! host's own log ([`ProjectNotificationLog::append_relayed`]). The outbox *is* the log: a record
//! leaves it only when the window evicts it, so while a host is unreachable nothing is dropped —
//! the relay keeps a cursor per (project, host), persisted, advanced only when the host
//! acknowledges, and retries. A cursor the window has passed is reported in the next batch as
//! `lost`, never silently skipped. Only this host's own records are relayed: a relayed record never
//! travels a second hop.
//!
//! The wire form ([`RelayRequest`], [`FollowRequest`]) rides `ExecuteTool` under two internal names
//! ([`RELAY_SESSION_NOTIFICATIONS`], [`FOLLOW_SESSION_NOTIFICATIONS`]) that no tool listing offers
//! and that a daemon serves only to a peer over the common room.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions-on-other-hosts

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tddy_core::session_lifecycle::unified_session_dir_path;

use crate::project_notification_log::{HeldRecord, ProjectNotificationLog};

/// The internal `ExecuteTool` name a relay batch rides under. Never a tool: no listing offers it,
/// and a daemon serves it only to a peer over the common room.
pub const RELAY_SESSION_NOTIFICATIONS: &str = "tddy.relay.SessionNotifications";
/// The internal `ExecuteTool` name a follow registration rides under; as
/// [`RELAY_SESSION_NOTIFICATIONS`].
pub const FOLLOW_SESSION_NOTIFICATIONS: &str = "tddy.relay.FollowSession";

/// Whether `tool_name` is one of the relay's internal names.
pub fn is_relay_tool(tool_name: &str) -> bool {
    tool_name == RELAY_SESSION_NOTIFICATIONS || tool_name == FOLLOW_SESSION_NOTIFICATIONS
}

/// The most records one batch carries.
pub const MAX_RECORDS_PER_BATCH: usize = 100;

/// One relayed record: the record as its own host persists it, and the session held on the
/// receiving host it is relayed for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelayedRecord {
    /// The record in the log's own JSON form ([`crate::project_notification_log::record_json`]).
    pub record_json: String,
    /// The orchestrator that started the record's session, the recipient of its message, or the
    /// owner of the watch that follows it.
    pub anchor_session_id: String,
}

/// A batch of one project's records, for one user, relayed to one host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelayRequest {
    /// The sending host: where the records' sessions run.
    pub origin_daemon_instance_id: String,
    pub project_id: String,
    /// The GitHub login the records' OS user maps to on the sender; the receiver maps it to its own.
    pub github_user: String,
    /// Oldest first, each with its `seq` on the origin.
    pub records: Vec<RelayedRecord>,
    /// How many of the origin's records for this receiver were evicted before they were relayed.
    pub lost: u64,
    /// The highest origin `seq` the batch covers.
    pub through_seq: u64,
}

/// A receiver's acknowledgement of a [`RelayRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayAck {
    /// The highest origin `seq` the receiver holds of the batch's origin and project.
    pub received_through_seq: u64,
}

/// A watch on another host follows one of this host's sessions by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FollowRequest {
    /// The host whose watch follows the session.
    pub follower_daemon_instance_id: String,
    pub project_id: String,
    pub session_id: String,
    /// The follower's session that holds the watch.
    pub watcher_session_id: String,
    pub until_unix_ms: u64,
    pub github_user: String,
}

/// What the relay needs from the daemon: who this host is, whose sessions are where, and the
/// transport to the other hosts.
#[async_trait]
pub trait RelayPorts: Send + Sync {
    /// This host, as the eligible-daemon roster names it.
    fn local_instance_id(&self) -> String;
    /// The GitHub login `os_user` maps to on this host.
    fn github_user_of(&self, os_user: &str) -> Option<String>;
    /// Where `os_user`'s sessions live on this host.
    fn sessions_base_for(&self, os_user: &str) -> Option<PathBuf>;
    /// Hand `request` to `host`; its acknowledgement, or why it was not taken.
    async fn relay(&self, host: &str, request: &RelayRequest) -> Result<RelayAck, String>;
}

/// One session another host's watch follows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Interest {
    project_id: String,
    session_id: String,
    follower: String,
    watcher_session_id: String,
    until_unix_ms: u64,
    /// The log's head when the follow was registered: only later records are relayed for it.
    from_seq: u64,
}

/// How far one project's records have been taken by one host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Cursor {
    project_id: String,
    host: String,
    through_seq: u64,
}

/// What the relay persists.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    cursors: Vec<Cursor>,
    #[serde(default)]
    interests: Vec<Interest>,
}

/// The relay's file under its state directory.
pub const RELAY_STATE_FILE: &str = "notification-relay.json";

#[derive(Default)]
struct RelayState {
    /// `(project, host)` → the highest `seq` that host has taken.
    cursors: BTreeMap<(String, String), u64>,
    interests: Vec<Interest>,
}

/// A destination of one record: the host, and the session there it is relayed for.
type Destination = (String, String);

/// The longest one relay call may take: a host that does not answer holds up the others no longer.
pub const RELAY_CALL_DEADLINE: Duration = Duration::from_secs(10);

/// How long a session found to have no orchestrator on another host is believed so before its
/// changeset is read again — an orchestrator link written after the session's first record is
/// still found.
const NO_ORCHESTRATOR_ELSEWHERE_FOR: Duration = Duration::from_secs(30);

/// Where one record goes, and whose it is.
#[derive(Debug, Clone)]
struct Route {
    os_user: String,
    destinations: Vec<Destination>,
}

impl Route {
    fn names(&self, host: &str) -> bool {
        self.destinations.iter().any(|(h, _)| h == host)
    }
}

/// The window one pass read: the `seq` of its oldest and newest records.
#[derive(Debug, Clone, Copy)]
struct Window {
    oldest_kept: Option<u64>,
    newest: Option<u64>,
}

/// `batch`'s records in the log's own JSON, each with the session it is relayed for.
fn relayed_records(batch: &[&(HeldRecord, String)]) -> Result<Vec<RelayedRecord>, String> {
    batch
        .iter()
        .map(|(held, anchor)| {
            crate::project_notification_log::record_json(&held.record)
                .map(|json| RelayedRecord {
                    record_json: json.to_string(),
                    anchor_session_id: anchor.clone(),
                })
                .map_err(|e| format!("serialize a record: {e}"))
        })
        .collect()
}

/// The relay of one daemon.
pub struct NotificationRelay {
    log: Arc<ProjectNotificationLog>,
    ports: Arc<dyn RelayPorts>,
    state_file: PathBuf,
    state: Mutex<RelayState>,
    /// Per session, the other host its orchestrator runs on and the orchestrator — read once from
    /// its changeset, which a start writes before the session's agent runs.
    orchestrators: Mutex<HashMap<(String, String), (Option<Destination>, std::time::Instant)>>,
    /// Per project, the destinations of each of its records the relay was told of as it was
    /// appended ([`ProjectNotificationLog::on_append`]), by `seq`: what a record evicted before it was
    /// relayed was owed to, so `lost` counts exactly those. An entry goes once every host it names
    /// has taken it.
    routes: Mutex<HashMap<String, BTreeMap<u64, Route>>>,
}

/// What one pass over the logs left.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PassOutcome {
    /// Some host did not take what it was sent: try again after the retry delay.
    failed: bool,
    /// Some batch was taken and more may wait: pass again at once.
    progressed: bool,
}

impl NotificationRelay {
    /// The relay of `log`'s records over `ports`, its state kept under `state_dir`. An unreadable
    /// state file is an error: the relay does not start over cursors it could not read.
    pub fn open(
        log: Arc<ProjectNotificationLog>,
        ports: Arc<dyn RelayPorts>,
        state_dir: &Path,
    ) -> anyhow::Result<Self> {
        let state_file = state_dir.join(RELAY_STATE_FILE);
        let persisted: PersistedState = match std::fs::read_to_string(&state_file) {
            Ok(body) => serde_json::from_str(&body)
                .map_err(|e| anyhow::anyhow!("read {}: {e}", state_file.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PersistedState::default(),
            Err(e) => return Err(anyhow::anyhow!("read {}: {e}", state_file.display())),
        };
        let state = RelayState {
            cursors: persisted
                .cursors
                .into_iter()
                .map(|c| ((c.project_id, c.host), c.through_seq))
                .collect(),
            interests: persisted.interests,
        };
        Ok(Self {
            log,
            ports,
            state_file,
            state: Mutex::new(state),
            orchestrators: Mutex::new(HashMap::new()),
            routes: Mutex::new(HashMap::new()),
        })
    }

    /// Relay until the log is dropped: a pass whenever a record is appended, again at once while
    /// batches are taken, and after `retry` while a host does not take its batch.
    pub async fn run(self: Arc<Self>, retry: Duration) {
        let tapped = Arc::clone(&self);
        if let Err(e) = self
            .log
            .on_append(Arc::new(move |project_id: &str, held: &HeldRecord| {
                tapped.note(project_id, held);
            }))
        {
            log::error!(
                target: "tddy_session_activity::notification_relay",
                "the notification relay cannot route records as they are appended: {e}"
            );
        }
        // What the window already holds was appended before the relay was told of appends.
        for project_id in self.log.project_ids() {
            for held in self.log.held_records(&project_id).0 {
                self.note(&project_id, &held);
            }
        }
        let appended = self.log.appended();
        loop {
            // Registered before the pass, so a record appended during it wakes the next one.
            let woken = appended.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            let outcome = self.pass().await;
            if outcome.progressed {
                continue;
            }
            if outcome.failed {
                let _ = tokio::time::timeout(retry, woken).await;
            } else {
                woken.await;
            }
        }
    }

    /// Register `request`: `request.session_id`'s records from now on are relayed to its follower
    /// until it expires. A second registration of the same watcher and session renews it.
    pub fn follow(&self, request: &FollowRequest) {
        let from_seq = self.log.head_seq(&request.project_id);
        let now = tddy_daemon_kernel::now_unix_ms();
        {
            let mut state = self.lock_state();
            state
                .interests
                .retain(|interest| interest.until_unix_ms > now);
            if let Some(existing) = state.interests.iter_mut().find(|interest| {
                interest.project_id == request.project_id
                    && interest.session_id == request.session_id
                    && interest.follower == request.follower_daemon_instance_id
                    && interest.watcher_session_id == request.watcher_session_id
            }) {
                existing.until_unix_ms = existing.until_unix_ms.max(request.until_unix_ms);
            } else {
                state.interests.push(Interest {
                    project_id: request.project_id.clone(),
                    session_id: request.session_id.clone(),
                    follower: request.follower_daemon_instance_id.clone(),
                    watcher_session_id: request.watcher_session_id.clone(),
                    until_unix_ms: request.until_unix_ms,
                    from_seq,
                });
            }
        }
        self.persist_best_effort();
        // Nothing waits on a registration; the next appended record is relayed under it.
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, RelayState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Note where `held`, just appended to `project_id`, must go.
    fn note(&self, project_id: &str, held: &HeldRecord) {
        let destinations = self.destinations_of(project_id, held);
        if destinations.is_empty() {
            return;
        }
        self.lock_routes()
            .entry(project_id.to_string())
            .or_default()
            .insert(
                held.record.seq,
                Route {
                    os_user: held.os_user.clone(),
                    destinations,
                },
            );
    }

    fn lock_routes(&self) -> std::sync::MutexGuard<'_, HashMap<String, BTreeMap<u64, Route>>> {
        self.routes.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// One pass: every project's pending records, batched per host.
    async fn pass(&self) -> PassOutcome {
        let mut outcome = PassOutcome::default();
        for project_id in self.log.project_ids() {
            let (held, oldest_kept) = self.log.held_records(&project_id);
            let window = Window {
                oldest_kept,
                newest: held.last().map(|held| held.record.seq),
            };
            let by_host = self.route(&project_id, held);
            for (host, routed) in by_host.iter() {
                match self
                    .send_next_batch(&project_id, host, routed, window)
                    .await
                {
                    Ok(true) => outcome.progressed = true,
                    Ok(false) => {}
                    Err(reason) => {
                        log::info!(
                            target: "tddy_session_activity::notification_relay",
                            "host {host} did not take project {project_id}'s relayed records: {reason}"
                        );
                        outcome.failed = true;
                    }
                }
            }
            self.forget_taken_routes(&project_id);
        }
        outcome
    }

    /// The window's records grouped by the other host each was routed to, with the session there;
    /// and every host still owed an evicted record, with nothing to send.
    fn route(
        &self,
        project_id: &str,
        held: Vec<HeldRecord>,
    ) -> BTreeMap<String, Vec<(HeldRecord, String)>> {
        let routes = self.lock_routes();
        let Some(routes) = routes.get(project_id) else {
            return BTreeMap::new();
        };
        let mut by_host: BTreeMap<String, Vec<(HeldRecord, String)>> = BTreeMap::new();
        for route in routes.values() {
            for (host, _) in &route.destinations {
                by_host.entry(host.clone()).or_default();
            }
        }
        for record in held {
            let Some(route) = routes.get(&record.record.seq) else {
                continue;
            };
            for (host, anchor) in &route.destinations {
                by_host
                    .entry(host.clone())
                    .or_default()
                    .push((record.clone(), anchor.clone()));
            }
        }
        by_host
    }

    /// The records routed to `host` past `cursor` that the window evicted before they were sent:
    /// how many, and the OS user of the first.
    fn evicted_unsent(
        &self,
        project_id: &str,
        host: &str,
        cursor: u64,
        window: Window,
    ) -> (u64, Option<String>) {
        // An empty window evicted everything it ever held.
        let below = window.oldest_kept.unwrap_or(u64::MAX);
        self.lock_routes()
            .get(project_id)
            .map_or((0, None), |routes| {
                let from = cursor.saturating_add(1);
                if from >= below {
                    return (0, None);
                }
                let evicted: Vec<&Route> = routes
                    .range(from..below)
                    .map(|(_, route)| route)
                    .filter(|route| route.names(host))
                    .collect();
                (
                    evicted.len() as u64,
                    evicted.first().map(|route| route.os_user.clone()),
                )
            })
    }

    /// Drop the routes every host they name has taken.
    fn forget_taken_routes(&self, project_id: &str) {
        let cursors = self.lock_state().cursors.clone();
        if let Some(routes) = self.lock_routes().get_mut(project_id) {
            routes.retain(|seq, route| {
                route.destinations.iter().any(|(host, _)| {
                    cursors
                        .get(&(project_id.to_string(), host.clone()))
                        .is_none_or(|cursor| cursor < seq)
                })
            });
        }
    }

    /// The cursor of `key`, created at the first of `routed` for a host first concerned now.
    fn cursor_of(&self, key: &(String, String), routed: &[(HeldRecord, String)]) -> u64 {
        let mut state = self.lock_state();
        if let Some(cursor) = state.cursors.get(key) {
            return *cursor;
        }
        let (project_id, host) = key;
        let first_routed = self.lock_routes().get(project_id).and_then(|routes| {
            routes
                .iter()
                .find(|(_, route)| route.names(host))
                .map(|(seq, _)| *seq)
        });
        let start = first_routed
            .or_else(|| routed.first().map(|(held, _)| held.record.seq))
            .map_or(0, |seq| seq.saturating_sub(1));
        state.cursors.insert(key.clone(), start);
        drop(state);
        self.persist_best_effort();
        start
    }

    /// Move `key`'s cursor to `through`, never back; persisted when `persist`.
    fn advance(&self, key: (String, String), through: u64, persist: bool) {
        {
            let mut state = self.lock_state();
            let at = state.cursors.entry(key).or_insert(0);
            *at = (*at).max(through);
        }
        if persist {
            self.persist_best_effort();
        }
    }

    /// Send `host` the next batch of `routed` past its cursor; `Ok(true)` when one was taken (or
    /// skipped for good), so another may follow at once.
    async fn send_next_batch(
        &self,
        project_id: &str,
        host: &str,
        routed: &[(HeldRecord, String)],
        window: Window,
    ) -> Result<bool, String> {
        let key = (project_id.to_string(), host.to_string());
        let cursor = self.cursor_of(&key, routed);
        let pending: Vec<&(HeldRecord, String)> = routed
            .iter()
            .filter(|(held, _)| held.record.seq > cursor)
            .collect();
        let (lost, lost_os_user) = self.evicted_unsent(project_id, host, cursor, window);
        let Some((first, _)) = pending.first() else {
            if let (true, Some(os_user)) = (lost > 0, lost_os_user) {
                // Owed only records the window has evicted: say so, and move past them.
                return self
                    .report_only_lost(key, host, &os_user, lost, window)
                    .await
                    .map(|()| true);
            }
            return Ok(false);
        };
        let os_user = first.os_user.clone();
        let batch: Vec<&(HeldRecord, String)> = pending
            .iter()
            .copied()
            .take_while(|(held, _)| held.os_user == os_user)
            .take(MAX_RECORDS_PER_BATCH)
            .collect();
        let through_seq = batch.last().map_or(cursor, |(held, _)| held.record.seq);
        let Some(github_user) = self.ports.github_user_of(&os_user) else {
            // Nobody on any host can be this user's: there is no one to deliver these to.
            log::warn!(
                target: "tddy_session_activity::notification_relay",
                "OS user {os_user} maps to no GitHub login here; {} of its records for host {host} are not relayed",
                batch.len()
            );
            self.advance(key, through_seq, true);
            return Ok(true);
        };
        let request = RelayRequest {
            origin_daemon_instance_id: self.ports.local_instance_id(),
            project_id: project_id.to_string(),
            github_user,
            records: relayed_records(&batch)?,
            // What the window evicted, owed to this host, before it was relayed: counted, never
            // skipped silently.
            lost,
            through_seq,
        };
        tokio::time::timeout(RELAY_CALL_DEADLINE, self.ports.relay(host, &request))
            .await
            .map_err(|_| format!("host {host} did not answer within {RELAY_CALL_DEADLINE:?}"))??;
        self.advance(key, through_seq, true);
        Ok(true)
    }

    /// Tell `host` that `lost` of its records were evicted before they could be relayed, when it is
    /// owed nothing the window still holds.
    async fn report_only_lost(
        &self,
        key: (String, String),
        host: &str,
        os_user: &str,
        lost: u64,
        window: Window,
    ) -> Result<(), String> {
        let through_seq = match window.oldest_kept {
            Some(oldest) => oldest.saturating_sub(1),
            None => window.newest.unwrap_or(0),
        };
        let Some(github_user) = self.ports.github_user_of(os_user) else {
            self.advance(key, through_seq, true);
            return Ok(());
        };
        let request = RelayRequest {
            origin_daemon_instance_id: self.ports.local_instance_id(),
            project_id: key.0.clone(),
            github_user,
            records: Vec::new(),
            lost,
            through_seq,
        };
        tokio::time::timeout(RELAY_CALL_DEADLINE, self.ports.relay(host, &request))
            .await
            .map_err(|_| format!("host {host} did not answer within {RELAY_CALL_DEADLINE:?}"))??;
        self.advance(key, through_seq, true);
        Ok(())
    }

    /// The other hosts `record` concerns, each with the session there it is relayed for. None for
    /// a record another host relayed here, or a relay gap.
    fn destinations_of(&self, project_id: &str, held: &HeldRecord) -> Vec<Destination> {
        let record = &held.record;
        if record.daemon_instance_id.is_some() || record.relay_lost > 0 {
            return Vec::new();
        }
        let local = self.ports.local_instance_id();
        let mut destinations: Vec<Destination> = Vec::new();
        let mut add = |host: &str, anchor: &str| {
            if host.is_empty() || host == local || anchor.is_empty() {
                return;
            }
            if !destinations.iter().any(|(h, _)| h == host) {
                destinations.push((host.to_string(), anchor.to_string()));
            }
        };
        if let Some((host, orchestrator)) =
            self.orchestrator_elsewhere(&held.os_user, &record.session_id)
        {
            add(&host, &orchestrator);
        }
        if let Some(message) = &record.message {
            if let Some(host) = &message.to_daemon_instance_id {
                add(host, &message.to_session_id);
            }
        }
        let now = tddy_daemon_kernel::now_unix_ms();
        let state = self.lock_state();
        for interest in state.interests.iter().filter(|interest| {
            interest.project_id == project_id
                && interest.session_id == record.session_id
                && interest.until_unix_ms > now
                && record.seq > interest.from_seq
        }) {
            add(&interest.follower, &interest.watcher_session_id);
        }
        destinations
    }

    /// The other host `session_id`'s orchestrator runs on, and the orchestrator, from its changeset;
    /// `None` for a session started here, or by nobody.
    fn orchestrator_elsewhere(&self, os_user: &str, session_id: &str) -> Option<Destination> {
        let key = (os_user.to_string(), session_id.to_string());
        if let Some((known, read_at)) = self
            .orchestrators
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            // A link, once read, does not change; its absence is read again after a while.
            if known.is_some() || read_at.elapsed() < NO_ORCHESTRATOR_ELSEWHERE_FOR {
                return known.clone();
            }
        }
        let found = self.ports.sessions_base_for(os_user).and_then(|base| {
            let changeset =
                tddy_core::read_changeset(&unified_session_dir_path(&base, session_id)).ok()?;
            let host = changeset
                .orchestrator_daemon_instance_id?
                .trim()
                .to_string();
            let orchestrator = changeset.orchestrator_session_id?.trim().to_string();
            (!host.is_empty() && !orchestrator.is_empty()).then_some((host, orchestrator))
        });
        self.orchestrators
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, (found.clone(), std::time::Instant::now()));
        found
    }

    fn persist_best_effort(&self) {
        let persisted = {
            let state = self.lock_state();
            PersistedState {
                cursors: state
                    .cursors
                    .iter()
                    .map(|((project_id, host), through_seq)| Cursor {
                        project_id: project_id.clone(),
                        host: host.clone(),
                        through_seq: *through_seq,
                    })
                    .collect(),
                interests: state.interests.clone(),
            }
        };
        if let Err(e) = write_atomically(&self.state_file, &persisted) {
            log::warn!(
                target: "tddy_session_activity::notification_relay",
                "could not persist the relay's state: {e:#}"
            );
        }
    }
}

fn write_atomically(path: &Path, state: &PersistedState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
    }
    let body = serde_json::to_string(state)?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, body)
        .map_err(|e| anyhow::anyhow!("write {}: {e}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .map_err(|e| anyhow::anyhow!("replace {}: {e}", path.display()))
}
