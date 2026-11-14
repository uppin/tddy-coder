# 2026-11-13 — A session follows its agent through `/clear`

**Type:** Bug Fix

`#orchestrators` 6/11, PR [#883](https://github.com/uppin/tddy-coder/pull/883), stack
[#889](https://github.com/uppin/tddy-coder/stack/889). Feature:
[claude-cli-session.md § Resume after `/clear`](../../ft/daemon/claude-cli-session.md#resume-after-clear).

## What changed

- **`tddy-coding-agent-specs`**: `ConversationSlot::DaemonSessionId { rotation: Option<RotationSlot> }`
  — `RotationSlot { ends, begins }` of `RotationHook { hook, when, id_field }`, with the pure
  `step(state, current, event, fields) -> RotationStep` (`Unchanged | Arm | RotateTo(id) | Disarm`).
  Both hooks are checked at load against `HooksAdapter::installed_events()`. `claude.yaml` declares
  `SessionEnd {reason: clear}` → `SessionStart {source: clear}`, `id_field: session_id`.
- **`tddy-agent-backend`**: `CLAUDE_HOOK_EVENTS` is the one `(event, matcher)` list;
  `CLAUDE_INSTALLED_EVENTS` is derived from it by a `const fn`.
- **`tddy-changeset`**: `SessionMetadata.agent_conversations: Vec<AgentConversation { id,
  started_at_unix_ms, reason: Start | Rotate }>` with `record_agent_conversation` (dedupe);
  `HookEvent.source`; `hook_event_fields` (every top-level string field).
- **`tddy-service`**: `ReportSessionStatusRequest.{agent_conversation_id, hook_fields}`.
- **`tddy-tools`**: `session-hook` forwards the event name, conversation id and fields on every report.
- **`tddy-session-activity`**: `follow_conversation_rotation` records a rotation after the status
  write; armed state is `<session dir>/.conversation-rotation-armed`, disarmed by any report that
  cannot complete it, so a crash between the two reports never arms a later, unrelated clear.
- **`tddy-agent-launch`**: resume binds the newest recorded conversation (`current_conversation_id`).
- ~60 `SessionMetadata` literals across test suites gained `agent_conversations: Vec::new()`.

## Measurements

- `tddy-tools/src/session_hook.rs`: 535 → 547 production lines; not split — stack-shared with n7
  (`stack-plan-tools`); record updated as regressed.
