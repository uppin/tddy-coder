# 2026-10-25 — Coding subagents: a cancel keeps the conversation, and every headless CLI gets tddy-tools

`#coagents` 9/9 ([#776](https://github.com/uppin/tddy-coder/pull/776)).

- **Cancelling a coding agent's turn no longer ends the conversation.** `subagent_cancel` stops the
  running turn (ACP `session/cancel`) and the agent keeps its process, context and worktree; the next
  `subagent_prompt` carries on. `subagent_end` and detaching still close it. A model agent's cancel
  is unchanged.
- **A permission request says what kind of call is asking** — `edit`, `execute`, `read`, … — instead
  of always `other`.
- **"Waiting for you" appears at once** for a client following the session live, and the daemon's
  status rows are marked (`source = "subagent-status"`) so nothing mistakes them for the agent's own
  calls.
- **Headless Cursor, Codex and Warp (`oz`) subagents get the session's tddy-tools tools**, each in its
  own CLI's shape. They still run only in unmanaged sessions.
- **A cancel can no longer close another session's conversation**, and a headless conversation that
  fails to open leaves no approval route or credential files behind; files a stopped daemon left are
  removed when it starts again.

Feature docs: [Session agent roster § Coding agents](../../daemon/session-agent-roster.md#coding-agents),
[Managed codebase subagents](../managed-codebase-subagents.md),
[Agent activity pane § The user answers a deferred permission request](../../web/agent-activity-pane.md#the-user-answers-a-deferred-permission-request-added-2026-10-25).
