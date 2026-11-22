use crate::project_notification_log::ProjectNotificationLog;
use crate::session_messages::{a_message_to, InboxMessage, InboxRead};
use crate::session_watches::a_watch_has_delivered;
use crate::session_watches::delivery::apply_delivery;
use crate::session_watches::delivery::end_expired;
use crate::session_watches::delivery::end_finished;
use crate::session_watches::delivery::WatchDelivery;
use crate::session_watches::inbox_position_of;
use crate::session_watches::matcher::watch_matches;
use crate::session_watches::matcher::WatchFilter;
use crate::session_watches::refuse_a_bad_ttl;
use crate::session_watches::stream::STREAM_LEASE_GRACE;
use crate::session_watches::ListedWatch;
use crate::session_watches::ProjectWatches;
use crate::session_watches::SessionWatch;
use crate::session_watches::SessionWatchStore;
use crate::session_watches::WatchOwner;
use crate::session_watches::MAX_LIVE_WATCHES;
use crate::session_watches::SESSION_WATCHES_FILE;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
            return Err("name at least one kind, or name none for every kind".to_string());
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
                let inbox = inbox_position_of(&state, owner);
                let waiting = records
                    .iter()
                    .filter(|record| watch_matches(watch, record, inbox))
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

    /// The messages addressed to `recipient` after `start_seq`, oldest first, at most `limit` —
    /// the body of `ReadSessionMessages`, which `crate::session_messages` words. With
    /// `skip_watch_delivered`, a message a live watch of `recipient` already handed is not handed
    /// again. The inbox position ends at the newest message handed — a watch-delivered one
    /// skipped included — never behind where it stood, under the project's lock, persisted with
    /// the watches. `lost` is the log's eviction count after `start_seq`.
    pub(crate) async fn read_inbox(
        &self,
        recipient: &WatchOwner,
        start_seq: u64,
        limit: usize,
        skip_watch_delivered: bool,
        log: &ProjectNotificationLog,
        now_unix_ms: u64,
    ) -> InboxRead {
        let project = self.project_of(&recipient.project_id);
        let mut state = project.lock().await;
        let ended_some = end_finished(&mut state, recipient, log, now_unix_ms);
        let position = inbox_position_of(&state, recipient);
        let (records, _) = log.records_after(&recipient.project_id, &recipient.os_user, start_seq);
        let lost = log.messages_lost_after(
            &recipient.project_id,
            &recipient.os_user,
            &recipient.session_id,
            start_seq,
        );
        let mut handed: Vec<InboxMessage> = Vec::new();
        let mut newest = position;
        let mut has_more = false;
        for record in records
            .iter()
            .filter(|record| a_message_to(record, &recipient.session_id).is_some())
        {
            if skip_watch_delivered && a_watch_has_delivered(&state, recipient, record, position) {
                // Delivered by a watch already; the position passes it, so no later read of
                // the log hands it either.
                newest = newest.max(record.seq);
                continue;
            }
            if handed.len() >= limit {
                has_more = true;
                break;
            }
            if let Some(message) = InboxMessage::of(record) {
                newest = newest.max(record.seq);
                handed.push(message);
            }
        }
        if newest > position {
            state
                .inbox_positions
                .insert(recipient.session_id.clone(), newest);
        }
        if newest != position || ended_some {
            self.persist_best_effort(&recipient.project_id, &state);
        }
        InboxRead {
            messages: handed,
            lost,
            has_more,
        }
    }

    /// The project's watches, loaded the first time it is named.
    pub(crate) fn project_of(&self, project_id: &str) -> Arc<tokio::sync::Mutex<ProjectWatches>> {
        let mut projects = self
            .projects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(projects.entry(project_id.to_string()).or_default())
    }
}
