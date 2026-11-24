# 2026-11-24 — A Cursor session's replies never reach its transcript

**Category:** Future enhancement
**Source:** `fix/cursor-tui-submit-and-resume-model` ([changeset](../1-WIP/2026-11-24-cursor-tui-submit-and-resume-model.md)); deferred out of that PR by the operator.

A `cursor-cli` session's ACP transcript (`acp-transcript.jsonl`, `ReadSessionTranscript`, the web's
Activities view) holds its tool calls but never what Cursor said; a `claude-cli` session's holds
both. Four gaps, each enough on its own:

1. **The events are Claude's, in code.** `record_spoken_message`
   (`packages/tddy-session-activity/src/service/transcript_messages.rs`) matches
   `UserPromptSubmit` → `prompt` and `Stop` → `last_assistant_message` by name. Cursor's are
   `beforeSubmitPrompt` → `prompt` and `afterAgentResponse` → `text` (verified 2026-11-24, Cursor
   Agent 2026.11.22: `afterAgentResponse` carries the whole reply, e.g.
   `"text": "> Auto routed to Claude Haiku 5.5\n\nHOOK1"`).
2. **`afterAgentResponse` is not installed.** `CURSOR_HOOK_EVENTS`
   (`packages/tddy-agent-backend/src/cursor_hooks.rs`) leaves it out on purpose — the comment there
   assumes it would duplicate a tool-use row, which it does not: it carries no tool payload.
3. **A status-less event never reaches the daemon.** `hook_request.rs` builds a report only when
   `activity_status_from_hook` maps the event (`?`), and `afterAgentResponse` maps to nothing, so it
   is dropped before `ReportSessionStatus`.
4. **The own-conversation check misses Cursor's chat.** `record_spoken_message` keeps a message only
   when `agent_conversation_id` (the hook's `session_id` — Cursor's chat id) is the session's own
   conversation (the last of `agent_conversations`, else the tddy session id). A Cursor session's chat
   id is `cursor_chat_id`, so every Cursor message would be ignored.

Shape of the fix: a spec slot (alongside `model_report`) naming the prompt and reply events and
fields — `transcript: { prompt: {event, field}, reply: {event, field} }` — read instead of the
hardcoded names; install `afterAgentResponse`; let a report with no status change through for a
slot-declared event; count `cursor_chat_id` as the session's own conversation.

**Why deferred:** moderate on its own (four crates) and independent of the submit and resume fixes;
the operator split it out.
