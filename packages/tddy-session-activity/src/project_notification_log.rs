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
//!   resolved from the session's metadata; a notification whose session or owner cannot be resolved
//!   belongs to no project and is not recorded.
//! - **Window.** The newest [`PROJECT_NOTIFICATION_CAPACITY`] records are kept per project. A cursor
//!   older than the window is reported as `lost`, never silently trimmed.
//! - **Persistence.** Appended as JSON lines to
//!   `<data dir>/projects/<project_id>/notifications.jsonl` and replayed on [`ProjectNotificationLog::open`],
//!   so a cursor survives a daemon restart and `seq` continues where it stopped.
//! - **Cursor.** The decimal `seq` of the last record a page returned. An empty cursor means "from
//!   now" for a wait and "the newest page" for a read. Paging is `#start-parity` 6/9's
//!   ([`FollowPage`]), with one addition the wait needs: **the `next_cursor` of an empty page is
//!   the log's newest `seq`** (`"0"` for a log that never held a record), not the empty cursor it
//!   was given, so polling from it can neither repeat nor skip a record published in between.
//! - **Scope.** A read or a wait is for one project and one operating-system user, and excludes one
//!   session — the caller's own ([`ProjectNotificationScope`]).
//!
//! Feature doc: docs/ft/daemon/session-notifications.md, docs/ft/daemon/project-concept.md.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_daemon_kernel::session_notification::{
    AgentNotification, QuestionNotice, SessionNotificationKind, SessionNotificationSource,
};
use tddy_tool_engine::session_follow::FollowPage;

use crate::session_notifications::{SessionNotification, SessionNotificationSubscriber};

/// How many records a project's log keeps; older ones are evicted and counted.
pub const PROJECT_NOTIFICATION_CAPACITY: usize = 1_000;

/// The longest [`ProjectNotificationLog::wait`] is held, whatever it is asked for: the
/// remote-call clamp, which keeps one call under every transport's deadline.
pub const MAX_NOTIFICATION_WAIT: Duration = Duration::from_millis(20_000);

/// `wait`'s timeout as the log honours it: `requested`, capped at [`MAX_NOTIFICATION_WAIT`].
#[must_use]
pub fn clamp_wait(requested: Duration) -> Duration {
    // TODO(home-wait): implement — `requested.min(MAX_NOTIFICATION_WAIT)`.
    requested
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
    /// Records after the cursor that were evicted before they were read; `0` when the cursor is
    /// within the window.
    pub lost: u64,
}

/// A wait's answer: a page, and whether it ended by timing out.
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationWait {
    pub page: FollowPage<ProjectNotificationRecord>,
    /// The wait returned because its timeout passed with nothing new; the page is then empty and
    /// its `next_cursor` is the cursor to wait from again.
    pub timed_out: bool,
}

/// The project notification log of one daemon: a bus subscriber and the reader of what it kept.
pub struct ProjectNotificationLog {
    data_dir: PathBuf,
    sessions_base_for: SessionsBaseFor,
}

impl ProjectNotificationLog {
    /// A log persisted under `data_dir` (`<data_dir>/projects/<project_id>/notifications.jsonl`),
    /// holding what a previous run of the daemon recorded there. `sessions_base_for` resolves a
    /// notifying session's project.
    pub fn open(data_dir: &Path, sessions_base_for: SessionsBaseFor) -> anyhow::Result<Self> {
        // TODO(home-wait): implement — replay each project's JSON lines into its ring, keeping the
        // newest `PROJECT_NOTIFICATION_CAPACITY`, so `seq` continues across a restart.
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            sessions_base_for,
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
        // TODO(home-wait): implement — page the project's ring after `cursor`, filtered by `scope`.
        let _ = (&self.data_dir, scope, cursor, limit);
        Err("ProjectNotificationLog::read: not implemented".to_string())
    }

    /// The records of `scope` after `cursor`, holding the call until one arrives or `timeout`
    /// (capped by [`clamp_wait`]) passes. An empty `cursor` waits for the next new record.
    pub async fn wait(
        &self,
        scope: &ProjectNotificationScope,
        cursor: &str,
        timeout: Duration,
    ) -> Result<NotificationWait, String> {
        // TODO(home-wait): implement — return at once when records after `cursor` exist, else park
        // on the project's `tokio::sync::Notify` until one is recorded or `clamp_wait(timeout)`.
        let _ = (scope, cursor, timeout);
        Err("ProjectNotificationLog::wait: not implemented".to_string())
    }
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
        // TODO(home-wait): implement — resolve the project from the session's metadata through
        // `sessions_base_for(os_user)`, append the record to the ring and the JSONL file, and wake
        // the project's waiters.
        let _ = (&self.sessions_base_for, notification);
        Ok(())
    }
}
