//! The addressable history of one subagent conversation: every message it holds carries an id, so
//! a caller can be told what a turn did and can name a point to send the conversation back to.
//!
//! Split out of `subagent.rs` rather than added to it — that file is already recorded as oversized
//! (`packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md`), and id minting, preview
//! truncation and rewind boundary-snapping are a self-contained concern with its own invariants.

use crate::openai::ChatMessage;

/// How much of one message a [`MessageDescriptor`] carries.
///
/// A descriptor is a handle for choosing a rewind point, not a copy of the payload: one `Read`
/// result in incident 2026-09-26 was 42 KB, and a turn outcome enumerating a handful of those
/// would cost its reader more context than the turn itself did.
pub const MESSAGE_PREVIEW_CHARS: usize = 240;

/// A message's identity for the life of its conversation.
///
/// The spelling is deliberately opaque — callers compare, store and hand ids back, and nothing
/// about how one is built is part of the contract. Two properties are: an id names exactly one
/// message, and an id a rewind discarded is never minted again.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct MessageId(String);

impl MessageId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MessageId {
    fn from(id: &str) -> Self {
        MessageId(id.to_string())
    }
}

impl From<String> for MessageId {
    fn from(id: String) -> Self {
        MessageId(id)
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Who a history message is from, as the chat protocol spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// The def's seed prompt. Present in the history and therefore addressable, but never part of
    /// a turn's appended messages — it is written once, at construction, before any turn runs.
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    /// The role of a chat message, or `None` for a spelling this build does not model.
    ///
    /// `None` rather than a default: a message whose role is not one of these four is a message
    /// this code cannot describe, and describing it as a user turn would misreport who said it.
    fn of(message: &ChatMessage) -> Option<Self> {
        match message.role.as_str() {
            "system" => Some(MessageRole::System),
            "user" => Some(MessageRole::User),
            "assistant" => Some(MessageRole::Assistant),
            "tool" => Some(MessageRole::Tool),
            _ => None,
        }
    }
}

/// What one message in a conversation was, in the form a caller reads to decide what to do next.
///
/// `camelCase` on the wire, because this is serialized **inside** the MCP turn outcome, whose
/// other fields are `stopReason`, `inputTokens`, `outputTokens`, `totalTokens` and
/// `clampedMaxTurns`. Without the rename an agent reads `isError` in the tool description and
/// `is_error` in the payload, which is a small lie of exactly the kind this surface exists to stop
/// telling.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDescriptor {
    pub id: MessageId,
    pub role: MessageRole,
    /// The tool that produced a `tool`-role message, by name.
    pub tool: Option<String>,
    /// The tools an `assistant`-role message called, by name, in the order it called them.
    pub tool_calls: Vec<String>,
    /// Whether this message reports a tool call that produced no result.
    ///
    /// The field whose absence let incident 2026-09-26 run for 54 refused calls: a main agent
    /// reading a subagent's turn could see how many messages went by, but not that every one of
    /// them was a refusal.
    pub is_error: bool,
    /// The message's own text, cut to [`MESSAGE_PREVIEW_CHARS`].
    pub preview: String,
}

/// Cut `text` to [`MESSAGE_PREVIEW_CHARS`], marking that it was cut.
///
/// Counted in `char`s, not bytes: a byte slice through a multi-byte character panics.
fn preview_of(text: &str) -> String {
    let total = text.chars().count();
    if total <= MESSAGE_PREVIEW_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MESSAGE_PREVIEW_CHARS - 1).collect();
    format!("{kept}…")
}

/// One message, with the identity and the refusal flag the chat protocol has nowhere to put.
struct TranscriptEntry {
    id: MessageId,
    message: ChatMessage,
    is_error: bool,
}

/// One conversation's messages, in order, each addressable by a [`MessageId`].
///
/// Ids come from a counter that only ever rises, including across a rewind: a rewind discards
/// messages, and the ids it discarded must not come back, or a caller holding an id from before
/// the rewind would silently address a different message than the one it saw.
#[derive(Default)]
pub(crate) struct Transcript {
    entries: Vec<TranscriptEntry>,
    /// The next id to mint. Never decremented — see the type's own doc.
    next_ordinal: u64,
}

