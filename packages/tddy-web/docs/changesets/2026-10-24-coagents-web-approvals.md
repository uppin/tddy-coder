# 2026-10-24 — Answering a deferred permission request from the conversation dialog

**Type:** Feature

`#coagents` 8/8 ([#727](https://github.com/uppin/tddy-coder/pull/727)). `PermissionRequestEntry`, `TranscriptPermissionRequest`, `pendingPermission` and `useAnswerAgentPermission` in `SubagentConversationDialog`; answers go to `rosterHalfOf(session)`; "waiting for you" for `awaiting_user`; "Answered by" / "Cancelled by". `useAgentConversation`'s cancel names no login and is refused with no signed-in user (`callerLogin` through `SessionsDrawerScreen` → `SessionRuntime` → `SessionMainPane`).

Code issues: new `oversized-file-sessions-drawer-screen` (497 → 501), deferred to the sweep node. See [agent conversation tabs](../session-agent-conversation.md) and the [feature](../../../../docs/ft/web/agent-activity-pane.md#the-user-answers-a-deferred-permission-request-added-2026-10-25).
