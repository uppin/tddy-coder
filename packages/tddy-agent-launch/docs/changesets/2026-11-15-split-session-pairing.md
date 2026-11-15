# 2026-11-15 — A split start reports its codebase half's branch and worktree

**Type:** Fix

`StartedSessionListings` answers the start ledger through a private
`ListedSessions` seam (`listed_session`); `reported_listing`
follows a split agent half's `codebase_*` pairing to the codebase host's
listing of its codebase half and reports that half's branch and worktree, with the agent half's own `display_name`.
It implements `StartedSessionDetails::listing` as #910 shaped it.

Cross-package record: [docs/dev/changesets/2026-11-15-split-session-pairing.md](../../../../docs/dev/changesets/2026-11-15-split-session-pairing.md).
