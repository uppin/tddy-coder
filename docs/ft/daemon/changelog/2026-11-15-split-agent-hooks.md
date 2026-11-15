# 2026-11-15 — A split agent loads its hooks file and carries its spec's launch variables

A split session's agent loads the hooks file the daemon writes into its context dir
with the flag its spec's `host_agent` variant declares (`--settings` for Claude), so
its status, questions and notifications are reported, and its environment carries the
spec's launch variables (`CLAUDE_CODE_NO_FLICKER=1` for Claude). See
[remote-managed-worktree.md § Session start](../remote-managed-worktree.md#session-start).
