# 2026-11-22 — The reader's lost messages, and a question held by its PermissionRequest hook

**Type:** Fix · PR #997 · Cross-package: [docs/dev/changesets/2026-11-22-coder-target-back-channel.md](../../../../docs/dev/changesets/2026-11-22-coder-target-back-channel.md)

- `hook_decision::question_decision_json_for(event, answer, tool_input)`: the `PermissionRequest` shape is `{behavior: allow, updatedInput}`.
- `ProjectNotificationLog::messages_lost_after` (`project_notification_log/evicted_messages.rs`, new): `ReadSessionMessages`' `lost` counts only evicted messages addressed to the reader.
- `AnswerRefusal::UnknownRequest` names a request, not a permission request, and says what to do.
