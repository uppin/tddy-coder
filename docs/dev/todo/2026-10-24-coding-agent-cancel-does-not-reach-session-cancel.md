# 2026-10-24 — `subagent_cancel` on a coding agent closes the conversation instead of cancelling the turn

**Category:** Deferred from `#coagents` 2/8 (#721) to `#coagents` 3/8 (`permission-yield`)
**Source:** `packages/tddy-coding-subagent/src/session.rs` (`CodingAgentSession::cancel_handle`, `TODO(acp-subagent n3)`)

`CodingAgentSession::cancel_handle()` sends ACP `session/cancel`, and the running turn ends
`cancelled` (tested in `coding_agent_session_acceptance.rs`). The daemon's conversation table holds
`Box<dyn SubagentSession>`, which cannot reach it, so `CancelAgentConversation` does what it does for
every agent: it removes the conversation, interrupting the turn and stopping the agent process. The
coding agent's own context is lost with it.

## What would close it

Give the conversation table a cancel path into the session (a `SubagentSession` method, or holding
the `CancelHandle` beside the boxed session) so a coding-agent cancel sends `session/cancel` and
keeps the conversation. `#coagents` 3/8 reworks this path for parked permission requests
("cancel while pending answers `Cancelled`, then `session/cancel`") and owns it.
