//! The daemon's central watch over each project's remote trunk (`#orchestra` 5/9).
//!
//! Orchestrator-role sessions subscribe when they start or resume, and leave when they stop. One
//! [`TrunkPoller`] serves every subscriber of a project, so two orchestrators of one project cost
//! one conditional read per poll interval, not two. Each change it finds is published once per
//! subscriber as a [`SessionNotificationSource::TrunkCommit`] activity of **that subscriber's own
//! session** — the orchestrator's own activity, which it watches (a self-watch of kind
//! `trunk_commit`) and reads like any other.
//!
//! A subscriber the liveness port reports gone (stopped, ended, deleted) is dropped at the next
//! step, and a project with no subscriber left has its poller dropped: nothing polls for nobody.
//! [`TrunkWatcher::step`] holds no loop and no clock, so every rule is testable with a hand clock;
//! the daemon drives it from one task.
//!
//! [`SessionNotificationSource::TrunkCommit`]: tddy_daemon_kernel::session_notification::SessionNotificationSource::TrunkCommit

use std::sync::Arc;

use tddy_github::trunk_feed::{TrunkChange, TrunkPoller};

use crate::session_notifications::SessionNotificationBus;

/// One orchestrator subscribed to its project's trunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrunkSubscription {
    pub project_id: String,
    pub os_user: String,
    pub session_id: String,
}

/// A project's trunk poller, built for its first subscriber: the project's repository read through
/// the transport its GitHub setting names (the host's `gh`, or its GitHub account's token).
pub trait TrunkSource: Send + Sync {
    /// The poller of `subscription`'s project, and the trunk ref it reports (`origin/main`). An
    /// error names why the project's trunk cannot be watched (no GitHub remote, no identity).
    fn poller_for(&self, subscription: &TrunkSubscription)
        -> Result<(TrunkPoller, String), String>;
}

/// Whether a subscribed session is still running.
pub trait SubscriberLiveness: Send + Sync {
    fn is_live(&self, session_id: &str) -> bool;
}

/// The lines a change becomes, one activity each: per landed commit, the trunk ref, the commit and
/// its subject; for a rewrite, the trunk ref and the old and new tips.
pub fn trunk_activity_text(trunk_ref: &str, change: &TrunkChange) -> Vec<String> {
    // TODO(trunk-watch): implement
    let _ = (trunk_ref, change);
    unimplemented!("trunk_activity_text")
}

/// The central trunk watch.
pub struct TrunkWatcher {
    source: Arc<dyn TrunkSource>,
    liveness: Arc<dyn SubscriberLiveness>,
    bus: Arc<SessionNotificationBus>,
}

impl TrunkWatcher {
    pub fn new(
        source: Arc<dyn TrunkSource>,
        liveness: Arc<dyn SubscriberLiveness>,
        bus: Arc<SessionNotificationBus>,
    ) -> Self {
        Self {
            source,
            liveness,
            bus,
        }
    }

    /// Subscribe an orchestrator; a project's first subscriber builds its poller. An error names why
    /// the project's trunk cannot be watched; the session runs on without it.
    pub fn subscribe(&self, subscription: TrunkSubscription) -> Result<(), String> {
        // TODO(trunk-watch): implement
        let _ = (&self.source, subscription);
        unimplemented!("TrunkWatcher::subscribe")
    }

    /// Unsubscribe a session; its project's last subscriber leaving drops the poller.
    pub fn unsubscribe(&self, session_id: &str) {
        // TODO(trunk-watch): implement
        let _ = session_id;
        unimplemented!("TrunkWatcher::unsubscribe")
    }

    /// The projects a poller runs for, sorted.
    pub fn live_pollers(&self) -> Vec<String> {
        // TODO(trunk-watch): implement
        unimplemented!("TrunkWatcher::live_pollers")
    }

    /// Drop subscribers that are no longer live (and pollers left with none), then poll every
    /// poller due at `now_unix_ms` and publish what each found to each of its subscribers. Returns
    /// the earliest time a poller is next due, or `None` when no poller is left.
    pub async fn step(&self, now_unix_ms: u64) -> Option<u64> {
        // TODO(trunk-watch): implement
        let _ = (&self.liveness, &self.bus, now_unix_ms);
        unimplemented!("TrunkWatcher::step")
    }
}
