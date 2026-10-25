# 2026-10-24 — A coding subagent's permission request yields to the caller
**Type:** Feature

`StopReason::PermissionRequested`, `PromptOutcome::permission_request` and `TurnRequest::permission_answer` (types in `subagent/permission.rs`); `prompt_outcome_json` emits `permissionRequest`; the conversation link parses the final frame's `permission_request_json` and encodes `permission_answer_json`; the model loop refuses a permission answer before calling the model. Code-issue rows added for `subagent.rs` (2,078), `subagent_runtime.rs` (740) and `roster/conversation.rs` (551) — not split, stack-shared. See [managed-codebase subagents](../../../../docs/ft/coder/managed-codebase-subagents.md#a-coding-subagents-permission-request-is-the-callers-to-answer) (#722, `#coagents` 3/8).
