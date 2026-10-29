//! A project's durable log of attention notifications, addressed by cursor (`#home-inbox` 7/8).
//!
//! The notification bus is live-only: right for the drawer, but it loses everything a project-home
//! agent was not connected for. [`ProjectNotificationLog`] is a [`SessionNotificationSubscriber`]
//! that appends every [`SessionNotificationKind::AttentionRequired`] notification of a project's
//! sessions to that project's log on this daemon, where `WaitProjectNotifications` and
//! `ReadProjectNotifications` read it.
//!
//! - **Records.** Each carries a monotonic `seq`, starting at 1 and never reused, and the
//!   notification's own fields (the agent's words, the question's id and content). The project is
//!   resolved from the session's metadata. A session of no project is skipped quietly; one whose
//!   owner or record cannot be resolved is a delivery error the bus logs.
//! - **Window.** The newest [`PROJECT_NOTIFICATION_CAPACITY`] records are kept per project. A cursor
//!   older than the window is reported as `lost`, never silently trimmed.
//! - **Persistence.** Appended as JSON lines to
//!   `<data dir>/projects/<project_id>/notifications.jsonl` and replayed on [`ProjectNotificationLog::open`],
//!   so a cursor survives a daemon restart and `seq` continues where it stopped. The file is rewritten
//!   to the window once it holds twice the window's lines, so it stays bounded.
//! - **Cursor.** The decimal `seq` of the last record a page returned. An empty cursor means "from
//!   now" for a wait and "the newest page" for a read. Paging is `#start-parity` 6/9's
//!   ([`FollowPage`]), with one addition the wait needs: **the `next_cursor` of an empty page is
//!   the log's newest `seq`** (`"0"` for a log that never held a record), not the empty cursor it
//!   was given, so polling from it can neither repeat nor skip a record published in between.
//! - **Scope.** A read or a wait is for one project and one operating-system user, and excludes one
//!   session — the caller's own ([`ProjectNotificationScope`]).
//!
//! Feature doc: docs/ft/daemon/session-notifications.md, docs/ft/daemon/project-concept.md.

mod storage;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Notify;
use tokio::time::Instant;

use tddy_core::session_lifecycle::{unified_session_dir_path, validate_session_id_segment};
use tddy_daemon_kernel::session_notification::{
    AgentNotification, QuestionNotice, SessionNotificationKind, SessionNotificationSource,
};
use tddy_tool_engine::project_home_inbox_tools::MAX_WAIT_MS;
use tddy_tool_engine::session_follow::{page_size, parse_cursor, FollowPage};

use crate::session_notifications::{SessionNotification, SessionNotificationSubscriber};

pub use storage::record_json;

/// How many records a project's log keeps; older ones are evicted and counted.
pub const PROJECT_NOTIFICATION_CAPACITY: usize = 1_000;

/// The longest [`ProjectNotificationLog::wait`] is held, whatever it is asked for: the tools' cap
/// ([`MAX_WAIT_MS`]), the remote-call clamp that keeps one call under every transport's deadline.
pub const MAX_NOTIFICATION_WAIT: Duration = Duration::from_millis(MAX_WAIT_MS);

/// `wait`'s timeout as the log honours it: `requested`, capped at [`MAX_NOTIFICATION_WAIT`].
#[must_use]
pub fn clamp_wait(requested: Duration) -> Duration {
    requested.min(MAX_NOTIFICATION_WAIT)
}

/// Resolves an OS user to the sessions directory its sessions live in, to read a notifying
/// session's project from its metadata. `None` for a user that does not resolve.
pub type SessionsBaseFor = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// One attention notification, as the log keeps and returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectNotificationRecord {
    /// Position in the project's log: 1 for the first record ever, then increasing by one.
    pub seq: u64,
    pub session_id: String,
    /// The session's display label.
    pub label: String,
    pub kind: SessionNotificationKind,
    pub source: SessionNotificationSource,
    /// The operator-facing line.
    pub text: String,
    /// What the agent wrote, for an agent notification (`#home-inbox` 1/8).
    pub agent: Option<AgentNotification>,
    /// What the session asks, with the `request_id` it is answered by (`#home-inbox` 3/8).
    pub question: Option<QuestionNotice>,
    pub at_unix_ms: u64,
}

