//! The addressable history of one subagent conversation: every message it holds carries an id, so
//! a caller can be told what a turn did and can name a point to send the conversation back to.
//!
//! Split out of `subagent.rs` rather than added to it — that file is already recorded as oversized
//! (`packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md`), and id minting, preview
//! truncation and rewind boundary-snapping are a self-contained concern with its own invariants.

use crate::openai::{ChatMessage, ToolCall, ToolCallFunction};

use super::result_summary::ResultSummary;

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

/// One tool call an assistant message made: which tool, and what it asked for.
///
/// The arguments are the half that was missing. In session 01a0e200 a main agent reading a
/// subagent's turn could see that three `READ`s had failed but not that all three asked for the
/// same path with a stray quote on the end — the one reader positioned to notice the subagent was
/// emitting broken arguments was handed a tool name and an error string, and could only conclude
/// "wrong file, try another".
///
/// Bounded for the reason [`MessageDescriptor::preview`] is: a `WRITE` carries a whole file in its
/// arguments, and an outcome enumerating several of those would cost its reader more context than
/// the turn it describes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallDescriptor {
    pub name: String,
    /// The call's arguments as the model wrote them, cut to [`MESSAGE_PREVIEW_CHARS`].
    pub arguments: String,
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
    /// The tool calls an `assistant`-role message made, in the order it made them — each by name
    /// and by what it asked for.
    pub tool_calls: Vec<ToolCallDescriptor>,
    /// Whether this message reports a tool call that produced no result.
    ///
    /// The field whose absence let incident 2026-09-26 run for 54 refused calls: a main agent
    /// reading a subagent's turn could see how many messages went by, but not that every one of
    /// them was a refusal.
    pub is_error: bool,
    /// The message's own text, cut to [`MESSAGE_PREVIEW_CHARS`].
    pub preview: String,
    /// The structured facts of a `tool`-role message's result — `None` for every other role and
    /// for a dispatch that produced nothing (whose shape [`ResultSummary::Error`] names and
    /// whose text the `preview` already carries).
    ///
    /// Serialized as an externally tagged object — `{"read": {…}}` — so a caller dispatches on
    /// the same tool name it dispatches the tool call on. Absent rather than `null` when there
    /// is nothing to say, matching the descriptor's other optional readings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<super::result_summary::ResultSummary>,
    /// What a mutating tool call did to the conversation's own worktree — files created, updated
    /// and removed, lines added and removed, and the commit that recorded it. `None` for a read,
    /// for every non-`tool` role, and for a call whose codebase has no conversation worktree.
    ///
    /// A sibling of `result_summary` rather than a field inside it: it describes the worktree, not
    /// the tool's answer, and `SHELL` / `AWAIT` change files their own summaries never mention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_change: Option<tddy_subagent_worktree::WorktreeChange>,
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

impl Transcript {
    /// Append a caller-provided tool call and its result as history: an `assistant` message
    /// carrying the call, then a `tool` message carrying the caller's result, each with a minted
    /// id — in the same shape a real call would have appeared in, so the model reading the
    /// history cannot tell a replaced call from one it made.
    ///
    /// Append-only by design (the yielded conversation's own history is untouched — the failed
    /// call stays, and the model sees both it and the operator's fix): nothing here rewrites or
    /// removes what the conversation already holds.
    ///
    /// Returns the minted ids, the assistant message's then the tool message's — the anchors a
    /// caller names to a later rewind.
    pub(crate) fn append_replacement(
        &mut self,
        replacement: &super::replacement::Replacement,
    ) -> (MessageId, MessageId) {
        // The call's id is minted here, not taken from a provider: this call never reached one.
        // It draws on the transcript's own rising ordinal — the uniqueness source `MessageId`
        // documents — and the `call_replacement` spelling is not one a provider's `call_<n>`
        // minting produces, so a later model-issued call cannot collide with it.
        let call_id = format!("call_replacement_{}", self.next_ordinal);
        let call_message_id = self.push(ChatMessage::assistant(
            // No prose: the caller supplied the call, and there was no model turn to write any.
            None,
            Some(vec![ToolCall {
                id: call_id.clone(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: replacement.tool.clone(),
                    arguments: replacement.arguments.to_string(),
                },
            }]),
        ));
        // Recorded as history, not as a refusal — the caller's result is the answer the failed
        // call should have produced, verbatim. `push` marks it non-error and leaves the summary
        // empty: no dispatch ran, and the summary vocabulary is for the engine's own results.
        let result_message_id = self.push(ChatMessage::tool_result(
            replacement.result.clone(),
            call_id,
            replacement.tool.clone(),
        ));
        (call_message_id, result_message_id)
    }
}

