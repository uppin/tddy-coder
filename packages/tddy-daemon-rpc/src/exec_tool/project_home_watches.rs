//! `CreateSessionWatch`, `ListSessionWatches`, `RenewSessionWatch` and `CancelSessionWatch`
//! answered on this host (`#activity-wake` 2/3).
//!
//! The tools ([`tddy_tool_engine::project_home_watch_tools`]) keep this host's session watches,
//! `tddy_session_activity::session_watches`, for the caller's project and OS user. Before a watch is
//! created the caller's agent must be one a turn can be sent to, and every session it names must be
//! one it can see — on this host, or on another connected host (n5). A named session on another
//! host is followed there: that host is asked to relay the session's records here until the watch
//! expires (`tddy_session_activity::notification_relay`), and a renewal asks again. The sessions the
//! caller started on other hosts need no follow: their hosts relay them to it already.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_session_activity::project_notification_log::ProjectNotificationLog;
use tddy_session_activity::session_watches::{
    SessionWatch, SessionWatchStore, WatchFilter, WatchKind, WatchOptions, WatchOwner,
};
use tddy_tool_engine::project_home_inbox_tools::NO_NOTIFICATION_LOG;
use tddy_tool_engine::project_home_watch_tools::{
    ProjectHomeWatchPort, WatchRequest, NO_SESSION_WATCHES,
};
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

use super::project_home::turn::terminal_submit_key;
use super::project_home::DaemonProjectHome;
use super::project_home_remote::PeerSessionLists;

impl DaemonProjectHome {
    /// The watches this host keeps, or a refusal when none is installed.
    fn watches(&self) -> Result<&SessionWatchStore, String> {
        self.session_watches
            .as_deref()
            .ok_or_else(|| NO_SESSION_WATCHES.to_string())
    }

    /// The log the watches read, or a refusal when none is installed.
    fn watch_log(&self) -> Result<&ProjectNotificationLog, String> {
        self.notification_log
            .as_deref()
            .ok_or_else(|| NO_NOTIFICATION_LOG.to_string())
    }

    /// Whose watches `home`'s are on this host.
    fn owner_of(&self, home: &HomeCaller) -> WatchOwner {
        WatchOwner {
            project_id: home.project_id.clone(),
            session_id: home.session_id.clone(),
            os_user: self.os_user.clone(),
        }
    }

    /// Refuse a caller whose agent no turn can be sent to, worded as `SendSessionMessage` words
    /// it: a watch with `wake_when_idle` promises a wake, which such an agent cannot take.
    fn refuse_an_agent_that_takes_no_turn(&self, home: &HomeCaller) -> Result<(), String> {
        let caller = SessionTarget {
            session_id: home.session_id.clone(),
            project_id: home.project_id.clone(),
            worktree: None,
            branch: String::new(),
            base_ref: String::new(),
            is_active: true,
            codebase: None,
        };
        let slot = self.turn_input_of(&caller)?;
        terminal_submit_key(&home.session_id, slot)?;
        Ok(())
    }

    /// Refuse a session `home` cannot watch: its own, or one with no record under its sessions
    /// base in its project on this host. Saying what to do instead is the tool's answer's job;
    /// here the refusal names the session.
    fn refuse_an_unseen_session(&self, home: &HomeCaller, session_id: &str) -> Result<(), String> {
        if session_id == home.session_id {
            return Err(format!(
                "session {session_id} is your own session: a watch never delivers your own session's events — name another session, or set started_by_me for the sessions you started"
            ));
        }
        let session_dir = unified_session_dir_path(&self.sessions_base, session_id);
        let metadata = tddy_core::read_session_metadata(&session_dir).map_err(|_| {
            format!(
                "session {session_id} cannot be seen on this host: it has no session record here — name a session of this project on this host (ListProjectSessions)"
            )
        })?;
        if metadata.project_id != home.project_id {
            return Err(format!(
                "session {session_id} belongs to another project: a watch sees only sessions of your own project on this host (ListProjectSessions)"
            ));
        }
        Ok(())
    }
}

