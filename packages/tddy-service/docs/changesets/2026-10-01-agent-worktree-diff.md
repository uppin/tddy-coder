# 2026-10-01 — `ConversationWorktreeRequest` gains a `DiffOp`

**Type:** Feature

`#agent-worktree` 3/4, PR [#562](https://github.com/uppin/tddy-coder/pull/562). Cross-package entry:
[2026-10-01-agent-worktree-diff.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-diff.md).

`exec_tools.proto`: `DiffOp diff = 13` in `ConversationWorktreeRequest.op`, with `DiffOp { string
from = 1; string to = 2; }` (empty means omitted: the base, the tip). The response's `result_json` is
`{"diff": {from, to, files, lines, diff, truncated}}`. `pull`, `remove` and `reset` are unchanged. The
generated TypeScript bindings (`packages/tddy-web/src/gen/exec_tools_pb.ts`) are regenerated.
