# 2026-10-25 — A conversation's waiting-on-the-user status row is durable only

**Category:** Deferred from `#coagents` 8/8 (#727, `web-approvals`) — known gap, accepted by the developer
**Source:** `packages/tddy-session-agents/src/conversation_status.rs` (`record`, `TODO(web-approvals)`); `packages/tddy-service/src/subagent_conversation_replay.rs` (`CONVERSATION_ROW_TOOL_NAME`)

While a turn waits on a permission request the main agent handed to the user, the daemon writes one
status row per conversation into the session's `agent-activity.jsonl`: call id
`subagent-conversation:<conversation id>`, tool name `subagent_conversation`, running with
`{awaitingUser: true}`, cleared when the answer is delivered, completed at turn end. The replay folds
it into the conversation's bubble, which is how the bubble reads "waiting for you".

Two gaps remain:

- **Not published live.** `conversation_status::record` appends to the log only; it does not publish
  on the daemon's `AgentActivityHub`. A web client following the session live sees the waiting state
  at its next replay snapshot, not when the deferral is accepted.
- **Raw activity shows it as a tool call.** `tddy-session-activity`'s `StreamAgentActivity` snapshot
  returns the row as an ordinary tool call named `subagent_conversation`. Only the ACP replay fold
  knows it is a status row, so any other consumer of raw activity has to tell it apart from the
  agent's own calls by name.

A refused deferral also shows running-with-awaiting for a moment, then completed.

## What would close it

Publish the row through the same path `ReportAgentActivity` uses (append, then the hub), so a live
subscriber's fold sees it at once; and either keep status rows out of `StreamAgentActivity` or mark
them with a field the raw stream carries, instead of a reserved tool name.
