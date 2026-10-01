# 2026-10-01 — `ConversationWorktreeRequest` gains a `PullRangeOp`

**Type:** Feature

`#agent-worktree` 4/4, PR [#563](https://github.com/uppin/tddy-coder/pull/563). Cross-package entry:
[2026-10-01-agent-worktree-range-pull.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-range-pull.md).

`exec_tools.proto`: `PullRangeOp pull_range = 14` in `ConversationWorktreeRequest.op`, with
`PullRangeOp { string from = 1; string to = 2; repeated string already_pulled = 3; }` (empty bound means
omitted). The response's `result_json` is `{"pulled": {commits, skipped, files, lines, conflicts} | null}`.
`pull`, `remove`, `reset` and `diff` are unchanged. The generated TypeScript bindings
(`packages/tddy-web/src/gen/exec_tools_pb.ts`) are regenerated.
