# 2026-11-19 — Agents sharing a target worktree keep their own pids, and a Coder session ends with its agent's status

A session whose agent runs on a target records its pid at `.tddy/agents/<session id>.pid` in the
worktree, so several agents in one worktree no longer overwrite each other's and a stop ends only its
own. A session on a Coder workspace reports the agent's own exit status, read from the `coder` CLI's
last line, instead of the CLI's constant 1. See [claude-cli-session.md](../claude-cli-session.md).
