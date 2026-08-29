//! What a claude-cli / cursor session's agent is doing, inferred from that session's own
//! conversation.
//!
//! Product contract: docs/ft/daemon/agent-session-status.md. The fields are
//! `SessionEntry.agent_status` and `SessionEntry.last_activity`, and they ride `ListSessions` —
//! there is no second stream to correlate a status against a row.
//!
//! Nothing here is a new *source*. A session already writes its conversation twice over — the
//! persisted ACP transcript and the durable agent-activity log, resolved together by
//! [`acp_replay::read_session_transcript`] — and the daemon already broadcasts each activity record
//! as it is recorded. This module holds one **observed signal** per session (the newest thing it was
//! seen doing), seeded once from those files and kept current by a hub subscription, and applies the
//! rules that turn that signal plus the session's hook word into a status.
//!
//! Two properties this module exists to hold:
//!
//! - **One mapper.** A live activity record is mapped to its ACP frame first and read by the same
//!   function a replayed frame is, so a row cannot word the same call differently from the replay
//!   of it — which would surface as a session that rewords its own status when the daemon restarts.
//! - **Nothing is persisted**, for the reason [`crate::session_agent_status`] already gives: a
//!   status read back from disk claims a tool call is in flight in a process that never started
//!   one. A restarted daemon reports `UNSPECIFIED` until it has re-read a transcript.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tddy_core::agent_activity::AgentActivityRecord;
use tddy_core::session_activity::SessionActivityStatus;
use tddy_service::acp_replay;
use tddy_service::proto::acp::{acp_agent_message, content_block, session_update};
use tddy_service::proto::acp::{AcpAgentMessage, ToolCallStatus};
use tddy_service::proto::connection::{AgentCloneState, SessionAgentStatus};

use crate::connection_service::AgentActivityHub;
use crate::session_agent_status::{
    agent_status, now_unix_ms, truncate_summary, AgentActivity, ManagedAgentState,
};

