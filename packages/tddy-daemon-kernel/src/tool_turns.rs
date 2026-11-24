//! The tool calls of a session's current turn, as two sources reported them: its hooks and its
//! own transcript. An agent whose transcript names no tool-call id (Cursor's) can only have its
//! two reports of one call matched by order: the Nth call of a tool one source reports this turn
//! is the Nth the other reports. A pair is how the transcript's text is placed around the calls the
//! hooks recorded, and how a call recorded from both is recorded once.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// One tool call as the source that reported it first recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenToolCall {
    pub tool_name: String,
    /// The id its rows carry.
    pub call_id: String,
    /// When that source recorded it starting.
    pub started_unix_ms: u64,
}

/// How many unpaired calls, and unreached turn ends, a session keeps: a session whose other source
/// never reports (its transcript is not followed) must not grow without bound. The oldest go first.
pub const UNPAIRED_REMEMBERED: usize = 256;

/// Which source reported a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCallSource {
    Hooks,
    Transcript,
}

#[derive(Debug, Default)]
struct ToolTurn {
    /// Calls one source reported that the other has not yet, oldest first.
    unpaired: VecDeque<(ToolCallSource, SeenToolCall)>,
    /// A hook-reported call id, by the id of the transcript call it was paired with first.
    adopted: HashMap<String, String>,
    /// When the hooks reported each turn's end that the transcript has not reached yet.
    hook_turn_ends: VecDeque<u64>,
}

/// The current turn's tool calls of every session.
#[derive(Debug, Default)]
pub struct ToolTurns {
    turns: Mutex<HashMap<String, ToolTurn>>,
}

impl ToolTurns {
    /// `source` reported `call` for `session_id`: the call the *other* source reported first for
    /// the same tool this turn, when there is one (and the two are now a pair); otherwise `call` is
    /// remembered for the other source to pair with.
    pub fn saw(
        &self,
        session_id: &str,
        source: ToolCallSource,
        call: SeenToolCall,
    ) -> Option<SeenToolCall> {
        let mut turns = self.turns.lock().unwrap_or_else(|p| p.into_inner());
        let turn = turns.entry(session_id.to_string()).or_default();
        let earlier = turn
            .unpaired
            .iter()
            .position(|(seen_by, seen)| *seen_by != source && seen.tool_name == call.tool_name);
        match earlier {
            Some(index) => {
                let (_, paired) = turn.unpaired.remove(index)?;
                if source == ToolCallSource::Hooks {
                    turn.adopted
                        .insert(call.call_id.clone(), paired.call_id.clone());
                }
                Some(paired)
            }
            None => {
                if turn.unpaired.len() == UNPAIRED_REMEMBERED {
                    turn.unpaired.pop_front();
                }
                turn.unpaired.push_back((source, call));
                None
            }
        }
    }

    /// The transcript call id a hook-reported `call_id` was paired with, if it was paired with one
    /// the transcript reported first.
    pub fn adopted_id(&self, session_id: &str, call_id: &str) -> Option<String> {
        self.turns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(session_id)
            .and_then(|turn| turn.adopted.get(call_id).cloned())
    }

    /// When the earliest call the hooks reported that the transcript has not reached yet started:
    /// whatever the transcript says next happened before it.
    pub fn earliest_unreached_hook_call(&self, session_id: &str) -> Option<u64> {
        self.turns
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(session_id)
            .and_then(|turn| {
                turn.unpaired
                    .iter()
                    .filter(|(source, _)| *source == ToolCallSource::Hooks)
                    .map(|(_, call)| call.started_unix_ms)
                    .min()
            })
    }

    /// The hooks reported the session's turn ending at `at_unix_ms`.
    pub fn hooks_turn_ended(&self, session_id: &str, at_unix_ms: u64) {
        let mut turns = self.turns.lock().unwrap_or_else(|p| p.into_inner());
        let turn = turns.entry(session_id.to_string()).or_default();
        if turn.hook_turn_ends.len() == UNPAIRED_REMEMBERED {
            turn.hook_turn_ends.pop_front();
        }
        turn.hook_turn_ends.push_back(at_unix_ms);
    }

