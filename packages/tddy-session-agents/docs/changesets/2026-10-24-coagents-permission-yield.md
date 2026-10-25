# 2026-10-24 — Permission requests and answers on the conversation RPCs
**Type:** Feature

The final `AgentConversationChunk` carries `permission_request_json`; a resume's `permission_answer_json` is applied to the turn request (`permission_wire.rs`); `PermissionRequested` joins the stop-reason table. `service.rs` 1,149 → 1,155, recorded, not split (stack-shared). See [managed-codebase subagents](../../../../docs/ft/coder/managed-codebase-subagents.md#a-coding-subagents-permission-request-is-the-callers-to-answer) (#722, `#coagents` 3/8).
