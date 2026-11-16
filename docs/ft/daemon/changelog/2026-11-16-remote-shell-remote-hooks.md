# 2026-11-16 — A remote agent's hooks reach the daemon with nothing installed

A session whose agent runs on a target shows working, waiting and idle like a local one: its
lifecycle hooks append one line each to `.tddy/hooks.log` in the worktree — a shell one-liner
using only `printf`, `cat` and `tr`, with no tddy software on the target — and the daemon
follows that file through the session's remote shell into the same hook handling a local hook
reaches. Hooks that wait for an answer are not installed on a target, so permission prompts and
questions appear in the agent's terminal. See
[agent-session-status.md](../agent-session-status.md) and
[claude-cli-session.md](../claude-cli-session.md).
