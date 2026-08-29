//! What a roster agent is *doing*, and the mapping that turns this daemon's existing signals into
//! the status a roster snapshot carries.
//!
//! Product contract: docs/ft/daemon/session-agent-roster.md. The field itself is
//! `SessionAgentEntry.status`, and it rides the whole-snapshot `StreamSessionAgents` stream — there
//! is no status RPC, for the reason the proto comment gives.
//!
//! Two properties this module exists to hold:
//!
//! - **Nothing here is persisted.** A status is a fact about a running turn loop. Written to
//!   `.session.yaml` and read back, it would claim a turn is in flight in a process that never
//!   started one, and the main agent would wait for an answer nothing is producing. So a restarted
//!   daemon reports `UNSPECIFIED` for every entry until a signal reaches it, which is the honest
//!   answer.
//! - **A checkout that cannot serve a prompt outranks whatever the conversation says.** An agent
//!   whose clone is still provisioning *refuses* prompts (`refuse_unready_clone` in
//!   [`crate::connection_service`]). Reporting it `IDLE` because no turn is in flight would offer
//!   the operator an agent that cannot answer, so `CONNECTING` and `ERROR` are read off the clone
//!   before the conversation is consulted at all.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use tddy_core::session_activity::SessionActivityStatus;
use tddy_service::proto::connection::{AgentCloneState, SessionAgentActivity, SessionAgentStatus};

/// How much of a summary line a roster entry carries.
///
/// Truncated here rather than by each reader: the summary rides a broadcast whose frames are whole
/// rosters, and an un-truncated prompt on every entry is how a snapshot crosses
/// `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES` — past which a lost chunk frame wedges the call
/// with no error at all.
const SUMMARY_MAX_CHARS: usize = 120;

/// What this daemon knows the conversation with one roster agent to be doing.
///
/// Deliberately not the same type as [`SessionActivityStatus`]: that enum is the *session's* hook
/// vocabulary, keyed by nothing finer than the session, and an agent roster needs a per-agent
/// answer. [`ManagedAgentState::from_activity_status`] bridges the two where a hook-shaped signal
/// does reach an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ManagedAgentState {
    /// No conversation is open with this agent. Attached, asked nothing.
    #[default]
    NoConversation,
    /// A conversation is open with no turn in flight — it has answered everything asked.
    Open,
    /// A turn is in flight: prompted, no stop reason yet.
    Prompting,
    /// The agent's own loop is inside a tool call. A refinement of [`Self::Prompting`].
    ExecutingTool,
    /// The agent is blocked on an answer only a human can give.
    WaitingForInput,
}

impl ManagedAgentState {
    /// The hook vocabulary as an agent state, for a signal that arrives shaped like a session hook.
    ///
    /// `Started` and `Done` both land on [`Self::Open`]: an agent that has just been opened and one
    /// that has finished its turn are the same thing to a reader — available, nothing in flight.
    /// `Ended` is [`Self::NoConversation`] rather than `Open`, because a conversation that ended
    /// cannot be prompted without opening a new one.
    #[must_use]
    pub fn from_activity_status(status: SessionActivityStatus) -> Self {
        match status {
            SessionActivityStatus::Started | SessionActivityStatus::Done => Self::Open,
            SessionActivityStatus::Running => Self::Prompting,
            SessionActivityStatus::ExecutingTool => Self::ExecutingTool,
            SessionActivityStatus::WaitingForInput => Self::WaitingForInput,
            SessionActivityStatus::Ended => Self::NoConversation,
        }
    }
}

/// The last thing this daemon observed one agent doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentActivity {
    pub state: ManagedAgentState,
    /// One short line, already truncated to [`SUMMARY_MAX_CHARS`].
    pub summary: String,
    /// Unix milliseconds. Never 0 on a record this module built.
    pub at_unix_ms: u64,
}

