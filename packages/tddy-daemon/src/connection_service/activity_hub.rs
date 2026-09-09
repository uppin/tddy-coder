use tddy_service::proto::connection::AgentActivityRecord as ProtoAgentActivityRecord;
use std::sync::Mutex as StdMutex;
use tokio::sync::broadcast::error::RecvError;

/// Live pub/sub hub for **agent activity** records, plus the per-session pending-call stack that
/// pairs a claude-cli `PreToolUse` (running) hook with its matching `PostToolUse` (terminal) hook.
///
/// Modeled on the per-session terminal-control broadcast ([`CliSessionManager::subscribe_control`]
/// / [`relay_control_events`]): [`subscribe`](AgentActivityHub::subscribe) hands out a
/// `broadcast::Receiver` for a session (creating the sender lazily) and
/// [`publish`](AgentActivityHub::publish) fans a record out to every current subscriber. The
/// durable `agent-activity.jsonl` log remains the source of truth — the hub only accelerates live
/// delivery, so publishing with no subscribers is a no-op.
#[derive(Default)]
pub struct AgentActivityHub {
    /// Per-session live broadcast; the sender is created lazily on first subscribe or publish.
    pub(crate) senders: StdMutex<
        std::collections::HashMap<
            String,
            tokio::sync::broadcast::Sender<tddy_core::agent_activity::AgentActivityRecord>,
        >,
    >,
    /// Per-session stack of in-flight `call_id`s awaiting their terminal (PostToolUse) row.
    pub(crate) pending: StdMutex<std::collections::HashMap<String, Vec<String>>>,
}

impl AgentActivityHub {
    /// Broadcast capacity per session. Sized so a burst of tool calls between a slow subscriber's
    /// polls rarely forces a `Lagged`; the relay tolerates `Lagged` regardless.
    pub(crate) const CHANNEL_CAPACITY: usize = 256;

    /// Subscribe to live records for `session_id`, creating the broadcast channel if absent.
    pub fn subscribe(
        &self,
        session_id: &str,
    ) -> tokio::sync::broadcast::Receiver<tddy_core::agent_activity::AgentActivityRecord> {
        let mut senders = self
            .senders
            .lock()
            .expect("agent activity hub mutex poisoned");
        let sender = senders
            .entry(session_id.to_string())
            .or_insert_with(|| tokio::sync::broadcast::channel(Self::CHANNEL_CAPACITY).0);
        sender.subscribe()
    }

    /// Publish a record to all live subscribers of `session_id`. A no-op when none are attached.
    pub fn publish(
        &self,
        session_id: &str,
        record: tddy_core::agent_activity::AgentActivityRecord,
    ) {
        let sender = {
            let senders = self
                .senders
                .lock()
                .expect("agent activity hub mutex poisoned");
            senders.get(session_id).cloned()
        };
        if let Some(sender) = sender {
            // Err = no live receivers; the durable log still holds the record, so ignore it.
            let _ = sender.send(record);
        }
    }

    /// Push an in-flight `call_id` onto the session's pending stack (a `PreToolUse` started a call).
    pub fn push_pending(&self, session_id: &str, call_id: &str) {
        let mut pending = self
            .pending
            .lock()
            .expect("agent activity hub mutex poisoned");
        pending
            .entry(session_id.to_string())
            .or_default()
            .push(call_id.to_string());
    }

    /// Pop the most-recent in-flight `call_id` for the session (its `PostToolUse` arrived). Returns
    /// `None` when no `PreToolUse` is outstanding, so the caller mints a fresh id instead.
    pub fn pop_pending(&self, session_id: &str) -> Option<String> {
        let mut pending = self
            .pending
            .lock()
            .expect("agent activity hub mutex poisoned");
        pending.get_mut(session_id).and_then(|stack| stack.pop())
    }
}

/// Relay task for `StreamSessionActivity`: forwards live agent-activity records for one session
/// (the broadcast is already session-scoped) from the hub into `tx` until the client disconnects.
pub(crate) async fn relay_agent_activity(
    mut broadcast_rx: tokio::sync::broadcast::Receiver<
        tddy_core::agent_activity::AgentActivityRecord,
    >,
    tx: tokio::sync::mpsc::UnboundedSender<ProtoAgentActivityRecord>,
) {
    loop {
        match broadcast_rx.recv().await {
            Ok(record) => {
                if tx
                    .send(tddy_service::agent_activity_to_proto(record))
                    .is_err()
                {
                    break;
                }
            }
            Err(RecvError::Lagged(_)) => {}
            Err(RecvError::Closed) => break,
        }
    }
}

/// Per-session QEMU demo VM lifecycle state.
pub(crate) enum DemoVmHandle {
    /// Boot has been requested; waiting for SSH port to become reachable.
    Booting,
    /// VM is up and accepting SSH connections.
    /// `share_url` is the first app port forward URL (e.g. "http://localhost:8080"), if any.
    Running {
        vm: tddy_vm::RunningVm,
        share_url: String,
    },
    /// Boot or shutdown failed.
    Error(String),
}