    /// The transcript reached the session's turn's end: nothing either source reported in that turn
    /// pairs with a later turn's call. The hooks' calls of a later turn — reported before the
    /// transcript got here, when its follower is behind — are kept, told apart by the hooks' own
    /// report of the turn's end.
    pub fn transcript_turn_ended(&self, session_id: &str) {
        let mut turns = self.turns.lock().unwrap_or_else(|p| p.into_inner());
        let Some(turn) = turns.get_mut(session_id) else {
            return;
        };
        let hooks_ended_at = turn.hook_turn_ends.pop_front();
        turn.unpaired.retain(|(source, call)| match source {
            ToolCallSource::Transcript => false,
            ToolCallSource::Hooks => hooks_ended_at.is_some_and(|end| call.started_unix_ms > end),
        });
        turn.adopted.clear();
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn call(tool: &str, id: &str, at: u64) -> SeenToolCall {
        SeenToolCall {
            tool_name: tool.to_string(),
            call_id: id.to_string(),
            started_unix_ms: at,
        }
    }

    #[test]
    fn the_nth_call_of_a_tool_pairs_with_the_other_sources_nth() {
        // Given the hooks reported two reads and a grep
        let turns = ToolTurns::default();
        for seen in [
            call("Read", "h1", 10),
            call("Grep", "h2", 11),
            call("Read", "h3", 12),
        ] {
            assert_eq!(turns.saw("s", ToolCallSource::Hooks, seen), None);
        }

        // When the transcript reports a read, a read and a grep
        let paired: Vec<Option<String>> = ["Read", "Read", "Grep"]
            .iter()
            .enumerate()
            .map(|(n, tool)| {
                turns
                    .saw(
                        "s",
                        ToolCallSource::Transcript,
                        call(tool, &format!("t{n}"), 20),
                    )
                    .map(|seen| seen.call_id)
            })
            .collect();

        // Then
        assert_eq!(
            paired,
            vec![Some("h1".into()), Some("h3".into()), Some("h2".into())]
        );
    }

    #[test]
    fn a_hook_call_paired_with_a_transcript_call_seen_first_adopts_its_id() {
        // Given
        let turns = ToolTurns::default();
        turns.saw(
            "s",
            ToolCallSource::Transcript,
            call("Shell", "transcript:1", 5),
        );

        // When
        let paired = turns.saw("s", ToolCallSource::Hooks, call("Shell", "hook-7", 6));

        // Then
        assert_eq!(
            (
                paired.map(|seen| seen.call_id),
                turns.adopted_id("s", "hook-7")
            ),
            (Some("transcript:1".into()), Some("transcript:1".into()))
        );
    }

    #[test]
    fn a_call_from_an_ended_turn_pairs_with_nothing_later() {
        // Given a hook call the transcript never reported, in a turn that ended
        let turns = ToolTurns::default();
        turns.saw("s", ToolCallSource::Hooks, call("Read", "old", 1));
        turns.hooks_turn_ended("s", 2);
        turns.transcript_turn_ended("s");

        // When the next turn's transcript reports a read
        let paired = turns.saw("s", ToolCallSource::Transcript, call("Read", "t", 2));

        // Then
        assert_eq!(paired, None);
    }

    #[test]
    fn a_later_turns_hook_calls_survive_the_transcript_reaching_an_earlier_turns_end() {
        // Given a follower behind the hooks: turn one ended at 20, turn two's read started at 30
        let turns = ToolTurns::default();
        turns.saw("s", ToolCallSource::Hooks, call("Read", "one", 10));
        turns.hooks_turn_ended("s", 20);
        turns.saw("s", ToolCallSource::Hooks, call("Read", "two", 30));
        turns.saw("s", ToolCallSource::Transcript, call("Read", "t1", 40));

        // When the transcript reaches turn one's end, then turn two's read
        turns.transcript_turn_ended("s");
        let paired = turns.saw("s", ToolCallSource::Transcript, call("Read", "t2", 41));

        // Then
        assert_eq!(paired.map(|seen| seen.call_id), Some("two".into()));
    }

    #[test]
    fn the_earliest_unreached_hook_call_is_what_the_transcript_says_next_precedes() {
        // Given
        let turns = ToolTurns::default();
        turns.saw("s", ToolCallSource::Hooks, call("Read", "h1", 10));
        turns.saw("s", ToolCallSource::Hooks, call("Grep", "h2", 11));

        // When the transcript reaches the read
        turns.saw("s", ToolCallSource::Transcript, call("Read", "t", 50));

        // Then
        assert_eq!(turns.earliest_unreached_hook_call("s"), Some(11));
    }
}
