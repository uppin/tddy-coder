# 2026-11-21 — A sandboxed session's shell tab runs in a jail of its own

A shell tab opened **In sandbox** on a sandboxed session runs in a [jail of its own](../session-liveness.md#a-sandboxed-sessions-tabs)
— a second runner over the session's worktree and grants, on the session's provider route — and is recorded
`[sandbox, shell]`, so Resume reopens it in a new jail. **On host** tabs of a sandboxed session are now reachable.
Closing a tab, deleting, stopping or resuming the session, and daemon shutdown stop its jails; shutdown also
stops every session's main jail.
