//! Waking an idle orchestrator with what its session watches matched (`#activity-wake` 3/3).
//!
//! A [`WakeTrigger`] is registered on the notification bus. The bus awaits each subscriber inside
//! the hook RPC that published, so the trigger only notes which session to look at and returns; a
//! [`WatchWaker`] task, started once the exec-tool handler exists, does the rest: it waits
//! [`WakeTimings::settle`] so a burst becomes one turn, keeps [`WakeTimings::min_gap`] between two
//! wakes of one orchestrator, checks the orchestrator is idle ([`orchestrator_is_idle`]), and
//! pastes [`wake_text`] into its main terminal — the paste `SendSessionMessage` makes — through a
//! delivery committed only when the paste succeeds.
//!
//! Feature: docs/ft/daemon/project-concept.md#watching-sessions

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationKind, SessionNotificationSource,
    SessionNotificationSubscriber,
};
use tddy_session_activity::session_watches::{
    WatchDelivery, WatchEndReason, WatchKind, WatchOwner,
};
use tddy_session_lifecycle::user_sessions_path::sessions_base_for_user;
use tddy_terminal_rpc::bridge::MAIN_TERMINAL_ID;
use tddy_terminal_rpc::session::TerminalSessionStore;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::project_home::turn::{paste_and_submit_to, terminal_submit_key};
use super::ExecToolRpcHandler;

/// How many cues may wait for the waker before a new one is dropped. A dropped cue loses nothing:
/// the next event, or the gap's timer, looks at the orchestrator again.
pub const WAKE_CUE_CAPACITY: usize = 1_024;

/// How long a wake waits after the event that triggered it, and the least time between two wakes of
/// one orchestrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakeTimings {
    pub settle: Duration,
    pub min_gap: Duration,
}

impl Default for WakeTimings {
    /// 2 s to settle, 30 s between wakes.
    fn default() -> Self {
        Self {
            settle: Duration::from_secs(2),
            min_gap: Duration::from_secs(30),
        }
    }
}

/// One notification's session, handed from the bus to the waker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeCue {
    pub session_id: String,
    pub os_user: String,
}

/// The waker's end of the trigger's channel, held until the waker starts.
pub struct WakeCues(mpsc::Receiver<WakeCue>);

/// The bus subscriber that tells the waker which session to look at.
pub struct WakeTrigger {
    cues: mpsc::Sender<WakeCue>,
}

impl WakeTrigger {
    /// A trigger to register on the bus, and the cues it sends, for [`ExecToolRpcHandler::start_watch_waker`].
    pub fn channel() -> (Self, WakeCues) {
        let (cues, received) = mpsc::channel(WAKE_CUE_CAPACITY);
        (Self { cues }, WakeCues(received))
    }
}

#[async_trait]
impl SessionNotificationSubscriber for WakeTrigger {
    fn name(&self) -> &'static str {
        "watch-wake"
    }

    fn wants(&self, notification: &SessionNotification) -> bool {
        notification.kind == SessionNotificationKind::AttentionRequired
            || notification.source == SessionNotificationSource::ActivityStatus
    }

    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()> {
        // Never awaits the waker: a full channel drops the cue (see `WAKE_CUE_CAPACITY`).
        let _ = self.cues.try_send(WakeCue {
            session_id: notification.session_id.clone(),
            os_user: notification.os_user.clone(),
        });
        Ok(())
    }
}

/// Whether an orchestrator may be sent a turn: its main terminal is live, it waits on no question
/// of its own, and its reported status is `Done`, `WaitingForInput`, `Started` or `Ended` (what
/// `/clear` writes) — no status counts as `Started`. `Running` and `ExecutingTool` are mid-turn.
pub fn orchestrator_is_idle(
    activity_status: Option<&str>,
    waits_on_its_own_question: bool,
    main_terminal_live: bool,
) -> bool {
    main_terminal_live
        && !waits_on_its_own_question
        && matches!(
            activity_status,
            None | Some("Done" | "WaitingForInput" | "Started" | "Ended")
        )
}

