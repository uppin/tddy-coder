//! The daemon drives the schedule runner (`#workqueue` 14/14): an armed schedule fires by itself,
//! once, when its last condition turns met — without anyone watching the queue.
//!
//! One watcher per queue with schedules, subscribed to the GitHub inbox's events for the queue's
//! repository (PR, checks, comments, pushes) — a subscription, never a poll, so the inbox's own quota
//! discipline holds. Each event re-evaluates the queue's schedules and hands the met ones to a
//! [`ScheduleRunner`](tddy_work_queue::schedule_runner::ScheduleRunner); a change to the queue itself
//! (a schedule armed or disarmed) calls [`after_arming`], which evaluates at once and makes sure a
//! watcher is running. A schedule is taken out of the store in the write that fires it, so however many
//! events or watchers see it met, it fires once. The watcher ends when nothing is armed.
//!
//! The watcher carries the credentials of the person who armed (the inbox registers the repository to
//! be read as them); it does not survive a daemon restart — the next arming, or a queue status stream
//! opened by the web, starts it again.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tddy_core::work_queue_store::WorkQueueStore;
use tddy_github_inbox::EventKind;
use tddy_service::proto::work_queue::WorkQueueAddress;
use tddy_work_queue::lineage::trunk_branch_of;
use tddy_worktree_service::pull_request_view::SessionPullRequests;

use super::land::{fire_met_in_background, DaemonFireSchedule, GitEnvSource};
use super::schedules::ScheduleCtx;

/// Everything a watcher needs, owned, for one queue on this host.
pub(crate) struct WatchInputs {
    pub source: GitEnvSource,
    pub pull_requests: Arc<dyn SessionPullRequests>,
    pub store: WorkQueueStore,
    pub sessions_base: PathBuf,
    pub os_user: String,
    pub address: WorkQueueAddress,
    pub session_token: String,
}

fn running() -> &'static Mutex<HashSet<String>> {
    static RUNNING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    RUNNING.get_or_init(Default::default)
}

impl WatchInputs {
    fn key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.sessions_base.display(),
            self.address.project_id,
            self.address.queue_id
        )
    }

    fn trunk(&self) -> Result<String, String> {
        tddy_projects::project_storage::effective_integration_base_ref_for_project(
            &self.store.projects_dir(),
            &self.address.project_id,
        )
        .map(|base| trunk_branch_of(&base).to_string())
        .map_err(|e| format!("the project's trunk could not be resolved: {e:#}"))
    }

    fn firing(&self) -> DaemonFireSchedule {
        DaemonFireSchedule::through_engines(
            self.source.clone(),
            self.store.clone(),
            self.sessions_base.clone(),
            self.os_user.clone(),
            self.address.clone(),
        )
    }

    /// Evaluate the queue's schedules now and fire the met ones. `false` when none is armed.
    async fn evaluate_and_fire(&self) -> bool {
        let Ok(record) = self
            .store
            .read(&self.address.project_id, &self.address.queue_id)
        else {
            return false;
        };
        if record.stack.schedules.is_empty() {
            return false;
        }
        let trunk = match self.trunk() {
            Ok(trunk) => trunk,
            Err(why) => {
                log::warn!(
                    "schedules of {} not evaluated: {why}",
                    self.address.queue_id
                );
                return true;
            }
        };
        let (views, _) = ScheduleCtx {
            store: &self.store,
            pull_requests: self.pull_requests.as_ref(),
            os_user: &self.os_user,
            sessions_base: &self.sessions_base,
            project_id: &self.address.project_id,
            queue_id: &self.address.queue_id,
            caller_session_id: &self.address.queue_id,
            session_token: &self.session_token,
            trunk: &trunk,
            armed_by: &self.os_user,
        }
        .evaluate_views(&record.stack)
        .await;
        fire_met_in_background(self.firing(), views);
        true
    }
}

/// A schedule was armed or removed: evaluate at once, and keep a watcher on the queue's repository
/// while anything is armed.
pub(crate) async fn after_arming(inputs: WatchInputs) {
    if inputs.evaluate_and_fire().await {
        ensure_watcher(inputs);
    }
}

fn ensure_watcher(inputs: WatchInputs) {
    let key = inputs.key();
    if !running()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.clone())
    {
        return;
    }
    tokio::spawn(async move {
        watch(&inputs, &key).await;
        running()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    });
}

async fn watch(inputs: &WatchInputs, key: &str) {
    let site = match inputs.pull_requests.site_for_session(
        &inputs.os_user,
        &inputs.address.queue_id,
        &inputs.address.project_id,
        &inputs.session_token,
    ) {
        Ok(site) => site,
        Err(status) => {
            log::warn!(
                "schedules of {} are not watched: {}",
                inputs.address.queue_id,
                status.message()
            );
            return;
        }
    };
    // Subscribed before the first evaluation, so a change landing during it is not missed.
    let mut events = site.inbox.subscribe(
        &site.repo,
        &[
            EventKind::Pr,
            EventKind::Checks,
            EventKind::Comments,
            EventKind::Push,
        ],
    );
    loop {
        if !inputs.evaluate_and_fire().await {
            // Nothing armed: stop, unless a schedule was armed while this decided (its arming found
            // this watcher running and started none).
            running()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(key);
            if !inputs.evaluate_and_fire().await {
                return;
            }
            running()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(key.to_string());
        }
        if events.next().await.is_none() {
            return;
        }
    }
}