/// One transcript frame as an observed signal, or `None` when the frame says nothing about what the
/// agent is doing.
///
/// A tool call still `PENDING`/`IN_PROGRESS` is [`ManagedAgentState::ExecutingTool`] titled by the
/// call; a `COMPLETED`/`FAILED` one is [`ManagedAgentState::Prompting`] with the same title, because
/// the tool returned and the turn that made it did not. The title is the transcript's own enriched
/// one (`Read main.rs`), which is the only place the *name* of a call comes from.
#[must_use]
pub fn activity_from_frame(frame: &AcpAgentMessage) -> Option<AgentActivity> {
    let Some(acp_agent_message::Msg::SessionUpdate(notification)) = &frame.msg else {
        return None;
    };
    let at_unix_ms = u64::try_from(notification.timestamp_unix_ms).unwrap_or_default();
    match notification.update.as_ref()?.update.as_ref()? {
        session_update::Update::ToolCall(call) => Some(AgentActivity {
            state: match ToolCallStatus::try_from(call.status) {
                Ok(ToolCallStatus::Completed | ToolCallStatus::Failed) => {
                    ManagedAgentState::Prompting
                }
                // Pending, in progress, or a status this build does not know: the call has not
                // returned, so the agent's loop is inside it.
                _ => ManagedAgentState::ExecutingTool,
            },
            summary: call.title.clone(),
            at_unix_ms,
        }),
        session_update::Update::AgentMessageChunk(chunk) => {
            match chunk.content.as_ref()?.block.as_ref()? {
                content_block::Block::Text(text) => Some(AgentActivity {
                    state: ManagedAgentState::Prompting,
                    summary: text.text.clone(),
                    at_unix_ms,
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// One durable activity record as an observed signal.
///
/// Mapped **through** [`acp_replay::frame_for_agent_activity`] — the builder the replay uses — and
/// into [`activity_from_frame`]. A second mapper here would let a live row and its replayed
/// counterpart disagree about the same call, and the disagreement would show as a row that rewords
/// itself when the daemon restarts and re-reads the file.
#[must_use]
pub fn activity_from_record(record: &AgentActivityRecord) -> AgentActivity {
    let frame = acp_replay::frame_for_agent_activity(record);
    activity_from_frame(&frame)
        .expect("frame_for_agent_activity always builds a tool_call frame, which is an observation")
}

/// The activity a session reports, from its hook word and the newest thing observed on it.
///
/// The rules, in the order they are applied (docs/ft/daemon/agent-session-status.md § How a signal
/// becomes a status):
///
/// 1. A hook word of `Done` or `Ended` **wins outright**. A session whose agent has stopped cannot
///    still be inside a tool call, and a `running` row whose terminal record never arrived would
///    otherwise pin the badge at `EXECUTING_TOOL` for the rest of the session's life.
/// 2. Otherwise a tool call in flight outranks the hook word. `ExecutingTool` is strictly more
///    precise than the hook's `Running`, and it carries the call's name.
/// 3. Otherwise the hook word decides, through the bridge the roster already owns — keeping the
///    observed summary, because what an agent was last seen doing is the useful thing on its row.
/// 4. With no hook word at all the observed signal decides alone: the cursor case, and the
///    claude-cli case before the first hook fires.
/// 5. Nothing observed and no hook word is no activity, never an idle one — "attached and ready" is
///    a claim, and this daemon has no grounds for it.
#[must_use]
pub fn inferred_activity(
    hook_status: Option<SessionActivityStatus>,
    observed: Option<&AgentActivity>,
) -> Option<AgentActivity> {
    match (hook_status, observed) {
        (None, None) => None,
        (None, Some(observed)) => Some(observed.clone()),
        // A hook word with nothing observed is a state and nothing to display: a summary-less
        // activity renders as no `last_activity` at all rather than a bare timestamp.
        (Some(hook_status), None) => Some(AgentActivity {
            state: ManagedAgentState::from_activity_status(hook_status),
            summary: String::new(),
            at_unix_ms: now_unix_ms(),
        }),
        (Some(hook_status), Some(observed)) => {
            let stopped = matches!(
                hook_status,
                SessionActivityStatus::Done | SessionActivityStatus::Ended
            );
            let in_flight = observed.state == ManagedAgentState::ExecutingTool;
            let state = match (stopped, in_flight) {
                (true, _) | (false, false) => ManagedAgentState::from_activity_status(hook_status),
                (false, true) => ManagedAgentState::ExecutingTool,
            };
            Some(AgentActivity {
                state,
                summary: observed.summary.clone(),
                at_unix_ms: observed.at_unix_ms,
            })
        }
    }
}

/// The status a session entry reports for its agent.
///
/// The conversation half of [`agent_status`], asked with [`AgentCloneState::Local`]: the roster's
/// clone precedence is about an agent's provisioned checkout, and a session has no such checkout to
/// outrank what its own conversation says.
#[must_use]
pub fn session_agent_status(activity: Option<&AgentActivity>) -> SessionAgentStatus {
    agent_status(AgentCloneState::Local, activity)
}

/// The newest signal observed on each agent session, plus the sessions already being tailed.
///
/// Keyed by session id alone, unlike [`crate::session_agent_status::SessionAgentActivityStore`]: a
/// session is one conversation, and there is no second agent on it to confuse the signal with.
///
/// A `std::sync::Mutex` rather than tokio's, because every operation is a map access with nothing
/// awaited under it.
#[derive(Default)]
pub struct SessionAgentInferenceStore {
    observed: Mutex<HashMap<String, AgentActivity>>,
    tailing: Mutex<HashSet<String>>,
}

impl SessionAgentInferenceStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what a session was just seen doing. Last write wins: the signals arrive in the order
    /// the session produced them, so the most recent is the most recent thing observed.
    ///
    /// The summary is truncated on the way in, by the store rule the roster already applies —
    /// a transcript title is agent-authored text of unbounded length, and `ListSessions` is a
    /// response an operator's dashboard polls.
    pub fn observe(&self, session_id: &str, activity: AgentActivity) {
        self.observed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(session_id.to_string(), truncated(activity));
    }

    /// What `session_id` was last observed doing, or `None` when nothing has been observed.
    #[must_use]
    pub fn latest(&self, session_id: &str) -> Option<AgentActivity> {
        self.observed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(session_id)
            .cloned()
    }

    /// Seed the newest signal from what the session has already written.
    ///
    /// Reads [`acp_replay::read_session_transcript`] — the same resolved view `StreamAcpReplay`
    /// replays, so a daemon-hosted session that writes only `agent-activity.jsonl` seeds too — and
    /// takes the newest frame that says anything. An absent transcript is not an error: it is a
    /// session that has written nothing yet.
    ///
    /// Recorded **only when nothing has been observed**. A record that lands while the file is being
    /// read is already the newer fact, and must not be overwritten by what was on disk before it.
    pub fn seed_from_transcript(&self, session_id: &str, session_dir: &Path) -> io::Result<()> {
        let frames = acp_replay::read_session_transcript(session_dir)?;
        let Some(seed) = frames.iter().rev().find_map(activity_from_frame) else {
            return Ok(());
        };
        self.observed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(session_id.to_string())
            .or_insert_with(|| truncated(seed));
        Ok(())
    }

    /// Forget a session entirely — what deleting it does.
    ///
    /// Drops the tailing mark as well as the signal, so the consumer task notices on its next record
    /// and exits instead of holding a subscription for the daemon's life.
    pub fn forget(&self, session_id: &str) {
        self.observed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id);
        self.tailing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id);
    }

    /// Start following `session_id`'s conversation, unless it is already being followed.
    ///
    /// Idempotent because a listing calls it for every agent session it returns: a second call must
    /// cost nothing, or every poll would re-read a transcript per session.
    ///
    /// The two halves are done in the order that matters — **subscribe first**, then seed. A record
    /// published while the file is being read is then already recorded, and the seed, which only
    /// writes when nothing has been observed, leaves it alone.
    pub fn ensure_tailing(
        self: &Arc<Self>,
        hub: &Arc<AgentActivityHub>,
        session_id: &str,
        session_dir: &Path,
    ) {
        if !self.mark_tailing(session_id) {
            return;
        }
        let mut records = hub.subscribe(session_id);
        let store = Arc::clone(self);
        let tailed_session = session_id.to_string();
        tokio::spawn(async move {
            loop {
                match records.recv().await {
                    Ok(record) => {
                        if !store.is_tailing(&tailed_session) {
                            break;
                        }
                        store.observe(&tailed_session, activity_from_record(&record));
                    }
                    // Lagged means records were dropped between polls; the next one is newer than
                    // anything missed, which is all this store holds.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        if let Err(e) = self.seed_from_transcript(session_id, session_dir) {
            log::warn!(
                target: "tddy_daemon::session_agent_inference",
                "could not seed agent status for session {} from {}: {}",
                session_id,
                session_dir.display(),
                e
            );
        }
    }

    /// Claim `session_id` as tailed, returning whether this call is the one that claimed it.
    fn mark_tailing(&self, session_id: &str) -> bool {
        self.tailing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(session_id.to_string())
    }

    fn is_tailing(&self, session_id: &str) -> bool {
        self.tailing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(session_id)
    }
}

/// The same activity with its summary cut to one line of the roster's length.
fn truncated(activity: AgentActivity) -> AgentActivity {
    AgentActivity {
        summary: truncate_summary(&activity.summary),
        ..activity
    }
}
