# 2026-07-25 — PR-Stack: branch-keyed live status & repoint

- A Planned PR is assigned a **definitive branch at creation** — the remote branch name is now the durable link between the planned PR, its worktree/session, and its GitHub PR ([pr-stack-live-status.md](../pr-stack-live-status.md)).
- The PR-Stack view resolves each node's **in-progress session by branch**, and polls GitHub (every 5 s) to show the **PR number as a link plus its state** (open/merged/closed/draft) — live, without the orchestrator agent running.
- A **Repoint control** re-points a node whose predecessor has merged: it drops the merged parent, rebases the node's local branch onto the new effective base, and re-targets the open PR's base branch.
- **Bug fix — the stack sequence is now respected on spawn.** Starting a session for a planned node branches off its parent node's effective base (skipping merged ancestors), not the default branch as before; starting a node before its non-merged parent is refused ([pr-stacking.md](../pr-stacking.md)).