impl Transcript {
    /// Append `message`, returning the id it was given.
    pub(crate) fn push(&mut self, message: ChatMessage) -> MessageId {
        self.push_marked(message, false)
    }

    /// Append `message`, recording whether it reports a tool call that produced no result.
    pub(crate) fn push_marked(&mut self, message: ChatMessage, is_error: bool) -> MessageId {
        self.next_ordinal += 1;
        let id = MessageId(format!("m{}", self.next_ordinal));
        self.entries.push(TranscriptEntry {
            id: id.clone(),
            message,
            is_error,
        });
        id
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// The history as the provider is sent it.
    pub(crate) fn messages(&self) -> Vec<ChatMessage> {
        self.entries
            .iter()
            .map(|entry| entry.message.clone())
            .collect()
    }

    /// Every message, oldest first — for the readers that summarize a conversation rather than
    /// address it ([`crate::subagent::SubagentSession::tail`], the handoff brief).
    pub(crate) fn iter(&self) -> impl Iterator<Item = &ChatMessage> {
        self.entries.iter().map(|entry| &entry.message)
    }

    pub(crate) fn last(&self) -> Option<&ChatMessage> {
        self.entries.last().map(|entry| &entry.message)
    }

    /// Describe the messages from `index` onward — what one turn appended.
    ///
    /// A message whose role this build does not model is left out rather than described wrongly;
    /// [`MessageRole::of`] says why.
    pub(crate) fn descriptors_from(&self, index: usize) -> Vec<MessageDescriptor> {
        self.entries
            .iter()
            .skip(index)
            .filter_map(|entry| {
                Some(MessageDescriptor {
                    id: entry.id.clone(),
                    role: MessageRole::of(&entry.message)?,
                    tool: entry.message.name.clone(),
                    tool_calls: entry
                        .message
                        .tool_calls
                        .iter()
                        .flatten()
                        .map(|call| call.function.name.clone())
                        .collect(),
                    is_error: entry.is_error,
                    preview: preview_of(entry.message.content.as_deref().unwrap_or("")),
                })
            })
            .collect()
    }

    /// Discard every message after `id`, so the conversation continues from there.
    ///
    /// An id this transcript does not hold is an error naming it, never a continue from the end:
    /// the caller asked to go back, and carrying on instead does the opposite of what was asked.
    /// A rewind a previous rewind already made unreachable lands here too, which is the point of
    /// never re-minting a discarded id.
    ///
    /// **Boundary snapping.** The cut is extended forward over any `tool`-role messages that
    /// immediately follow it. OpenAI rejects an assistant message carrying `tool_calls` that no
    /// `tool` message answers, so a cut landing between a call and its results — or in the middle
    /// of one call's results — would build a request the provider refuses, at the exact moment the
    /// caller is trying to recover from a failure. Keeping the group whole is the smallest legal
    /// history that still honours the caller's point.
    pub(crate) fn rewind_to(&mut self, id: &MessageId) -> Result<(), RewindError> {
        let at = self
            .entries
            .iter()
            .position(|entry| &entry.id == id)
            .ok_or_else(|| RewindError(id.clone()))?;
        let mut keep_through = at;
        while self
            .entries
            .get(keep_through + 1)
            .is_some_and(|entry| entry.message.role == "tool")
        {
            keep_through += 1;
        }
        self.entries.truncate(keep_through + 1);
        Ok(())
    }
}

/// A rewind naming a message the conversation does not hold.
#[derive(Debug)]
pub(crate) struct RewindError(MessageId);

impl std::fmt::Display for RewindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "cannot resume from message '{}': this conversation has no such message (it was never \
             minted, or an earlier rewind discarded it)",
            self.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::{ToolCall, ToolCallFunction};