impl AgentActivity {
    /// This activity as the wire message, or `None` when there is no summary to show.
    ///
    /// An activity with an empty summary produces no message rather than one carrying a bare
    /// timestamp: a reader renders the summary, and a timestamp with nothing beside it shows as an
    /// agent that did something unnameable just now.
    #[must_use]
    pub fn to_proto(&self) -> Option<SessionAgentActivity> {
        match self.summary.is_empty() {
            true => None,
            false => Some(SessionAgentActivity {
                at_unix_ms: self.at_unix_ms,
                summary: self.summary.clone(),
            }),
        }
    }
}

/// The status a roster entry reports, from the two signals that decide it.
///
/// The clone is read first and wins outright, because an agent whose checkout is not ready refuses
/// prompts however idle its conversation looks. `UNSPECIFIED` from the clone store is treated as
/// `CONNECTING` for the same reason `roster_entry` refuses to call it READY: it is the state of a
/// remote entry restored from disk whose checkout nothing in this process has measured, and an
/// unmeasured checkout is one no prompt may be served from.
///
/// With the checkout out of the way, `activity` decides. `None` — no signal has reached this daemon
/// — is `UNSPECIFIED`, never `IDLE`: "attached and ready" is a claim, and this daemon has no
/// grounds for it.
#[must_use]
pub fn agent_status(
    clone_state: AgentCloneState,
    activity: Option<&AgentActivity>,
) -> SessionAgentStatus {
    match clone_state {
        AgentCloneState::Error => return SessionAgentStatus::Error,
        AgentCloneState::Provisioning | AgentCloneState::Unspecified => {
            return SessionAgentStatus::Connecting
        }
        AgentCloneState::Local | AgentCloneState::Ready => {}
    }
    match activity.map(|a| a.state) {
        None => SessionAgentStatus::Unspecified,
        Some(ManagedAgentState::NoConversation | ManagedAgentState::Open) => {
            SessionAgentStatus::Idle
        }
        Some(ManagedAgentState::Prompting) => SessionAgentStatus::Running,
        Some(ManagedAgentState::ExecutingTool) => SessionAgentStatus::ExecutingTool,
        Some(ManagedAgentState::WaitingForInput) => SessionAgentStatus::WaitingForInput,
    }
}

/// Every roster agent's activity this process has observed, keyed by `(session_id, agent_id)`.
///
/// Beside [`crate::session_agent_clone::SessionAgentCloneStore`] and read at the same moment: a
/// snapshot asks both, so an entry reports the checkout actually serving it *and* the conversation
/// actually running on it.
///
/// Keyed by the pair, never by `agent_id` alone: one def attached to two sessions is one
/// `agent_id`, and a turn on one session would show as a turn on the other.
///
/// A `std::sync::Mutex` rather than tokio's, because every operation is a map write with nothing
/// awaited under it — and because the roster's snapshot path is already synchronous.
#[derive(Default)]
pub struct SessionAgentActivityStore {
    entries: Mutex<HashMap<(String, String), AgentActivity>>,
}

