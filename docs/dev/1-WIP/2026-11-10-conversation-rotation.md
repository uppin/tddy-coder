# Changeset: a session follows its agent through `/clear`

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Bug Fix
**Complexity**: 4 — more than one crate (coding-agent-specs, agent-backend, tddy-tools hook, session-activity, changeset, agent-launch), a proto contract change (`ReportSessionStatusRequest`), a record-format change
**Stack**: `#orchestrators` 6/11 · branch `feature/orchestrators/conversation-rotation` · base `feature/orchestrators/session-lifecycle-tools` (#882)

## Initial Discovery

[2026-11-10-conversation-rotation-initial-discovery.md](./2026-11-10-conversation-rotation-initial-discovery.md)

## Prerequisites

### ⚠ DURING — A claude-cli agent tab reports its activity as the session's — [`2026-10-24-claude-agent-tab-shares-session-hooks.md`](../todo/2026-10-24-claude-agent-tab-shares-session-hooks.md)

⚠ Directly in the path: a `/clear` inside an **agent tab** would arrive on the session's hook and rotate
the session's main conversation. A rotation is recorded only when the hook's `cwd`/agent identity is the
session's main agent; an agent-tab report is ignored for rotation. Not claimed (attribution of activity
stays as the entry describes).

### ⚠ DURING — `session_hook.rs` is 535 lines — [`oversized-file-session-hook.md`](../../../packages/tddy-tools/docs/code-issues/oversized-file-session-hook.md)

Forwarding the conversation fields is a small addition in `session_status_request`; it must not grow the
file further — the rotation matching lives in `tddy-changeset::session_activity` beside `HookEvent`.

### ⚠ DURING — resume-path complexity — [`complexity-cli-agent-steps-resumed-session-wiring.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-cli-agent-steps-resumed-session-wiring.md), [`complexity-svc-resume-session-resume-session-at-session-coordinate.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-session-resume-session-at-session-coordinate.md)

The newest id is chosen in one helper (`current_conversation_id(meta, spec)`) called where
`LaunchConversation::SessionId` is built (`cli_agent_steps/conversation.rs:124`); neither function grows.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10).

## Affected Packages

- [tddy-coding-agent-specs](../../../packages/tddy-coding-agent-specs/README.md) — `conversation.rotation` slot; `claude.yaml`
- [tddy-agent-backend](../../../packages/tddy-agent-backend/README.md) — claude hook settings carry what the slot names
- [tddy-tools](../../../packages/tddy-tools/README.md) — `session-hook` forwards conversation fields
- [tddy-service](../../../packages/tddy-service/) — `ReportSessionStatusRequest` conversation fields
- [tddy-session-activity](../../../packages/tddy-session-activity/README.md) — record the rotation
- [tddy-changeset](../../../packages/tddy-changeset/README.md) — `SessionMetadata.agent_conversations`; `HookEvent.source`
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — resume binds the newest id

## Related Feature Documentation

- PRD: [PRD-2026-11-10-conversation-rotation.md](../../ft/daemon/1-WIP/PRD-2026-11-10-conversation-rotation.md)
- [Claude CLI session](../../ft/daemon/claude-cli-session.md), [Coding agent specs](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md)

## Summary

The agent spec declares how a conversation rotation is reported; claude's: its `SessionStart` hook with
`source: clear`, carrying the new id in `session_id`. The hook forwards it, the daemon appends it to
`.session.yaml` `agent_conversations` (every id kept, newest last), and resume continues the newest.

## Background

Claude's conversation id is the tddy session id (`--session-id` at start, `--resume <tddy id>` at
resume). After `/clear` a resume — including main's resume after auto-stop — brings back the pre-clear
conversation. The developer asked for a generic, agent-YAML-declared fix (2026-11-10) and for **every**
conversation id to be kept on the session record (2026-11-10).

## Responsibility

- `ConversationSlot` gains `rotation: Option<RotationSlot { hook: String, when: BTreeMap<String,String>,
  id_field: String }>`, validated at load against the hook events the agent's hooks adapter installs.
- `HookEvent.source` parsed; `ReportSessionStatusRequest.{agent_conversation_id, hook_fields}`.
- `SessionMetadata.agent_conversations: Vec<AgentConversation { id, started_at_unix_ms, reason: Start |
  Rotate }>` — the first entry written at start.
- `current_conversation_id(meta, spec)`; resume and transcript/usage readers use it.

## Boundaries