    fn a_tool_call(name: &str) -> ToolCall {
        ToolCall {
            id: format!("call_{name}"),
            call_type: "function".to_string(),
            function: ToolCallFunction {
                name: name.to_string(),
                arguments: "{}".to_string(),
            },
        }
    }

    /// The history shape a rewind has to reason about: a prompt, a call, its result, a conclusion.
    fn a_conversation_that_called_one_tool() -> (Transcript, MessageId, MessageId) {
        let mut transcript = Transcript::default();
        let prompt = transcript.push(ChatMessage::user("find it"));
        let call = transcript.push(ChatMessage::assistant(
            Some("looking".to_string()),
            Some(vec![a_tool_call("READ")]),
        ));
        transcript.push_marked(
            ChatMessage::tool_result(
                "{}".to_string(),
                "call_READ".to_string(),
                "READ".to_string(),
            ),
            false,
        );
        transcript.push(ChatMessage::assistant(Some("done".to_string()), None));
        (transcript, prompt, call)
    }

    fn roles_in(transcript: &Transcript) -> Vec<String> {
        transcript.iter().map(|m| m.role.clone()).collect()
    }

    #[test]
    fn a_rewind_keeps_the_named_message_and_drops_what_followed_it() {
        // Given
        let (mut transcript, prompt, _call) = a_conversation_that_called_one_tool();

        // When
        transcript
            .rewind_to(&prompt)
            .expect("the prompt is present");

        // Then
        assert_eq!(roles_in(&transcript), vec!["user".to_string()]);
    }

    #[test]
    fn a_rewind_onto_a_tool_call_keeps_the_results_that_answer_it() {
        // Given
        let (mut transcript, _prompt, call) = a_conversation_that_called_one_tool();

        // When
        transcript.rewind_to(&call).expect("the call is present");

        // Then the assistant's call is still answered, so the history is one a provider accepts
        assert_eq!(
            roles_in(&transcript),
            vec![
                "user".to_string(),
                "assistant".to_string(),
                "tool".to_string()
            ]
        );
    }

    #[test]
    fn an_id_a_rewind_discarded_is_never_minted_again() {
        // Given a conversation rewound to its first message
        let (mut transcript, prompt, _call) = a_conversation_that_called_one_tool();
        transcript
            .rewind_to(&prompt)
            .expect("the prompt is present");

        // When it grows again
        let minted = transcript.push(ChatMessage::user("and again"));

        // Then the new message does not reuse an id the rewind discarded
        assert_eq!(minted.as_str(), "m5");
    }

    #[test]
    fn a_rewind_to_an_id_the_conversation_never_held_names_it() {
        // Given
        let (mut transcript, _prompt, _call) = a_conversation_that_called_one_tool();

        // When
        let refused = transcript
            .rewind_to(&MessageId::from("m-never-minted"))
            .expect_err("an unknown rewind point is an error");

        // Then
        assert_eq!(
            refused.to_string(),
            "cannot resume from message 'm-never-minted': this conversation has no such message \
             (it was never minted, or an earlier rewind discarded it)"
        );
    }

    #[test]
    fn a_preview_of_a_message_longer_than_the_bound_is_cut_to_it() {
        // Given a tool result far larger than a preview
        let mut transcript = Transcript::default();
        transcript.push(ChatMessage::user("x".repeat(MESSAGE_PREVIEW_CHARS * 10)));

        // When it is described
        let described = transcript.descriptors_from(0);

        // Then the descriptor carries a handle, not the payload
        assert_eq!(described[0].preview.chars().count(), MESSAGE_PREVIEW_CHARS);
        assert!(described[0].preview.ends_with('…'));
    }

    #[test]
    fn a_descriptor_names_the_tools_its_assistant_message_called() {
        // Given
        let mut transcript = Transcript::default();
        transcript.push(ChatMessage::assistant(
            Some("looking".to_string()),
            Some(vec![a_tool_call("READ"), a_tool_call("GREP")]),
        ));

        // When
        let described = transcript.descriptors_from(0);

        // Then
        assert_eq!(
            described[0].tool_calls,
            vec!["READ".to_string(), "GREP".to_string()]
        );
    }
}
