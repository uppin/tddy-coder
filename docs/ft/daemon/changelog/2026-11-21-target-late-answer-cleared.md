# 2026-11-21 — A late answer to a target agent's prompt is cleared from the target

An orchestrator's answer that reaches the target after the agent's waiting hook gave up (a Coder write
takes several seconds) is removed by the daemon once the hook's wait has passed, instead of staying in
the session's worktree on the target until the session ends. See
[Claude CLI session § On a target session](../claude-cli-session.md#on-a-target-session).