/// The turn a wake sends: one line per event, oldest first, naming the session, the kind, what it
/// said and the `request_id` a question is answered by; then what was lost and which watches ended.
pub fn wake_text(delivery: &WatchDelivery) -> String {
    let count = delivery.events.len();
    let mut text = format!(
        "[tddy watch] {count} watched event{} (oldest first):",
        if count == 1 { "" } else { "s" }
    );
    for event in &delivery.events {
        let kind = WatchKind::of(event).map_or("event", WatchKind::as_str);
        text.push_str(&format!(
            "\n- Session {} — {kind}: {}",
            event.label, event.text
        ));
        if let Some(question) = &event.question {
            text.push_str(&format!(" (request_id {})", question.request_id));
        }
    }
    if delivery.lost > 0 {
        text.push_str(&format!(
            "\n{} lost to eviction before they could be delivered.",
            delivery.lost
        ));
    }
    for end in &delivery.ended {
        let reason = match end.reason {
            WatchEndReason::Expired => "expired",
            WatchEndReason::OrchestratorEnded => "orchestrator_ended",
            WatchEndReason::SessionsEnded => "sessions_ended",
        };
        text.push_str(&format!("\nwatch {} ended ({reason})", end.watch_id));
    }
    if count > 0 {
        text.push_str(
            "\nAnswer with AnswerSessionPermission / AnswerSessionQuestion, or read ReadProjectNotifications for detail.",
        );
    }
    text
}

/// One orchestrator's wake schedule in the waker task.
#[derive(Default)]
struct OwnerWake {
    /// When the armed settle (or the gap's end) fires; `None` when nothing is armed.
    fire_at: Option<Instant>,
    /// When this orchestrator was last woken, for the gap.
    last_wake: Option<Instant>,
}

impl ExecToolRpcHandler {
    /// Start waking this host's idle watching orchestrators from `cues`, with `timings`.
    pub fn start_watch_waker(
        self: &Arc<Self>,
        cues: WakeCues,
        timings: WakeTimings,
    ) -> JoinHandle<()> {
        let handler = Arc::clone(self);
        tokio::spawn(async move { handler.run_watch_waker(cues, timings).await })
    }

    /// The waker task: a cue arms a settle for every orchestrator with live watches in the cue's
    /// project; on fire, the gap is honoured first, then an idle orchestrator is pasted its
    /// waiting delivery — committed only when the paste succeeds, so a failed or refused wake
    /// leaves everything waiting for the next cue.
    async fn run_watch_waker(&self, cues: WakeCues, timings: WakeTimings) {
        let WakeCues(mut cues) = cues;
        let mut owners: HashMap<WatchOwner, OwnerWake> = HashMap::new();
        loop {
            let deadline = owners.values().filter_map(|owner| owner.fire_at).min();
            let cue = match deadline {
                Some(at) => tokio::select! {
                    received = cues.recv() => match received {
                        Some(cue) => Some(cue),
                        None => break,
                    },
                    () = tokio::time::sleep_until(at) => None,
                },
                None => match cues.recv().await {
                    Some(cue) => Some(cue),
                    None => break,
                },
            };
            if let Some(cue) = cue {
                for owner in self.owners_concerned(&cue).await {
                    let wake = owners.entry(owner).or_default();
                    if wake.fire_at.is_none() {
                        wake.fire_at = Some(Instant::now() + timings.settle);
                    }
                }
            }
            let now = Instant::now();
            let due: Vec<WatchOwner> = owners
                .iter()
                .filter(|(_, wake)| wake.fire_at.is_some_and(|at| at <= now))
                .map(|(owner, _)| owner.clone())
                .collect();
            for owner in due {
                let Some(wake) = owners.get_mut(&owner) else {
                    continue;
                };
                wake.fire_at = None;
                if let Some(last) = wake.last_wake {
                    if now.duration_since(last) < timings.min_gap {
                        wake.fire_at = Some(last + timings.min_gap);
                        continue;
                    }
                }
                if self.try_wake(&owner).await {
                    if let Some(wake) = owners.get_mut(&owner) {
                        wake.last_wake = Some(Instant::now());
                    }
                }
            }
        }
    }

