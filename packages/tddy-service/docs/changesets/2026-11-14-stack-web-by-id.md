# 2026-11-14 — Stack address on PR-stack requests; SessionEntry.stack_id

**Type:** Feature

`pr_stack.proto` gains `StackAddress` on the six `session_id`-addressed requests and the
`ListProjectStacks` / `GetProjectStack` RPCs with `ProjectStackSummary`, `StackNodeCounts` and
`UnreachableStackHost`. `session.proto` gains `SessionEntry.stack_id = 38`.
