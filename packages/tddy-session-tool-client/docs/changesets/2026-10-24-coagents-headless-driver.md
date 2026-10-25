# 2026-10-24 — `request_approval` over the tool transport

**Type:** Feature

`#coagents` 7/8 ([#726](https://github.com/uppin/tddy-coder/pull/726)). New `approval` module: `request_approval(conversation_id, tool_name, input)` calls `ExecToolService/RequestApproval` over whichever transport tool calls use (sandbox IPC, daemon UDS, daemon HTTP, LiveKit), bounded by `APPROVAL_CLIENT_BOUND` (70 s); a missing route or a failed call is an `Err`, never an allow. Not cut at `MAX_REMOTE_BLOCK_MS` (test: `approval_request_is_not_cut_at_the_remote_block_cap`).

Code issues: new `complexity-approval-request-approval` (104 lines, duplicates `dispatch_session_tool`'s transport match), deferred to the sweep node.