impl SessionAgentActivityStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what an agent is doing now, stamped with the current time.
    ///
    /// Last write wins: the signals arrive from this daemon's own conversation handlers, so the
    /// most recent one is by construction the most recent thing observed.
    pub fn record(
        &self,
        session_id: &str,
        agent_id: &str,
        state: ManagedAgentState,
        summary: impl AsRef<str>,
    ) {
        let activity = AgentActivity {
            state,
            summary: truncate_summary(summary.as_ref()),
            at_unix_ms: now_unix_ms(),
        };
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((session_id.to_string(), agent_id.to_string()), activity);
    }

    /// Put an agent back to [`ManagedAgentState::Open`], but only if it is still mid-turn.
    ///
    /// Returns whether anything changed, so a caller can skip republishing a roster that did not
    /// move.
    ///
    /// The guard is the point. A turn's end is observed from a spawned task that outlives the
    /// handler, and by then a cancel or a detach may already have moved the agent on. An
    /// unconditional write would resurrect a conversation that is gone: an agent whose conversation
    /// was cancelled would go back to reporting one it can no longer be prompted through, and a
    /// detached agent would reappear in a store the detach had just emptied.
    pub fn record_turn_end(
        &self,
        session_id: &str,
        agent_id: &str,
        summary: impl AsRef<str>,
    ) -> bool {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let Some(activity) = entries.get_mut(&(session_id.to_string(), agent_id.to_string()))
        else {
            return false;
        };
        if !matches!(
            activity.state,
            ManagedAgentState::Prompting | ManagedAgentState::ExecutingTool
        ) {
            return false;
        }
        activity.state = ManagedAgentState::Open;
        activity.summary = truncate_summary(summary.as_ref());
        activity.at_unix_ms = now_unix_ms();
        true
    }

    /// What `agent_id` was last observed doing, or `None` when nothing has been observed.
    #[must_use]
    pub fn get(&self, session_id: &str, agent_id: &str) -> Option<AgentActivity> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(session_id.to_string(), agent_id.to_string()))
            .cloned()
    }

    /// Forget an agent entirely — what a detach does.
    ///
    /// Forgotten rather than set to [`ManagedAgentState::NoConversation`]: a detached agent is off
    /// the roster, and leaving a record behind would have a re-attach show the previous
    /// attachment's last activity as if it were this one's.
    pub fn forget(&self, session_id: &str, agent_id: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(session_id.to_string(), agent_id.to_string()));
    }

    /// Forget every agent of one session — what deleting the session does.
    pub fn forget_session(&self, session_id: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(session, _), _| session != session_id);
    }
}

/// A tool call as one summary line: the tool's name, plus the one argument naming what it acted on.
///
/// Only the argument a reader would recognise the call by is carried — a path, a pattern, a command.
/// The whole argument object is deliberately not: a `Write`'s `content` is the file, and putting it
/// on a roster row that rides a broadcast is how a snapshot crosses the chunk-framing limit.
#[must_use]
pub fn tool_call_summary(tool_name: &str, args: &serde_json::Value) -> String {
    const NAMING_ARGS: [&str; 5] = ["file_path", "path", "pattern", "command", "query"];
    let subject = NAMING_ARGS
        .iter()
        .find_map(|key| args.get(key).and_then(serde_json::Value::as_str))
        .unwrap_or_default();
    match subject.is_empty() {
        true => tool_name.to_string(),
        false => format!("{tool_name} {subject}"),
    }
}

