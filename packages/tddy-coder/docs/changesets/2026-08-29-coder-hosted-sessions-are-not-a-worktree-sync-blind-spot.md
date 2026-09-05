# 2026-08-29 — coder-hosted sessions are not a worktree-sync blind spot

**Type:** Feature

`session_participant` surfaces the agent's own tool calls so a coder-hosted session's activity reaches the room like a daemon-hosted one's. No new coder credential and no new channel: the durable `agent-activity.jsonl` the coder already writes is what the daemon's poll loop tails, which is why the originally-planned `ReportAgentActivity` transport from the coder was never built. Feature [session-worktree-sync.md](../../../../docs/ft/daemon/session-worktree-sync.md).
