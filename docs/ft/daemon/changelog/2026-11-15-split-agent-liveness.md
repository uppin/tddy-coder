# 2026-11-15 — A split session's agent either comes up or the start fails with its own words

A split session's agent terminal is recorded in its session directory, every ended `main`
terminal is logged with its real outcome and lifetime, and a split start whose agent exits
inside the daemon's startup window fails, quoting the agent's last output, with nothing left
behind on either host. See
[remote-managed-worktree.md § Session start](../remote-managed-worktree.md#session-start).
