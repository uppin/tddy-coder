//! How this daemon names and publishes its sessions' notifications.
//!
//! The notification domain itself — the bus, the event, the subscriber trait and the three
//! builders — moved to [`tddy_session_activity::session_notifications`] with `#unbundle` node 7,
//! and is re-exported below so a call site names one module rather than two.
//!
//! What could not go is here: a session's display label is read from
//! [`crate::session_list_enrichment`], which serves `ListSessions` — family C, which stays in the
//! daemon deliberately. [`SessionNotificationPublishing`] is the publish context built on that
//! label, so it stays with it.

use std::path::Path;
use std::sync::Arc;

use tddy_core::session_label::session_display_label;

pub use tddy_session_activity::session_notifications::*;

/// A session's display label, read from the same values `ListSessions` reports to the drawer:
/// `repo_path` from `.session.yaml` and `workflow_goal` from the session-list enrichment.
///
/// Reading a different source — the worktree recorded in `changeset.yaml`, say — would name
/// sessions correctly right up until the two disagreed, and the whole point of the shared rule is
/// that the chat and the drawer cannot disagree. A session directory that is missing or unreadable
/// (a hook outracing session creation, a session deleted with a report in flight) falls to the
/// short session id rather than failing: a label is display text, and there is always one.
pub fn resolve_session_label(sessions_base: &Path, session_id: &str) -> String {
    let session_dir = tddy_core::unified_session_dir_path(sessions_base, session_id);

    let repo_path = tddy_core::read_session_metadata(&session_dir)
        .map(|meta| meta.repo_path.unwrap_or_default())
        .unwrap_or_else(|e| {
            log::debug!(
                target: "tddy_daemon::session_notifications",
                "resolve_session_label: no readable .session.yaml for session {session_id}: {e}"
            );
            String::new()
        });

    // The worktree wins outright when there is one, and there is one for most sessions — so ask
    // that first and skip the enrichment entirely. This runs on every reported hook, twice per
    // agent tool call, and the enrichment is a second parse of the file just read plus, for a
    // workflow session, its `changeset.yaml`. Reading a value the rule will not consult is the
    // difference between a cheap label and a per-tool-call file-parse tax.
    if let Some(basename) = tddy_core::session_label::label_from_repo_path(&repo_path) {
        return basename;
    }

    let workflow_goal =
        crate::session_list_enrichment::session_list_status_from_session_dir(&session_dir)
            .map(|status| status.workflow_goal)
            .unwrap_or_else(|e| {
                log::debug!(
                    target: "tddy_daemon::session_notifications",
                    "resolve_session_label: could not enrich session {session_id}: {e}"
                );
                String::new()
            });

    session_display_label(&repo_path, &workflow_goal, session_id)
}

/// What a publish site needs to raise a notification for a session: the bus, the OS user the
/// session belongs to, and the sessions directory its label is read from.
///
/// The three travel together because they are one fact — *whose* session this is — seen from three
/// sides. A publish without a label source would fall back to the session id and quietly undo the
/// parity FR1 exists for; a publish without an owner would raise a notification no client is
/// allowed to receive.
#[derive(Clone)]
pub struct SessionNotificationPublishing {
    pub bus: Arc<SessionNotificationBus>,
    /// The OS user whose sessions directory `sessions_base` is, and therefore the owner every
    /// notification raised through this context names.
    pub os_user: String,
    pub sessions_base: std::path::PathBuf,
}

impl SessionNotificationPublishing {
    /// Resolve `session_id`'s label and publish `build(label, os_user)`, when it yields a
    /// notification.
    ///
    /// `build` is handed the owner rather than reading it from anywhere else, so a notification
    /// raised here cannot name a user other than the one whose sessions directory its label came
    /// from.
    pub async fn publish_for_session(
        &self,
        session_id: &str,
        build: impl FnOnce(&str, &str) -> Option<SessionNotification>,
    ) {
        let label = resolve_session_label(&self.sessions_base, session_id);
        if let Some(notification) = build(&label, &self.os_user) {
            self.bus.publish(notification).await;
        }
    }
}