/// Whose records a read or a wait may see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectNotificationScope {
    pub project_id: String,
    /// The operating-system user the sessions belong to.
    pub os_user: String,
    /// The caller's own session, whose notifications are never returned.
    pub exclude_session_id: String,
}

/// A read's answer: a page, and how many records the cursor lost to eviction.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationRead {
    pub page: FollowPage<ProjectNotificationRecord>,
    /// Records of the project after the cursor that were evicted before they were read; `0` when
    /// the cursor is within the window. Counted over the project, whoever's records they were.
    pub lost: u64,
}

/// A wait's answer: a page, how many records the cursor lost to eviction, and whether the wait
/// ended by timing out.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationWait {
    pub page: FollowPage<ProjectNotificationRecord>,
    /// As [`NotificationRead::lost`]: a cursor evicted while the caller was not reading is reported,
    /// not silently trimmed.
    pub lost: u64,
    /// The wait returned because its timeout passed with nothing new; the page is then empty and
    /// its `next_cursor` is the cursor to wait from again.
    pub timed_out: bool,
}

/// The project notification log of one daemon: a bus subscriber and the reader of what it kept.
pub struct ProjectNotificationLog {
    data_dir: PathBuf,
    sessions_base_for: SessionsBaseFor,
    projects: Mutex<HashMap<String, Arc<Project>>>,
}

/// A record as the log holds it: the record, and the OS user whose session made it.
#[derive(Debug, Clone)]
struct Held {
    os_user: String,
    record: ProjectNotificationRecord,
}

impl Held {
    fn visible_to(&self, scope: &ProjectNotificationScope) -> bool {
        self.os_user == scope.os_user && self.record.session_id != scope.exclude_session_id
    }
}

/// One project's records, and the wake-up its waiters sleep on.
#[derive(Default)]
struct Project {
    state: Mutex<ProjectState>,
    /// Woken whenever a record is appended to the project.
    arrived: Notify,
}

#[derive(Default)]
struct ProjectState {
    /// The newest [`PROJECT_NOTIFICATION_CAPACITY`] records, oldest first.
    window: VecDeque<Held>,
    /// The `seq` of the newest record ever appended; `0` for a log that never held one.
    newest_seq: u64,
    /// The lines the project's file holds, which is rewritten once it has grown past twice the
    /// window.
    lines_on_disk: usize,
}

