# 2026-09-26 — `session_agents.proto` carries a turn's budget, its rewind and its transcript

**Type:** API

`ResumeAgentConversation` (server stream) joins the family, taking `from_message_id`,
`correction` and `max_turns`; `PromptAgentConversationRequest` gains `max_turns`. The final
`AgentConversationChunk` gains `messages` — `AgentMessageDescriptor { id, role, tool, tool_calls,
is_error, preview }` — and `clamped_max_turns`. Both ride the final frame, as `stop_reason` does,
because a turn's transcript is not known until the turn has ended.

`IN_JAIL_RELAYABLE` widens **5 → 6**. A conversation an in-jail `tddy-tools` opened over the relay
must be one it can continue; without the entry the resume fails `not_found` — closed, which is the
safe direction, but it leaves every jailed conversation startable and never continuable. The entry
reaches the same code path as `PromptAgentConversation` under the same authentication, so it opens
no route weaker than one already open. It is **not** the case that a caller is confined to its own
session's conversations: the token resolves to an OS user and is never cross-checked against
`session_id`, and a conversation is looked up in a host-global map keyed on its id alone —
pre-existing, and recorded in
[`docs/dev/todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`](../../../../docs/dev/todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md).
What resume adds over prompt is a **destructive write**, not one more turn.

[docs/ft/daemon/session-agent-roster.md](../../../../docs/ft/daemon/session-agent-roster.md) § A
conversation's turn control crosses the wire · [../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