    /// The owners a cue concerns: every session of the cue session's project on this host that
    /// holds a live watch — the cue's own session included, for when its own status changed.
    async fn owners_concerned(&self, cue: &WakeCue) -> Vec<WatchOwner> {
        let (Some(watches), Some(log)) = (
            self.project_home_host.session_watches.as_deref(),
            self.project_home_host.notification_log.as_deref(),
        ) else {
            return Vec::new();
        };
        let Some(sessions_base) = sessions_base_for_user(&cue.os_user, Some(&self.tddy_data_dir))
        else {
            return Vec::new();
        };
        let cue_dir = unified_session_dir_path(&sessions_base, &cue.session_id);
        let Ok(cue_metadata) = tddy_core::read_session_metadata(&cue_dir) else {
            return Vec::new();
        };
        if cue_metadata.project_id.is_empty() {
            return Vec::new();
        }
        let Ok(sessions) =
            tddy_session_activity::session_reader::list_sessions_in_dir(&sessions_base)
        else {
            return Vec::new();
        };
        let now = tddy_daemon_kernel::now_unix_ms();
        let mut concerned = Vec::new();
        for session in sessions {
            if session.project_id != cue_metadata.project_id {
                continue;
            }
            let owner = WatchOwner {
                project_id: session.project_id,
                session_id: session.session_id,
                os_user: cue.os_user.clone(),
            };
            if watches.has_live_watches(&owner, log, now).await {
                concerned.push(owner);
            }
        }
        concerned
    }

    /// Whether `owner` is idle now: it waits on no question of its own, its main terminal is
    /// live, and its reported status is an idle one.
    async fn is_idle_now(
        &self,
        owner: &WatchOwner,
        session_dir: &std::path::Path,
        metadata: &tddy_core::SessionMetadata,
    ) -> bool {
        let waits_on_its_own_question =
            tddy_core::pending_question::read_pending_question(session_dir)
                .is_ok_and(|pending| pending.is_some());
        let main_terminal_live = self
            .project_home_host
            .terminals
            .get_terminal(&owner.session_id, MAIN_TERMINAL_ID)
            .await
            .is_some();
        orchestrator_is_idle(
            metadata.activity_status.as_deref(),
            waits_on_its_own_question,
            main_terminal_live,
        )
    }

    /// The submit key a wake ends with for a session of `metadata`'s type, or `None` when its
    /// agent takes no terminal turn this host can send.
    fn wake_submit_key(
        &self,
        metadata: &tddy_core::SessionMetadata,
        session_id: &str,
    ) -> Option<String> {
        let launch = self.session_launch.get()?;
        let session_type = metadata.session_type.as_deref().unwrap_or_default();
        let agent = launch
            .coding_agents
            .agents()
            .for_session_type(session_type)?;
        let slot = agent.turn_input().cloned()?;
        terminal_submit_key(session_id, slot).ok()
    }

