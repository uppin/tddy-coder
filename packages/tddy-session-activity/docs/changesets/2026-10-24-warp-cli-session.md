# 2026-10-24 — warp-cli worktrees removed on delete

**Type:** Feature

`#warp-agent` 1/2, PR [#717](https://github.com/uppin/tddy-coder/pull/717). Cross-package entry:
[2026-10-24-warp-cli-session.md](../../../../docs/dev/changesets/2026-10-24-warp-cli-session.md).

`worktree_removal_applies_to` includes `warp-cli` (with `claude-cli`, `opencode-cli`, `muse-cli`,
`workspace`). Test: `tests/worktree_removal_eligibility.rs` —
`a_warp_cli_session_is_eligible_for_worktree_removal`.
