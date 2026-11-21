# 2026-11-21 — A target session's shell tab runs on the target

A shell tab opened **On target** on an ssh or Coder target session runs [on the target](../session-liveness.md#a-target-sessions-tabs)
— the target user's login shell, with its own environment, in the session's worktree there, through the session's
remote shell — and is recorded `[target, shell]`, so Resume reopens it on the target or drops it with the reason.
It writes no pid file and touches no session pointer; its failure ends only the tab. **On this host** tabs of a
target session run in the session's directory here.