impl Project {
    fn restored(records: Vec<Held>, lines_on_disk: usize) -> Self {
        let newest_seq = records
            .iter()
            .map(|held| held.record.seq)
            .max()
            .unwrap_or(0);
        let skip = records.len().saturating_sub(PROJECT_NOTIFICATION_CAPACITY);
        let window = records.into_iter().skip(skip).collect();
        Self {
            state: Mutex::new(ProjectState {
                window,
                newest_seq,
                lines_on_disk,
            }),
            arrived: Notify::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ProjectState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Where a read or a wait starts: from the newest page, or after a `seq`.
#[derive(Clone, Copy)]
enum Start {
    Newest,
    After(u64),
}

impl ProjectNotificationLog {
    /// A log persisted under `data_dir` (`<data_dir>/projects/<project_id>/notifications.jsonl`),
    /// holding what a previous run of the daemon recorded there. `sessions_base_for` resolves a
    /// notifying session's project.
    ///
    /// An unreadable file is an error: a log is not started over records it could not read.
    pub fn open(data_dir: &Path, sessions_base_for: SessionsBaseFor) -> anyhow::Result<Self> {
        let projects = storage::replay_all(data_dir)?
            .into_iter()
            .map(|replayed| {
                (
                    replayed.project_id,
                    Arc::new(Project::restored(replayed.records, replayed.lines)),
                )
            })
            .collect();
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            sessions_base_for,
            projects: Mutex::new(projects),
        })
    }

    /// Up to `limit` records of `scope` after `cursor`, oldest first (default and maximum as the
    /// follow tools'), without waiting.
    ///
    /// An empty `cursor` returns the newest page. An error names why nothing was read: a `cursor`
    /// that is not a decimal `seq`.
    pub fn read(
        &self,
        scope: &ProjectNotificationScope,
        cursor: &str,
        limit: Option<usize>,
    ) -> Result<NotificationRead, String> {
        let start = match parse_cursor(cursor)? {
            Some(seq) => Start::After(seq),
            None => Start::Newest,
        };
        let project = self.project(&scope.project_id);
        let state = project.lock();
        let (page, lost) = page_of(&state, scope, start, page_size(limit));
        Ok(NotificationRead { page, lost })
    }

    /// The records of `scope` after `cursor`, holding the call until one arrives or `timeout`
    /// (capped by [`clamp_wait`]) passes. An empty `cursor` waits for the next new record.
    pub async fn wait(
        &self,
        scope: &ProjectNotificationScope,
        cursor: &str,
        timeout: Duration,
    ) -> Result<NotificationWait, String> {
        let cursor = parse_cursor(cursor)?;
        let project = self.project(&scope.project_id);
        let after = match cursor {
            Some(seq) => seq,
            // "From now": what is recorded after this call is what the wait is for.
            None => project.lock().newest_seq,
        };
        let size = page_size(None);
        let deadline = Instant::now() + clamp_wait(timeout);
        loop {
            // Registered before the log is read, so a record appended in between wakes this wait.
            // `notify_waiters` already reaches a `Notified` from its creation; `enable` makes the
            // registration explicit rather than leaning on that guarantee.
            let arrived = project.arrived.notified();
            tokio::pin!(arrived);
            arrived.as_mut().enable();
            let answer = {
                let state = project.lock();
                let (page, lost) = page_of(&state, scope, Start::After(after), size);
                (!page.items.is_empty() || Instant::now() >= deadline).then_some((page, lost))
            };
            if let Some((page, lost)) = answer {
                let timed_out = page.items.is_empty();
                return Ok(NotificationWait {
                    page,
                    lost,
                    timed_out,
                });
            }
            // Returns when a record arrives or the deadline passes; the loop then looks again.
            let _ = tokio::time::timeout_at(deadline, arrived).await;
        }
    }

    /// The project's log, created empty the first time it is named.
    fn project(&self, project_id: &str) -> Arc<Project> {
        let mut projects = self.projects.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(projects.entry(project_id.to_string()).or_default())
    }

    /// The project a notification's session belongs to, from its own record; `None` for a session
    /// of no project. A session whose OS user or record cannot be resolved is an error.
    fn project_of(&self, notification: &SessionNotification) -> anyhow::Result<Option<String>> {
        validate_session_id_segment(&notification.session_id)
            .map_err(|e| anyhow::anyhow!("{}", e.message()))?;
        let sessions_base = (self.sessions_base_for)(&notification.os_user).ok_or_else(|| {
            anyhow::anyhow!("no sessions directory for OS user {}", notification.os_user)
        })?;
        let session_dir = unified_session_dir_path(&sessions_base, &notification.session_id);
        let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|e| {
            anyhow::anyhow!(
                "session {} has no readable record: {e}",
                notification.session_id
            )
        })?;
        Ok((!metadata.project_id.is_empty()).then_some(metadata.project_id))
    }
}

