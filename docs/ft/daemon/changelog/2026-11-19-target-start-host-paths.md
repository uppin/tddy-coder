# 2026-11-19 — A start on a Coder workspace gets past its system prompt, says why a command failed, and branches from main

A Claude start on a Linux target (a Coder workspace, or an SSH host whose home differs from the
daemon's) no longer fails creating `/Users/…` on the target: the agent's appended system prompt is
copied into the worktree's `.tddy/` there and the agent is told that path. A command that fails on a
Coder workspace is reported with what it said (`mkdir: … Permission denied`), not the `coder` CLI's
generic "Process exited with status 1", on the start record and in the start's error. A new branch
on a target that names no base is cut from the project's `origin/master` (or `origin/main`) as a
local start's is, not from the workspace clone's stale local `master`. See
[claude-cli-session.md](../claude-cli-session.md).
