# 2026-11-21 — A target session's agent tab runs the agent on the target

An agent tab opened **On target** on an ssh or Coder target session runs [the agent on the
target](../session-liveness.md#a-target-sessions-tabs) — its bare command found on the target login's own `PATH`, in the
session's worktree there, in a chat minted there — so a daemon host with no copy of the agent opens one. It is
recorded `[target, agent]`, and Resume reopens it on the target, resuming its conversation where the harness can. No path
of the daemon's host reaches the target, and the tab's failure ends only the tab.