/// What `state` holds for `scope` from `start`: a page of at most `size` records, and the number
/// of records the start lost to eviction.
///
/// The page's `next_cursor` is its last record's `seq`, or, for an empty page, the log's newest
/// `seq`. `has_more` says whether a record of `scope` follows the page's end.
fn page_of(
    state: &ProjectState,
    scope: &ProjectNotificationScope,
    start: Start,
    (size, clamped_to): (usize, Option<usize>),
) -> (FollowPage<ProjectNotificationRecord>, u64) {
    let visible = state.window.iter().filter(|held| held.visible_to(scope));
    let (held, has_more, lost) = match start {
        Start::Newest => {
            let all: Vec<&Held> = visible.collect();
            let first = all.len().saturating_sub(size);
            (all[first..].to_vec(), false, 0)
        }
        Start::After(seq) => {
            let after: Vec<&Held> = visible.filter(|held| held.record.seq > seq).collect();
            let has_more = after.len() > size;
            (
                after.into_iter().take(size).collect(),
                has_more,
                lost_after(state, seq),
            )
        }
    };
    let next_cursor = held
        .last()
        .map_or(state.newest_seq, |held| held.record.seq)
        .to_string();
    let items = held.into_iter().map(|held| held.record.clone()).collect();
    (
        FollowPage {
            items,
            next_cursor,
            has_more,
            clamped_to,
        },
        lost,
    )
}

/// How many records between `seq` and the window's oldest were evicted before they were read.
fn lost_after(state: &ProjectState, seq: u64) -> u64 {
    let Some(oldest) = state.window.front() else {
        return 0;
    };
    // The records numbered after `seq` and before the oldest kept one are the evicted ones.
    let last_evicted = oldest.record.seq.saturating_sub(1);
    last_evicted.saturating_sub(seq)
}

#[async_trait]
impl SessionNotificationSubscriber for ProjectNotificationLog {
    fn name(&self) -> &'static str {
        "project-notification-log"
    }

    fn wants(&self, notification: &SessionNotification) -> bool {
        matches!(
            notification.kind,
            SessionNotificationKind::AttentionRequired
        )
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        let Some(project_id) = self.project_of(notification)? else {
            // A session started without a project is a normal session, not a fault: nothing to
            // record, and nothing for the bus to warn about.
            log::debug!(
                target: "tddy_session_activity::project_notification_log",
                "session {} belongs to no project; its notification is not recorded",
                notification.session_id
            );
            return Ok(());
        };
        let path = storage::project_file(&self.data_dir, &project_id)?;
        let project = self.project(&project_id);
        {
            // Held across the append, so the file's lines are in `seq` order and a record is
            // either recorded whole or not at all.
            let mut state = project.lock();
            let held = Held {
                os_user: notification.os_user.clone(),
                record: ProjectNotificationRecord {
                    seq: state.newest_seq + 1,
                    session_id: notification.session_id.clone(),
                    label: notification.label.clone(),
                    kind: notification.kind,
                    source: notification.source,
                    text: notification.text.clone(),
                    agent: notification.agent.clone(),
                    question: notification.question.clone(),
                    at_unix_ms: notification.at_unix_ms,
                },
            };
            storage::append(&path, &held)?;
            state.newest_seq = held.record.seq;
            state.lines_on_disk += 1;
            state.window.push_back(held);
            if state.window.len() > PROJECT_NOTIFICATION_CAPACITY {
                state.window.pop_front();
            }
            if state.lines_on_disk > 2 * PROJECT_NOTIFICATION_CAPACITY {
                match storage::rewrite(&path, state.window.iter()) {
                    Ok(()) => state.lines_on_disk = state.window.len(),
                    // The record is recorded; the file keeps growing until a rewrite succeeds.
                    Err(e) => log::warn!(
                        target: "tddy_session_activity::project_notification_log",
                        "could not compact {}: {e:#}",
                        path.display()
                    ),
                }
            }
        }
        project.arrived.notify_waiters();
        Ok(())
    }
}
