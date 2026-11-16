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
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-16-idle-wake.md

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_session_activity::session_notifications::{
    SessionNotification, SessionNotificationSubscriber,
};
use tddy_session_activity::session_watches::WatchDelivery;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

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
        // TODO(idle-wake): implement — attention notifications and every status report.
        let _ = notification;
        false
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
    // TODO(idle-wake): implement
    let _ = (
        activity_status,
        waits_on_its_own_question,
        main_terminal_live,
    );
    false
}

/// The turn a wake sends: one line per event, oldest first, naming the session, the kind, what it
/// said and the `request_id` a question is answered by; then what was lost and which watches ended.
pub fn wake_text(delivery: &WatchDelivery) -> String {
    // TODO(idle-wake): implement
    let _ = delivery;
    String::new()
}

impl ExecToolRpcHandler {
    /// Start waking this host's idle watching orchestrators from `cues`, with `timings`.
    pub fn start_watch_waker(
        self: &Arc<Self>,
        cues: WakeCues,
        timings: WakeTimings,
    ) -> JoinHandle<()> {
        // TODO(idle-wake): implement — settle, gap, idle check, committed paste.
        let _ = timings;
        let WakeCues(mut cues) = cues;
        tokio::spawn(async move { while cues.recv().await.is_some() {} })
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
