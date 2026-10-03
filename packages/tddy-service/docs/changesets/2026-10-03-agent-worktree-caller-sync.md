# 2026-10-03 — `ConversationWorktreeRequest` gains a `SyncOp`

**Type:** Feature

PR [#576](https://github.com/uppin/tddy-coder/pull/576). Cross-package entry:
[2026-10-03-agent-worktree-caller-sync.md](../../../../docs/dev/changesets/2026-10-03-agent-worktree-caller-sync.md).

`exec_tools.proto`: `SyncOp sync = 15` in `ConversationWorktreeRequest.op`, `message SyncOp {}`. The
response's `result_json` is `{"sync": {commit, files, lines, paths, morePaths}}`, `{"sync": null}`
(no worktree, nothing new, or a recorded merge that changed no file), or
`{"conflicts": [paths], "moreConflicts": n}` (at most 20 paths). The generated TypeScript bindings
(`packages/tddy-web/src/gen/exec_tools_pb.ts`, `packages/tddy-rust-typescript-tests/gen/exec_tools_pb.ts`)
are regenerated.
