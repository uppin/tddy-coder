# 2026-10-01 — `ConversationWorktreeRequest` gains a `ResetOp`

**Type:** Feature

`#agent-worktree` 2/4, PR [#561](https://github.com/uppin/tddy-coder/pull/561). Cross-package entry:
[2026-10-01-agent-worktree-rewind-reset.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-rewind-reset.md).

`exec_tools.proto`: `ResetOp reset = 12` in `ConversationWorktreeRequest.op`, with `ResetOp { string
commit = 1; }` (empty names the conversation's base). The response's `result_json` is
`{"reset": {"to", "droppedCommits"}}`, or `{"reset": null}` when the conversation has no worktree.
`pull` and `remove` are unchanged. The generated TypeScript bindings
(`packages/tddy-web/src/gen/exec_tools_pb.ts`, `packages/tddy-rust-typescript-tests/gen/exec_tools_pb.ts`)
are regenerated.
