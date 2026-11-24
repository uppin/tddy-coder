# 2026-11-24 — A target session's hooks keep reporting after the agent changes directory

- On a Coder or SSH target, every hook names the session's hook log by its absolute path, `<worktree>/.tddy/hooks.log`. Before this, a Claude session whose agent had `cd`-ed into a subdirectory failed every hook with `cannot create .tddy/hooks.log: Directory nonexistent`. Under `dash`, the `/bin/sh` of a Debian or Ubuntu target, the failing `PreToolUse` hook refused every tool. Cursor and Muse sessions on a target take the same command.
- A waiting hook (a permission prompt, a question) reads its answer from `<worktree>/.tddy/answers/`. The hook-log line a child on a target is told to run names the absolute log too.
- A hook that cannot append still fails, as loudly as before; nothing hides that failure.

Feature: [claude-cli-session.md § On a target session](../claude-cli-session.md#on-a-target-session) · [agent-session-status.md](../agent-session-status.md)
