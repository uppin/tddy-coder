# 2026-11-21 — A sandboxed session's agent tab runs in a jail of its own

An agent tab opened **In sandbox** on a sandboxed session runs the agent in a [jail of its own](../session-liveness.md#a-sandboxed-sessions-tabs)
— over the session's worktree and the agent's grants, in a conversation of its own — and keeps the session live
while it runs. It is recorded `[sandbox, agent]`; Resume reopens it in a new jail, resuming its conversation where
the harness can and saying so where it cannot. The web's "In sandbox" group lists every agent kind.
