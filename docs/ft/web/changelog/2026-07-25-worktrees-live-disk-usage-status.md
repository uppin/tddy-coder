# 2026-07-25 — Worktrees: live disk-usage status

- Each worktree now tracks its on-disk size independently with a **None / Calculating / Cached** status and a "last calculated" time; sizes are computed lazily and centrally rate-limited (at most two walks at once) instead of one eager project-wide sweep. See [worktree-disk-usage-streaming.md](../worktree-disk-usage-streaming.md).
- The **Worktrees** screen streams results live — a first snapshot, then each worktree's size fills in as it finishes — with **Recalculate all** and per-row **Calculate** controls.
- The Session Inspector **Worktree** tab shows the same live status and its Refresh re-triggers the calculation, replacing the old 10-minute poll.
