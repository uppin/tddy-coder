# 2026-11-21 — An orchestrator answers the permission prompts of an agent on a target

A permission prompt of Claude Code running on a target (a Coder workspace or an SSH host) becomes the
session's pending question, with its tool, its input and a `request_id`, and `AnswerSessionPermission`
answers it as it answers a local session's. The answer reaches the agent through a file in the
session's worktree on the target, so the target still needs no tddy software. With no orchestrator,
or no answer within the session's wait, Claude asks in its terminal. The answers directory is removed
with the hook log when the session ends. See
[Claude CLI session § On a target session](../claude-cli-session.md#on-a-target-session).
