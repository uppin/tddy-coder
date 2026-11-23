# 2026-11-23 — A tmux-run target session does not reconnect by itself

**Category:** Future enhancement
**Source:** `docs/dev/1-WIP/2026-11-23-target-tmux.md` (`#target-tmux` 1/2)

When the `coder ssh`/`ssh` client of a tmux-run target session drops, the agent keeps running in
tmux on the target but the session lists inactive until the operator resumes it, which re-attaches.

**Why deferred:** the developer chose inactive + resume for this stack; an automatic reconnect needs
a re-dial loop with backoff and must tell a dropped transport from an agent that exited (the
client's exit status alone does not, see `2026-11-22-coder-target-stop-reads-exit-code-1.md`).

**What would close it:** on a main exit of a tmux-run target session, probe `tmux has-session` on
the target; when it is alive, re-run the attach with backoff instead of ending the session.
