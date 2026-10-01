# 2026-10-01 — `subagent_diff` is allowlisted beside `subagent_cancel`

**Type:** Feature

`#agent-worktree` 3/4, PR [#562](https://github.com/uppin/tddy-coder/pull/562). Cross-package entry:
[2026-10-01-agent-worktree-diff.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-diff.md).

`mcp__tddy-tools__subagent_diff` joins `SUBAGENT_TOOLS` in `claude_cli.rs`. It is read-only, so a
sandboxed agent can see what a conversation changed before taking it. Pinned by
`subagent_diff_is_allowlisted_wherever_subagent_cancel_is`.
