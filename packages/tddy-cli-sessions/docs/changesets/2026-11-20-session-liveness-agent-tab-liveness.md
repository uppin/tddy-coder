# 2026-11-20 — A session is live only while the daemon holds an agent tab

**Type:** Fix · **Stack:** `#session-liveness` 1/4 (PR #958)

`CliSessionManager::live_agent_sessions()` (a `main` or agent-kind terminal, or a detached tmux agent a watcher holds; never `bash` / `tmux`), the `settling` hold, and `ReattachOutcome` announced on `reattach_finished()`. Full record: [`docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md`](../../../../docs/dev/changesets/2026-11-20-session-liveness-agent-tab-liveness.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md).