/// The longest a watch tool waits for the other hosts to say which holds a session.
const HOST_LOOKUP_DEADLINE: Duration = Duration::from_secs(5);

impl DaemonProjectHome {
    /// Whether this host holds a record of `session_id` (of any project).
    fn holds_a_record_of(&self, session_id: &str) -> bool {
        tddy_core::read_session_metadata(&unified_session_dir_path(&self.sessions_base, session_id))
            .is_ok()
    }

    /// The other host holding `session_id` in the caller's project, as that host lists it; `None`
    /// when no connected host lists it, or for a caller held elsewhere (whose own host fans out).
    async fn host_holding(&self, session_id: &str) -> Option<String> {
        let peers = self.peers.as_ref()?;
        let listing = PeerSessionLists {
            routing: peers.routing.clone(),
            local_id: peers.local_id.clone(),
            request: tddy_service::proto::exec_tools::ExecuteToolRequest {
                tool_name: tddy_tool_engine::project_home_tools::LIST_PROJECT_SESSIONS.to_string(),
                args_json: "{}".to_string(),
                daemon_instance_id: String::new(),
                ..peers.request.clone()
            },
        };
        tokio::time::timeout(HOST_LOOKUP_DEADLINE, listing.list())
            .await
            .ok()?
            .sessions
            .into_iter()
            .find(|listed| listed.session_id == session_id)
            .map(|listed| listed.daemon_instance_id)
            .filter(|host| !host.is_empty())
    }

    /// Ask each host in `elsewhere` to relay its named session's records here, for `home`'s watch,
    /// until `until_unix_ms`. The first host that refuses is named.
    async fn follow_elsewhere(
        &self,
        home: &HomeCaller,
        elsewhere: &[(String, String)],
        until_unix_ms: u64,
    ) -> Result<(), String> {
        if elsewhere.is_empty() {
            return Ok(());
        }
        let peers = self
            .peers
            .as_ref()
            .ok_or("a watch over a session on another host is kept on the caller's own host")?;
        let github_user = self
            .github_user
            .clone()
            .ok_or("the caller's OS user maps to no GitHub login on this host")?;
        for (session_id, host) in elsewhere {
            let follow = tddy_session_activity::notification_relay::FollowRequest {
                follower_daemon_instance_id: peers.local_id.clone(),
                project_id: home.project_id.clone(),
                session_id: session_id.clone(),
                watcher_session_id: home.session_id.clone(),
                until_unix_ms,
                github_user: github_user.clone(),
            };
            self.relay_credentials
                .follow_on(host, &follow)
                .await
                .map_err(|reason| {
                    format!("host {host} cannot relay session {session_id}'s events here: {reason}")
                })?;
        }
        Ok(())
    }
}

/// A watch as the tools answer it, with how many matched events wait when listed. Shared with the
/// inbox watch a session that is not an orchestrator holds (`#activity-wake` 5/5).
pub(super) fn watch_json(watch: &SessionWatch, waiting: Option<usize>) -> Value {
    let mut answered = json!({
        "watch_id": watch.watch_id,
        "session_ids": watch.filter.session_ids,
        "started_by_me": watch.filter.started_by_me,
        "kinds": watch
            .filter
            .kinds
            .iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>(),
        "wake_when_idle": watch.wake_when_idle,
        "created_at_unix_ms": watch.created_at_unix_ms,
        "expires_at_unix_ms": watch.expires_at_unix_ms,
    });
    if let Some(waiting) = waiting {
        answered["waiting"] = json!(waiting);
    }
    answered
}

/// This moment, as the store's injected clock takes it.
fn now_unix_ms() -> u64 {
    tddy_daemon_kernel::now_unix_ms()
}

