# 2026-10-25 — `CancelAgentConversation` removes a conversation without checking it is the caller's session's

**Category:** Pre-existing defect, widened by `#coagents` 7/8 (#726, `headless-driver`)
**Source:** `packages/tddy-session-agents/src/service.rs` (`cancel_agent_conversation`)

`cancel_agent_conversation` authenticates the caller for `req.session_id` (`session_dir`), then
removes `req.conversation_id` from the conversation table by id alone. A caller authenticated for one
session can cancel a conversation of another, given its id. Since #726 that path also calls
`sessions.conversation_closed`, so it removes that conversation's approval route and its headless
files too.

Conversation ids are unguessable, so this is not reachable by enumeration; it is still a missing
ownership check.

## What would close it

Remove the conversation only when its recorded session equals `req.session_id` (the table's entry
knows its session), and answer `NOT_FOUND` otherwise, as for an unknown id.
