# 2026-10-24 — Deferring a permission request, and the second resolver's types

**Type:** Feature

`#coagents` 8/8 ([#727](https://github.com/uppin/tddy-coder/pull/727)). `TurnRequest::deferring_permission_to_user` / `permission_deferred_to_user`; the link encodes it as `defer_permission_to_user`. `subagent/permission.rs`: `PermissionAnswering`, `AnsweredBy { MainAgent, User }` (`name()`: `main-agent` or the login), `permission_tool_call_id`, `answered_output`, `answer_delivered_while_held`. `SubagentSession::permission_answerer` (default `None`). `PendingTurns::mark_awaiting_user` and the pending JSON's `awaitingUser`. `run_turn` sends a permission answer for a remote conversation past the session a deferred turn holds (`RemoteConversationHandle::take_turn`).

Code issues: `oversized-file-subagent-runtime` 745 → 823, `-subagent` 2,087 → 2,096, `-conversation` 551 → 560, deferred to the sweep node. See [roster and subagent runtime](../roster-and-subagent-runtime.md).