/// One line, cut to [`SUMMARY_MAX_CHARS`] characters (not bytes — a cut mid-codepoint panics).
///
/// Whitespace collapses to single spaces: the consumers render the summary on one row, and a raw
/// newline there truncates the line at the reader instead, hiding the rest without saying so.
fn truncate_summary(raw: &str) -> String {
    let flattened = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    match flattened.chars().count() > SUMMARY_MAX_CHARS {
        false => flattened,
        true => {
            let kept: String = flattened.chars().take(SUMMARY_MAX_CHARS - 1).collect();
            format!("{kept}…")
        }
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn an_activity(state: ManagedAgentState) -> AgentActivity {
        AgentActivity {
            state,
            summary: "prompted: summarise the diff".to_string(),
            at_unix_ms: 1_780_828_020_298,
        }
    }

    // ─── the clone outranks the conversation ────────────────────────────────────────────────────

    #[test]
    fn an_agent_whose_clone_is_still_being_built_is_connecting_however_idle_its_conversation_is() {
        // Given a clone mid-provision and a conversation that has answered everything asked
        let activity = an_activity(ManagedAgentState::Open);

        // When
        let status = agent_status(AgentCloneState::Provisioning, Some(&activity));

        // Then — reporting IDLE would offer an agent whose next prompt is refused
        assert_eq!(status, SessionAgentStatus::Connecting);
    }

    #[test]
    fn an_agent_whose_clone_failed_is_in_error_however_idle_its_conversation_is() {
        let activity = an_activity(ManagedAgentState::Open);

        let status = agent_status(AgentCloneState::Error, Some(&activity));

        assert_eq!(status, SessionAgentStatus::Error);
    }

    #[test]
    fn a_remote_agent_whose_clone_this_process_never_measured_is_connecting_not_idle() {
        // Given the shape of a roster restored from `.session.yaml`: the entry survived the
        // restart, the clone did not
        let status = agent_status(AgentCloneState::Unspecified, None);

        // Then — an unmeasured checkout is one no prompt may be served from
        assert_eq!(status, SessionAgentStatus::Connecting);
    }

    // ─── with a usable checkout, the conversation decides ───────────────────────────────────────

    #[test]
    fn a_local_agent_nothing_has_been_observed_of_reports_nothing_rather_than_idle() {
        // Given no signal has reached this daemon — a roster read back after a restart
        let status = agent_status(AgentCloneState::Local, None);

        // Then — "attached and ready" is a claim this daemon has no grounds for
        assert_eq!(status, SessionAgentStatus::Unspecified);
    }

    #[test]
    fn a_local_agent_with_no_conversation_open_is_idle() {
        let activity = an_activity(ManagedAgentState::NoConversation);

        let status = agent_status(AgentCloneState::Local, Some(&activity));

        assert_eq!(status, SessionAgentStatus::Idle);
    }

    #[test]
    fn a_local_agent_between_turns_is_idle() {
        let activity = an_activity(ManagedAgentState::Open);

        let status = agent_status(AgentCloneState::Local, Some(&activity));

        assert_eq!(status, SessionAgentStatus::Idle);
    }

    #[test]
    fn a_local_agent_with_a_turn_in_flight_is_running() {
        let activity = an_activity(ManagedAgentState::Prompting);

        let status = agent_status(AgentCloneState::Local, Some(&activity));

        assert_eq!(status, SessionAgentStatus::Running);
    }

    #[test]
    fn a_local_agent_inside_a_tool_call_is_executing_tool_rather_than_merely_running() {
        let activity = an_activity(ManagedAgentState::ExecutingTool);

        let status = agent_status(AgentCloneState::Local, Some(&activity));

        assert_eq!(status, SessionAgentStatus::ExecutingTool);
    }

    #[test]
    fn a_local_agent_blocked_on_a_human_is_waiting_for_input() {
        let activity = an_activity(ManagedAgentState::WaitingForInput);

        let status = agent_status(AgentCloneState::Local, Some(&activity));

        assert_eq!(status, SessionAgentStatus::WaitingForInput);
    }

    #[test]
    fn a_remote_agent_on_a_ready_clone_reports_its_conversation_exactly_as_a_local_one_does() {
        // The clone is what a remote agent has and a local one does not; once it is READY the two
        // are the same question.
        let activity = an_activity(ManagedAgentState::Prompting);

        let status = agent_status(AgentCloneState::Ready, Some(&activity));

        assert_eq!(status, SessionAgentStatus::Running);
    }

    // ─── the hook vocabulary ────────────────────────────────────────────────────────────────────

    #[test]
    fn the_hook_vocabulary_maps_onto_agent_states() {
        assert_eq!(
            ManagedAgentState::from_activity_status(SessionActivityStatus::Started),
            ManagedAgentState::Open
        );
        assert_eq!(
            ManagedAgentState::from_activity_status(SessionActivityStatus::Done),
            ManagedAgentState::Open,
            "a finished turn and a fresh conversation are the same thing to a reader"
        );
        assert_eq!(
            ManagedAgentState::from_activity_status(SessionActivityStatus::Running),
            ManagedAgentState::Prompting
        );
        assert_eq!(
            ManagedAgentState::from_activity_status(SessionActivityStatus::ExecutingTool),
            ManagedAgentState::ExecutingTool
        );
        assert_eq!(
            ManagedAgentState::from_activity_status(SessionActivityStatus::WaitingForInput),
            ManagedAgentState::WaitingForInput
        );
        assert_eq!(
            ManagedAgentState::from_activity_status(SessionActivityStatus::Ended),
            ManagedAgentState::NoConversation,
            "an ended conversation cannot be prompted without opening a new one"
        );
    }

    // ─── the store ──────────────────────────────────────────────────────────────────────────────

    #[test]
    fn the_store_answers_per_agent_not_per_session() {
        // Given two agents on one session, doing different things
        let store = SessionAgentActivityStore::new();
        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "prompted: find the caller",
        );
        store.record(
            "session-1",
            "reviewer@ws-01",
            ManagedAgentState::Open,
            "answered",
        );

        // Then
        assert_eq!(
            store.get("session-1", "explorer@ws-01").map(|a| a.state),
            Some(ManagedAgentState::Prompting)
        );
        assert_eq!(
            store.get("session-1", "reviewer@ws-01").map(|a| a.state),
            Some(ManagedAgentState::Open)
        );
    }

    #[test]
    fn the_same_agent_id_on_two_sessions_is_two_records() {
        // One def attached to two sessions is one `agent_id` — keying on it alone would have a turn
        // on one session show as a turn on the other.
        let store = SessionAgentActivityStore::new();
        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "prompted",
        );

        assert_eq!(store.get("session-2", "explorer@ws-01"), None);
    }

    #[test]
    fn a_detached_agent_is_forgotten_rather_than_left_reporting_its_last_turn() {
        // Given
        let store = SessionAgentActivityStore::new();
        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "prompted",
        );

        // When
        store.forget("session-1", "explorer@ws-01");

        // Then — a re-attach must not inherit the previous attachment's activity
        assert_eq!(store.get("session-1", "explorer@ws-01"), None);
    }

    #[test]
    fn deleting_a_session_forgets_its_agents_and_leaves_every_other_session_alone() {
        // Given
        let store = SessionAgentActivityStore::new();
        store.record("session-1", "explorer@ws-01", ManagedAgentState::Open, "a");
        store.record("session-2", "explorer@ws-01", ManagedAgentState::Open, "b");

        // When
        store.forget_session("session-1");

        // Then
        assert_eq!(store.get("session-1", "explorer@ws-01"), None);
        assert!(store.get("session-2", "explorer@ws-01").is_some());
    }

    #[test]
    fn a_recorded_activity_is_stamped_with_a_real_time() {
        let store = SessionAgentActivityStore::new();

        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Open,
            "done",
        );

        let at = store
            .get("session-1", "explorer@ws-01")
            .expect("the activity just recorded")
            .at_unix_ms;
        // A summary with no time behind it reads as current forever.
        assert!(at > 1_700_000_000_000, "stamped with {at}");
    }

    // ─── a turn ending ──────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_turn_that_ends_puts_its_agent_back_to_open() {
        // Given a turn in flight
        let store = SessionAgentActivityStore::new();
        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "prompted: find the caller",
        );

        // When
        let changed = store.record_turn_end("session-1", "explorer@ws-01", "answered (412 chars)");

        // Then
        assert!(changed);
        let activity = store
            .get("session-1", "explorer@ws-01")
            .expect("the record");
        assert_eq!(activity.state, ManagedAgentState::Open);
        assert_eq!(activity.summary, "answered (412 chars)");
    }

    #[test]
    fn a_turn_that_ends_inside_a_tool_call_still_puts_its_agent_back_to_open() {
        // EXECUTING_TOOL is a refinement of RUNNING, so a turn can end from it.
        let store = SessionAgentActivityStore::new();
        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::ExecutingTool,
            "Read src/main.rs",
        );

        assert!(store.record_turn_end("session-1", "explorer@ws-01", "answered"));
        assert_eq!(
            store.get("session-1", "explorer@ws-01").map(|a| a.state),
            Some(ManagedAgentState::Open)
        );
    }

    #[test]
    fn a_turn_ending_after_its_conversation_was_cancelled_does_not_resurrect_it() {
        // Given a cancel that landed while the turn was still in flight — the ordinary race, since
        // the end of a turn is observed from a task that outlives the handler
        let store = SessionAgentActivityStore::new();
        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "prompted",
        );
        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::NoConversation,
            "conversation cancelled",
        );

        // When
        let changed = store.record_turn_end("session-1", "explorer@ws-01", "answered");

        // Then — the agent would otherwise report a conversation it can no longer be prompted
        // through
        assert!(!changed);
        assert_eq!(
            store.get("session-1", "explorer@ws-01").map(|a| a.summary),
            Some("conversation cancelled".to_string())
        );
    }

    #[test]
    fn a_turn_ending_after_its_agent_was_detached_does_not_reappear_in_the_store() {
        let store = SessionAgentActivityStore::new();
        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "prompted",
        );
        store.forget("session-1", "explorer@ws-01");

        assert!(!store.record_turn_end("session-1", "explorer@ws-01", "answered"));
        assert_eq!(store.get("session-1", "explorer@ws-01"), None);
    }

    // ─── the summary ────────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_long_prompt_is_cut_before_it_rides_the_roster_broadcast() {
        let store = SessionAgentActivityStore::new();

        store.record(
            "session-1",
            "explorer@ws-01",
            ManagedAgentState::Prompting,
            "x".repeat(500),
        );

        let summary = store
            .get("session-1", "explorer@ws-01")
            .expect("the activity")
            .summary;
        assert_eq!(summary.chars().count(), SUMMARY_MAX_CHARS);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn a_multi_line_prompt_becomes_one_line() {
        // A raw newline truncates the line at the reader instead, hiding the rest without saying so.
        assert_eq!(
            truncate_summary("prompted: summarise\n  the diff\n"),
            "prompted: summarise the diff"
        );
    }

    #[test]
    fn a_summary_is_cut_on_characters_so_a_multibyte_prompt_does_not_panic() {
        let cut = truncate_summary(&"é".repeat(500));

        assert_eq!(cut.chars().count(), SUMMARY_MAX_CHARS);
    }

    // ─── tool-call summaries ────────────────────────────────────────────────────────────────────

    #[test]
    fn a_tool_call_is_summarised_by_the_argument_that_names_what_it_acted_on() {
        assert_eq!(
            tool_call_summary("Read", &serde_json::json!({ "file_path": "src/main.rs" })),
            "Read src/main.rs"
        );
        assert_eq!(
            tool_call_summary("Grep", &serde_json::json!({ "pattern": "fn main" })),
            "Grep fn main"
        );
    }

    #[test]
    fn a_tool_call_with_no_recognisable_subject_is_summarised_by_its_name_alone() {
        assert_eq!(
            tool_call_summary("Glob", &serde_json::json!({ "limit": 20 })),
            "Glob"
        );
    }

    #[test]
    fn a_tool_calls_payload_never_reaches_the_summary() {
        // A `Write`'s content is the whole file; on a roster row it is how a snapshot crosses the
        // chunk-framing limit, past which a lost frame wedges the call with no error.
        let summary = tool_call_summary(
            "Write",
            &serde_json::json!({ "file_path": "src/main.rs", "content": "x".repeat(9_000) }),
        );

        assert_eq!(summary, "Write src/main.rs");
    }

    #[test]
    fn an_activity_with_nothing_to_say_produces_no_wire_message() {
        // A timestamp with no summary beside it shows as an agent that did something unnameable
        // just now.
        let activity = AgentActivity {
            state: ManagedAgentState::Open,
            summary: String::new(),
            at_unix_ms: 1_780_828_020_298,
        };

        assert_eq!(activity.to_proto(), None);
    }

    #[test]
    fn an_activity_with_a_summary_carries_both_halves_onto_the_wire() {
        let activity = an_activity(ManagedAgentState::Prompting);

        let proto = activity.to_proto().expect("a populated activity");

        assert_eq!(proto.summary, "prompted: summarise the diff");
        assert_eq!(proto.at_unix_ms, 1_780_828_020_298);
    }
}
