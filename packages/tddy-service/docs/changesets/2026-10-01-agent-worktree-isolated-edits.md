# 2026-10-01 — `ExecToolService` names a subagent conversation and gains `ConversationWorktree`

**Type:** Feature

`#agent-worktree` 1/4, PR [#560](https://github.com/uppin/tddy-coder/pull/560). Cross-package entry:
[2026-10-01-agent-worktree-isolated-edits.md](../../../../docs/dev/changesets/2026-10-01-agent-worktree-isolated-edits.md).

`exec_tools.proto`: `ExecuteToolRequest.conversation_id = 6` (empty means the session worktree) and
`rpc ConversationWorktree(ConversationWorktreeRequest) returns (ConversationWorktreeResponse)` with
`session_token, session_id, daemon_instance_id, conversation_id` and `oneof op { PullOp pull = 10;
RemoveOp remove = 11; }`; the response carries `result_json`. The hand-kept tonic adapter supplement
(`exec_tool_tonic_adapter_supplement.rs`) serves the RPC, and `packages/tddy-web/src/gen/exec_tools_pb.ts`
is regenerated. The 40 existing request literals set `conversation_id: String::new()`.