/// One message, with the identity and the refusal flag the chat protocol has nowhere to put.
struct TranscriptEntry {
    id: MessageId,
    message: ChatMessage,
    is_error: bool,
    /// The structured facts of a `tool`-role result, computed at the append site where the
    /// result JSON is still structured — `None` for every other kind of message.
    result_summary: Option<ResultSummary>,
    /// What the call did to the conversation worktree, read off the result's `worktreeChange` at
    /// the same append site. Kept on the entry so a rewind can find the commit it goes back to.
    worktree_change: Option<tddy_subagent_worktree::WorktreeChange>,
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
        self.push_with_summary(message, is_error, None, None)
    }

    /// Append a `tool`-role result carrying the facts of that result, as the append site read
    /// them from the dispatch's still-structured JSON.
    pub(crate) fn push_tool_result(
        &mut self,
        message: ChatMessage,
        is_error: bool,
        result_summary: ResultSummary,
        worktree_change: Option<tddy_subagent_worktree::WorktreeChange>,
    ) -> MessageId {
        self.push_with_summary(message, is_error, Some(result_summary), worktree_change)
    }

    fn push_with_summary(
        &mut self,
        message: ChatMessage,
        is_error: bool,
        result_summary: Option<ResultSummary>,
        worktree_change: Option<tddy_subagent_worktree::WorktreeChange>,
    ) -> MessageId {
        self.next_ordinal += 1;
        let id = MessageId(format!("m{}", self.next_ordinal));
        self.entries.push(TranscriptEntry {
            id: id.clone(),
            message,
            is_error,
            result_summary,
            worktree_change,
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
                        .map(|call| ToolCallDescriptor {
                            name: call.function.name.clone(),
                            arguments: preview_of(&call.function.arguments),
                        })
                        .collect(),
                    is_error: entry.is_error,
                    preview: preview_of(entry.message.content.as_deref().unwrap_or("")),
                    result_summary: entry.result_summary.clone(),
                    worktree_change: entry.worktree_change.clone(),
                })
            })
            .collect()
    }

    /// Where a rewind to `id` takes the conversation's worktree: the commit of the last entry the
    /// rewind keeps that made one — the cut extends over the tool results answering `id`, so a
    /// call's commit is kept with its call — or [`ResetTarget::Base`] when no kept entry did. An id
    /// this transcript does not hold is the same error [`Self::rewind_to`] gives.
    pub(crate) fn commit_kept_by(
        &self,
        id: &MessageId,
    ) -> Result<super::worktree_reset::ResetTarget, RewindError> {
        let keep_through = self.last_kept_by(id)?;
        let commit = self.entries[..=keep_through]
            .iter()
            .rev()
            .find_map(|entry| entry.worktree_change.as_ref()?.commit.clone());
        Ok(
            commit.map_or(super::worktree_reset::ResetTarget::Base, |commit| {
                super::worktree_reset::ResetTarget::Commit(commit)
            }),
        )
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
        let keep_through = self.last_kept_by(id)?;
        self.entries.truncate(keep_through + 1);
        Ok(())
    }

    /// The index of the last entry a rewind to `id` keeps: `id` itself, extended forward over the
    /// `tool` results that answer it.
    fn last_kept_by(&self, id: &MessageId) -> Result<usize, RewindError> {
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
        Ok(keep_through)
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
            described[0]
                .tool_calls
                .iter()
                .map(|call| call.name.as_str())
                .collect::<Vec<_>>(),
            vec!["READ", "GREP"]
        );
    }
}
