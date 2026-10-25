# 2026-10-24 — `subagent_resume` answers a permission request
**Type:** Feature

`subagent_resume` takes `permission: { requestId, optionId | cancelled: true }` (`permission_answer_arg.rs`: schema property and parse; exactly one of the two is accepted). `server.rs` +6, recorded, not split (stack-shared). See [managed-codebase subagents](../../../../docs/ft/coder/managed-codebase-subagents.md#a-coding-subagents-permission-request-is-the-callers-to-answer) (#722, `#coagents` 3/8).
