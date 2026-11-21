# 2026-11-21 — An orchestrator checks and refreshes its base, and a start reports the commit it was cut from

`#orchestra 4/9` ([#982](https://github.com/uppin/tddy-coder/pull/982)).

- **`CheckBaseFreshness`** fetches the project's trunk and reports where the orchestrator's own home
  worktree stands against its tip: its commit, the tip, and how many commits ahead and behind.
- **`RefreshHome`** fast-forwards the home to the tip. A home with commits of its own is refused, naming
  where it stands, and left untouched.
- **Every start records its base.** `GetSessionStart` carries `base_commit` and `base_behind`: the commit
  the session's worktree was cut from and how far behind its upstream that was at cut time. Reporting only —
  a start from a stale base is not refused.
- A local start now reports its worktree being cut (`worktree_ready`), as a start on a target already did.
- Served on the host that holds the orchestrator; no peer forwarding yet. Nothing polls the remote.

See [Project concept § Keeping the base fresh](../project-concept.md#keeping-the-base-fresh).
