//! The conversations a session's agent has had, in order: `.session.yaml` `agent_conversations`
//! (`#orchestrators` 6/11).
//!
//! An agent may start a fresh conversation inside a running session — `/clear` in Claude Code —
//! and its spec says how that is reported (`coding_agent.conversation.rotation`). Every id the
//! session has had is kept, the first written at start and each rotation appended, so the last is
//! the conversation a resume continues and an earlier one's transcript stays findable.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-10-conversation-rotation.md`.

use serde::{Deserialize, Serialize};

/// One conversation a session's agent had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConversation {
    /// The agent's own id for the conversation (Claude Code's `session_id`).
    pub id: String,
    /// When the daemon recorded it began, in Unix milliseconds.
    pub started_at_unix_ms: u64,
    /// Why it began.
    pub reason: AgentConversationReason,
}

/// Why a conversation began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentConversationReason {
    /// The session's first conversation, recorded at start.
    Start,
    /// A fresh conversation the agent rotated to inside the running session (`/clear`).
    Rotate,
}

/// Append `next` to `conversations` unless its id is already listed; true when it was appended.
/// The same conversation reported twice — a hook fired twice, or retried — is one entry.
pub fn record_agent_conversation(
    conversations: &mut Vec<AgentConversation>,
    next: AgentConversation,
) -> bool {
    // TODO(conversation-rotation): implement — append `next` unless its id is already listed.
    let _ = (conversations, next);
    false
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    const AT: u64 = 1_762_771_200_000;

    fn started(id: &str) -> AgentConversation {
        AgentConversation {
            id: id.to_string(),
            started_at_unix_ms: AT,
            reason: AgentConversationReason::Start,
        }
    }

    fn rotated(id: &str) -> AgentConversation {
        AgentConversation {
            id: id.to_string(),
            started_at_unix_ms: AT + 1,
            reason: AgentConversationReason::Rotate,
        }
    }

    #[test]
    fn a_new_conversation_is_appended_last() {
        // Given
        let mut conversations = vec![started("c-0")];

        // When
        let appended = record_agent_conversation(&mut conversations, rotated("c-1"));

        // Then
        assert!(appended);
        assert_eq!(conversations, vec![started("c-0"), rotated("c-1")]);
    }

    #[test]
    fn recording_the_same_conversation_twice_lists_it_once() {
        // Given
        let mut conversations = vec![started("c-0")];
        record_agent_conversation(&mut conversations, rotated("c-1"));

        // When
        let appended = record_agent_conversation(&mut conversations, rotated("c-1"));

        // Then
        assert!(!appended);
        assert_eq!(conversations, vec![started("c-0"), rotated("c-1")]);
    }

    #[test]
    fn the_first_conversation_is_recorded_into_an_empty_list() {
        // Given
        let mut conversations = Vec::new();

        // When
        let appended = record_agent_conversation(&mut conversations, started("c-0"));

        // Then
        assert!(appended);
        assert_eq!(conversations, vec![started("c-0")]);
    }

    #[test]
    fn a_conversation_is_written_with_its_reason_in_snake_case() {
        // When
        let yaml = serde_yaml::to_string(&rotated("c-1")).expect("serializes");

        // Then
        assert_eq!(
            yaml,
            "id: c-1\nstarted_at_unix_ms: 1762771200001\nreason: rotate\n"
        );
    }
}
