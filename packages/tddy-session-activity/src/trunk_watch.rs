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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tddy_github::trunk_feed::{TrunkChange, TrunkPoller};

use tddy_daemon_kernel::session_notification::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
};

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

/// The label a trunk activity carries: it belongs to the orchestrator, not to a repository.
const ACTIVITY_LABEL: &str = "orchestrator";

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

/// The lines a change becomes, one activity each: per landed commit, the trunk ref, the commit and
/// its subject; for a rewrite, the trunk ref and the old and new tips.
pub fn trunk_activity_text(trunk_ref: &str, change: &TrunkChange) -> Vec<String> {
    match change {
        TrunkChange::Landed(commits) => commits
            .iter()
            .map(|c| {
                format!(
                    "{trunk_ref}: {} {} ({})",
                    short(&c.sha),
                    c.subject,
                    c.author
                )
            })
            .collect(),
        TrunkChange::Rewritten { from, to } => vec![format!(
            "{trunk_ref} was rewritten: {} -> {} {} ({})",
            short(from),
            short(&to.sha),
            to.subject,
            to.author
        )],
    }
}

struct ProjectPoller {
    /// Taken out while a poll is awaited, so no lock is held across it.
    poller: Option<TrunkPoller>,
    trunk_ref: String,
    next_due_unix_ms: u64,
    subscribers: Vec<TrunkSubscription>,
}

/// The central trunk watch.
pub struct TrunkWatcher {
    source: Arc<dyn TrunkSource>,
    liveness: Arc<dyn SubscriberLiveness>,
    bus: Arc<SessionNotificationBus>,
    projects: Mutex<HashMap<String, ProjectPoller>>,
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
            projects: Mutex::new(HashMap::new()),
        }
    }

    fn projects(&self) -> std::sync::MutexGuard<'_, HashMap<String, ProjectPoller>> {
        self.projects.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Subscribe an orchestrator; a project's first subscriber builds its poller. An error names why
    /// the project's trunk cannot be watched; the session runs on without it.
    pub fn subscribe(&self, subscription: TrunkSubscription) -> Result<(), String> {
        let mut projects = self.projects();
        if !projects.contains_key(&subscription.project_id) {
            let (poller, trunk_ref) = self.source.poller_for(&subscription)?;
            projects.insert(
                subscription.project_id.clone(),
                ProjectPoller {
                    poller: Some(poller),
                    trunk_ref,
                    next_due_unix_ms: 0,
                    subscribers: Vec::new(),
                },
            );
        }
        let project = projects
            .get_mut(&subscription.project_id)
            .expect("inserted above");
        project
            .subscribers
            .retain(|s| s.session_id != subscription.session_id);
        project.subscribers.push(subscription);
        Ok(())
    }

    /// Unsubscribe a session; its project's last subscriber leaving drops the poller.
    pub fn unsubscribe(&self, session_id: &str) {
        let mut projects = self.projects();
        for project in projects.values_mut() {
            project.subscribers.retain(|s| s.session_id != session_id);
        }
        projects.retain(|_, p| !p.subscribers.is_empty());
    }

    /// The projects a poller runs for, sorted.
    pub fn live_pollers(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.projects().keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Drop subscribers that are no longer live (and pollers left with none), then poll every
    /// poller due at `now_unix_ms` and publish what each found to each of its subscribers. Returns
    /// the earliest time a poller is next due, or `None` when no poller is left.
    pub async fn step(&self, now_unix_ms: u64) -> Option<u64> {
        let due: Vec<(String, TrunkPoller)> = {
            let mut projects = self.projects();
            for project in projects.values_mut() {
                project
                    .subscribers
                    .retain(|s| self.liveness.is_live(&s.session_id));
            }
            projects.retain(|_, p| !p.subscribers.is_empty());
            projects
                .iter_mut()
                .filter(|(_, p)| p.next_due_unix_ms <= now_unix_ms)
                .filter_map(|(id, p)| Some((id.clone(), p.poller.take()?)))
                .collect()
        };
        for (project_id, mut poller) in due {
            let outcome = poller.poll(now_unix_ms).await;
            let (trunk_ref, subscribers) = {
                let mut projects = self.projects();
                let Some(project) = projects.get_mut(&project_id) else {
                    continue;
                };
                project.poller = Some(poller);
                project.next_due_unix_ms = outcome.next_poll_at_unix_ms;
                (project.trunk_ref.clone(), project.subscribers.clone())
            };
            for change in &outcome.changes {
                for text in trunk_activity_text(&trunk_ref, change) {
                    for subscriber in &subscribers {
                        self.bus
                            .publish(SessionNotification {
                                session_id: subscriber.session_id.clone(),
                                os_user: subscriber.os_user.clone(),
                                label: ACTIVITY_LABEL.to_string(),
                                kind: SessionNotificationKind::Activity,
                                source: SessionNotificationSource::TrunkCommit,
                                text: text.clone(),
                                agent: None,
                                question: None,
                                ended: None,
                                activity_status: None,
                                message: None,
                                at_unix_ms: now_unix_ms,
                            })
                            .await;
                    }
                }
            }
        }
        self.projects().values().map(|p| p.next_due_unix_ms).min()
    }
}
