# 2026-11-21 — Clear, Delete and Restore act on a target session's worktree

For a session whose agent runs on an ssh host or a Coder workspace, the Worktree tab's **Clear**,
**Delete** and **Restore** now run on that target. Restore no longer cuts a worktree on the daemon
host and leaves the session pointing at it, and an unreachable target is reported with its reason.
See [session-worktree-inspector.md § On a target session](../session-worktree-inspector.md#on-a-target-session).