#[async_trait]
impl ProjectHomeWatchPort for DaemonProjectHome {
    async fn create_watch(
        &self,
        home: &HomeCaller,
        request: WatchRequest,
    ) -> Result<Value, String> {
        if request.wake_when_idle {
            self.refuse_an_agent_that_takes_no_turn(home)?;
        }
        // The self-watches: the orchestrator's own trunk activity (`#orchestra` 5/9) and its own
        // work queue's moves (`#workqueue` 8/14).
        let own_trunk_watch = request.kinds == [WatchKind::TrunkCommit.as_str()]
            || request.kinds == [WatchKind::WorkQueue.as_str()];
        let mut elsewhere: Vec<(String, String)> = Vec::new();
        for session_id in &request.session_ids {
            if own_trunk_watch && session_id == &home.session_id {
                continue;
            }
            if let Err(refusal) = self.refuse_an_unseen_session(home, session_id) {
                if session_id == &home.session_id || self.holds_a_record_of(session_id) {
                    return Err(refusal);
                }
                let host = self.host_holding(session_id).await.ok_or_else(|| {
                    format!(
                        "session {session_id} cannot be seen on this host or on any connected host: name a session of this project ListProjectSessions lists"
                    )
                })?;
                elsewhere.push((session_id.clone(), host));
            }
        }
        let kinds = request
            .kinds
            .iter()
            .map(|name| WatchKind::parse(name).ok_or_else(|| format!("unknown kind {name}")))
            .collect::<Result<Vec<_>, _>>()?;
        let watch = self
            .watches()?
            .create(
                &self.owner_of(home),
                WatchFilter {
                    session_ids: request.session_ids,
                    started_by_me: request.started_by_me,
                    kinds,
                },
                WatchOptions {
                    wake_when_idle: request.wake_when_idle,
                },
                Duration::from_secs(request.ttl_seconds),
                self.watch_log()?,
                now_unix_ms(),
            )
            .await?;
        if let Err(refusal) = self
            .follow_elsewhere(home, &elsewhere, watch.expires_at_unix_ms)
            .await
        {
            // A watch that cannot follow a session it names is not created half-working.
            let _ = self
                .watches()?
                .cancel(&self.owner_of(home), &watch.watch_id)
                .await;
            return Err(refusal);
        }
        Ok(watch_json(&watch, None))
    }

    async fn list_watches(&self, home: &HomeCaller) -> Result<Value, String> {
        let listed = self
            .watches()?
            .list(&self.owner_of(home), self.watch_log()?, now_unix_ms())
            .await;
        Ok(json!({
            "watches": listed
                .iter()
                .map(|listed| watch_json(&listed.watch, Some(listed.waiting)))
                .collect::<Vec<_>>(),
        }))
    }

    async fn renew_watch(
        &self,
        home: &HomeCaller,
        watch_id: &str,
        ttl_seconds: u64,
        wake_when_idle: Option<bool>,
    ) -> Result<Value, String> {
        let watch = self
            .watches()?
            .renew(
                &self.owner_of(home),
                watch_id,
                Duration::from_secs(ttl_seconds),
                wake_when_idle,
                now_unix_ms(),
            )
            .await?;
        // The named sessions on other hosts are followed for as long as the renewed watch lives; one
        // that cannot be followed again is named, never skipped silently.
        let mut elsewhere = Vec::new();
        for session_id in &watch.filter.session_ids {
            if session_id == &home.session_id || self.holds_a_record_of(session_id) {
                continue;
            }
            let host = self.host_holding(session_id).await.ok_or_else(|| {
                format!(
                    "watch {watch_id} was renewed, but session {session_id} is on no connected host that answered, so its events are not followed past the old expiry: renew again once its host is back"
                )
            })?;
            elsewhere.push((session_id.clone(), host));
        }
        self.follow_elsewhere(home, &elsewhere, watch.expires_at_unix_ms)
            .await
            .map_err(|refusal| format!("watch {watch_id} was renewed, but {refusal}"))?;
        Ok(watch_json(&watch, None))
    }

    async fn cancel_watch(&self, home: &HomeCaller, watch_id: &str) -> Result<Value, String> {
        self.watches()?
            .cancel(&self.owner_of(home), watch_id)
            .await?;
        Ok(json!({ "watch_id": watch_id, "cancelled": true }))
    }
}
