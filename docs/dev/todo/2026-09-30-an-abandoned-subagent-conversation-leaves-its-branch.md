# 2026-09-30 — An abandoned subagent conversation leaves its branch behind

**Category:** Future enhancement
**Source:** `#agent-worktree` stack, `2026-09-30-agent-worktree-isolated-edits` changeset

A conversation worktree is removed by `subagent_end` or `subagent_cancel`. A `tddy-tools` process
that exits (or is killed) with conversations still open does neither, leaving:

- `<session worktree>/tmp/subagent-worktrees/<conv>` — removed with the session worktree, but still
  registered in `git worktree list` until `git worktree prune`;
- branch `tddy/subagent/<session>/<conv>` in the project's common git dir — never removed.

## Why it was deferred

Nothing in the daemon knows which conversations a `tddy-tools` process holds; the ids are minted
in-process. A sweep needs either the daemon to record every conversation worktree it creates (and
remove them at session teardown) or a startup prune of `tddy/subagent/<session>/*` for sessions that
no longer exist. Both are session-lifecycle work, orthogonal to the capability.

Related: [`2026-08-13-session-deletion-leaks-the-worktree-for-every-session-type-except-clau.md`](2026-08-13-session-deletion-leaks-the-worktree-for-every-session-type-except-clau.md).