- Does **not** change session ids, records or worktrees.
- Does **not** fix agent-tab activity attribution (todo above).
- Does **not** add any orchestrator tool.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1–n5 (#878–#882) | role, store, effort, turn control, marks | nothing — line predecessors. n1's main resume path is today's `EnsureProjectHomeSession`, unchanged by n1 | touch their surfaces |

## Draft PR contract

- `RotationSlot` + loader key; `HookEvent.source`; proto fields + gen; `AgentConversation` +
  `SessionMetadata.agent_conversations`; `current_conversation_id` signature
  (`TODO(conversation-rotation)`).
- Failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes
**Concurrent with:** n1, n2, n3, n4, n5
**Blocks:** none

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] Spec slot + `claude.yaml` (`SessionStart`, `when: {source: clear}`, `id_field: session_id`)
- [ ] Hook parse + forward; proto fields
- [ ] Record: first entry at start, append on rotation, dedupe; agent-tab reports ignored
- [ ] Resume binds the newest; transcript/usage readers read the current conversation
- [ ] Main's resume after auto-stop continues the post-clear conversation

## Technical Changes

### State A (Current)

- `claude.yaml:47` `conversation: { kind: daemon_session_id }`; start `--session-id`, resume
  `--resume {session_id}` (`cli_agent_steps/conversation.rs:85-86,124`).
- `HookEvent` (`tddy-changeset/src/session_activity.rs:137-158`) parses Claude's `session_id`, drops
  `source`; `session_status_request` (`tddy-tools/src/session_hook.rs:144-205`) forwards neither;
  `report_session_status_impl` (`tddy-session-activity/src/service/hook_reports.rs:159-222`) updates
  activity status and pending question only.
- Hook settings `tddy-agent-backend/src/claude_hooks.rs:84-133` install `SessionStart` (matcher "").
- Transcript usage reads `<claude_home>/.claude/projects/<cwd>/<session_id>.jsonl`
  (`claude/transcript_usage.rs:37`).
- `SessionMetadata` is `deny_unknown_fields`.

### State B (Target)

- A matching hook report appends `{id, now, Rotate}`; resume passes `--resume <newest id>`; readers open
  the newest transcript.

### Delta

- Specs slot + validation; claude hooks unchanged in event set; hook CLI forwards; proto; activity
  service records; changeset model; agent-launch resume binding; backend transcript readers.

## Implementation Milestones

- [ ] Slot + spec tests
- [ ] Hook → report → record
- [ ] Resume binding
- [ ] Readers
- [ ] Main after auto-stop

## Testing Plan

**Level**: spec-load unit; hook CLI acceptance (stdin JSON → request); daemon integration over the
activity service; spec-launch acceptance asserting the resume argv. No real claude binary.

## Acceptance Tests

### tddy-coding-agent-specs — `packages/tddy-coding-agent-specs/tests/spec_conversation_rotation_acceptance.rs`
- `claude_declares_a_clear_rotation_on_session_start`
- `a_rotation_naming_a_hook_the_adapter_does_not_install_is_refused_at_load`

### tddy-tools — `packages/tddy-tools/tests/session_hook_conversation_rotation.rs`
- `a_session_start_hook_with_source_clear_forwards_the_new_conversation_id`

### tddy-session-activity — `packages/tddy-session-activity/tests/conversation_rotation_record.rs`
- `a_rotation_appends_the_new_conversation_and_keeps_every_earlier_one`
- `the_same_conversation_reported_twice_is_one_entry`
- `an_agent_tab_clear_does_not_rotate_the_sessions_conversation`

### tddy-agent-launch — `packages/tddy-agent-launch/tests/spec_launch_conversation_rotation.rs`
- `resuming_after_a_clear_continues_the_post_clear_conversation`
- `a_session_with_no_conversation_list_resumes_by_its_session_id`

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/project_home_idle_stop_acceptance.rs` (new case)
- `main_auto_stopped_after_a_clear_resumes_the_post_clear_conversation`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Generic slot** (developer, 2026-11-10); claude is the only spec that declares one now.
- **Keep every id** (developer, 2026-11-10): history is cheap and lets a later tool read an earlier
  conversation's transcript.
- **Older daemons cannot read** a record with `agent_conversations` (`deny_unknown_fields`).

## Refactoring Needed

### From @red (TDD Red Phase)
### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### Change Validation (@validate-changes)
### Test Validation (@validate-tests)
### Production Readiness (@prod-ready)
### Code Quality (@analyze-clean-code)

## Successor PRs

n7 `feature/orchestrators/stack-plan-tools`.

## TODO

- [x] Record initial discovery (`2026-11-10-conversation-rotation-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