    /// Paste `owner`'s waiting delivery into its main terminal when it is idle. `true` only when
    /// a turn was pasted, which starts the owner's gap.
    async fn try_wake(&self, owner: &WatchOwner) -> bool {
        let (Some(watches), Some(log)) = (
            self.project_home_host.session_watches.as_deref(),
            self.project_home_host.notification_log.as_deref(),
        ) else {
            return false;
        };
        let Some(sessions_base) = sessions_base_for_user(&owner.os_user, Some(&self.tddy_data_dir))
        else {
            return false;
        };
        let session_dir = unified_session_dir_path(&sessions_base, &owner.session_id);
        let Ok(metadata) = tddy_core::read_session_metadata(&session_dir) else {
            return false;
        };
        if !self.is_idle_now(owner, &session_dir, &metadata).await {
            return false;
        }
        let Some(submit_key) = self.wake_submit_key(&metadata, &owner.session_id) else {
            return false;
        };
        let pasted = Arc::new(AtomicBool::new(false));
        let outcome = watches
            .deliver_with(owner, log, tddy_daemon_kernel::now_unix_ms(), |delivery| {
                let terminals = self.project_home_host.terminals.clone();
                let session_id = owner.session_id.clone();
                let pasted = Arc::clone(&pasted);
                async move {
                    if delivery.events.is_empty() && delivery.lost == 0 {
                        return Ok(());
                    }
                    pasted.store(true, Ordering::SeqCst);
                    paste_and_submit_to(&terminals, &session_id, &wake_text(&delivery), &submit_key)
                        .await
                }
            })
            .await;
        match outcome {
            Ok(_) => pasted.load(Ordering::SeqCst),
            Err(e) => {
                log::debug!(
                    target: "tddy_daemon_rpc::watch_wake",
                    "waking {} failed, its events stay waiting: {e}",
                    owner.session_id
                );
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tddy_session_activity::project_notification_log::ProjectNotificationRecord;
    use tddy_session_activity::session_notifications::{
        SessionNotificationKind, SessionNotificationSource,
    };
    use tddy_session_activity::session_watches::{WatchEnd, WatchEndReason};

    use super::*;

    fn a_permission_prompt(request_id: &str) -> ProjectNotificationRecord {
        ProjectNotificationRecord {
            seq: 7,
            session_id: "s-1".to_string(),
            label: "fix-login".to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::PendingQuestion,
            text: "🔔 Session fix-login wants to run Bash: cargo test".to_string(),
            agent: None,
            question: Some(tddy_daemon_kernel::session_notification::QuestionNotice {
                request_id: request_id.to_string(),
                kind: "permission".to_string(),
                tool_name: "Bash".to_string(),
                summary: "cargo test".to_string(),
                questions: Vec::new(),
            }),
            ended: None,
            activity_status: None,
            at_unix_ms: 1_790_000_000_000,
            orchestrator_session_id: None,
        }
    }

    #[test]
    fn idle_is_done_waiting_started_or_ended_with_a_live_terminal() {
        let idle: Vec<bool> = [
            Some("Done"),
            Some("WaitingForInput"),
            Some("Started"),
            Some("Ended"),
            None,
        ]
        .into_iter()
        .map(|status| orchestrator_is_idle(status, false, true))
        .collect();
        assert_eq!(idle, vec![true; 5]);
    }

    #[test]
    fn running_executing_a_pending_question_or_no_terminal_is_not_idle() {
        assert_eq!(
            [
                orchestrator_is_idle(Some("Running"), false, true),
                orchestrator_is_idle(Some("ExecutingTool"), false, true),
                orchestrator_is_idle(Some("WaitingForInput"), true, true),
                orchestrator_is_idle(Some("Done"), false, false),
            ],
            [false; 4]
        );
    }

    #[test]
    fn the_wake_text_lists_each_event_and_its_request_id() {
        let text = wake_text(&WatchDelivery {
            events: vec![a_permission_prompt("req-1"), a_permission_prompt("req-2")],
            lost: 0,
            ended: Vec::new(),
        });
        assert_eq!(
            (
                text.starts_with("[tddy watch] 2 watched events"),
                text.contains("fix-login"),
                text.contains("req-1"),
                text.contains("req-2"),
            ),
            (true, true, true, true),
            "{text}"
        );
    }

    #[test]
    fn the_wake_text_reports_lost_and_ended_watches() {
        let text = wake_text(&WatchDelivery {
            events: vec![a_permission_prompt("req-1")],
            lost: 3,
            ended: vec![WatchEnd {
                watch_id: "w-9".to_string(),
                reason: WatchEndReason::Expired,
            }],
        });
        assert_eq!(
            (
                text.contains("3 lost"),
                text.contains("w-9"),
                text.contains("expired")
            ),
            (true, true, true),
            "{text}"
        );
    }

    #[tokio::test]
    async fn a_trigger_never_waits_on_the_waker() {
        // Given a trigger whose waker never reads, with its channel already full
        let (trigger, _cues) = WakeTrigger::channel();
        let notification = SessionNotification {
            session_id: "s-1".to_string(),
            os_user: "ada".to_string(),
            label: "fix-login".to_string(),
            kind: SessionNotificationKind::AttentionRequired,
            source: SessionNotificationSource::ActivityStatus,
            text: String::new(),
            agent: None,
            question: None,
            ended: None,
            activity_status: Some("Done".to_string()),
            at_unix_ms: 1_790_000_000_000,
        };
        for _ in 0..WAKE_CUE_CAPACITY {
            trigger.deliver(&notification).await.unwrap();
        }

        // When one more is delivered, it returns at once, and the trigger wanted it
        let delivered =
            tokio::time::timeout(Duration::from_millis(100), trigger.deliver(&notification)).await;
        assert_eq!(
            (delivered.is_ok(), trigger.wants(&notification)),
            (true, true)
        );
    }
}
