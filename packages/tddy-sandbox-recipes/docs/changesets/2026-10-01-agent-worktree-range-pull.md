# 2026-10-01 — `subagent_pull` is allowlisted beside `subagent_cancel`

**Type:** Feature

`#agent-worktree` 4/4, PR [#563](https://github.com/uppin/tddy-coder/pull/563). Cross-package entry:
[2026-10-01-agent-worktree-range-pull.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-range-pull.md).

`mcp__tddy-tools__subagent_pull` joins `SUBAGENT_TOOLS` in `claude_cli.rs`: an agent allowed
`subagent_end` and not `subagent_pull` could only take work by ending the conversation. Pinned by
`subagent_pull_is_allowlisted_wherever_subagent_cancel_is`.
