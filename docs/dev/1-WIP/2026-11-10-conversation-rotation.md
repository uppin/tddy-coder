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

The agent spec declares how a conversation rotation is reported; claude's: its `SessionEnd` hook with
`reason: clear` for the session's current conversation, followed by its `SessionStart` hook with
`source: clear`, carrying the new id in `session_id`. The hook forwards both, the daemon appends it to
`.session.yaml` `agent_conversations` (every id kept, newest last), and resume continues the newest.

## Background

Claude's conversation id is the tddy session id (`--session-id` at start, `--resume <tddy id>` at
resume). After `/clear` a resume — including main's resume after auto-stop — brings back the pre-clear
conversation. The developer asked for a generic, agent-YAML-declared fix (2026-11-10) and for **every**
conversation id to be kept on the session record (2026-11-10).

## Responsibility

- `ConversationSlot::DaemonSessionId` gains `rotation: Option<RotationSlot { ends: RotationHook,
  begins: RotationHook }>` (each `RotationHook { hook, when, id_field }`), both hooks validated at
  load against the hook events the agent's hooks adapter installs (shape revised 2026-11-13).
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

Refined at pass 2 (2026-11-13): no test of this node reaches a `TODO(<n1–n5>)` stub. The only
inherited touch is mechanical — `SessionMetadata` literals gain `agent_conversations: Vec::new()`
beside n5's `orchestration_mark: None` (48 files). `project_home_idle_stop_acceptance.rs` also holds
n1's `idle_auto_stop_leaves_an_orchestrator_running`, which stays red on `TODO(orchestrator-role)`
until n1 is green; this node's new case there does not depend on it.

## Draft PR contract

- `RotationSlot` + loader key; `HookEvent.source`; proto fields + gen; `AgentConversation` +
  `SessionMetadata.agent_conversations`; `current_conversation_id` signature
  (`TODO(conversation-rotation)`).
- Failing tests below.

Published at pass 2 (every stub marked `TODO(conversation-rotation)`, each keeping today's behaviour):

| Symbol | File | Stub |
|---|---|---|
| `RotationSlot { ends: RotationHook, begins: RotationHook }`, `RotationSlot::step(RotationState, current, event, &HashMap) -> RotationStep`, `RotationSlot::hooks() -> [&str; 2]` | `tddy-coding-agent-specs/src/slots/conversation.rs` | `step` → `Unchanged`; `hooks` real |
| `RotationHook { hook, when: BTreeMap, id_field }`, `RotationHook::conversation_of(event, &HashMap) -> Option<String>` | same | `None` |
| `RotationState { Idle, Armed }` (`Default` = `Idle`), `RotationStep { Unchanged, Arm, RotateTo(String) }` | same | real types |
| `ConversationSlot::DaemonSessionId { rotation: Option<RotationSlot> }` (was a unit variant), `ConversationSlot::rotation()` | same | real |
| `HooksAdapter::installed_events() -> &'static [&'static str]` (default method); the load check refuses a rotation unless **both** `hooks()` are installed | `tddy-coding-agent-specs/src/slots/hooks.rs` | `&[]` for every adapter; check not yet written |
| `HookEvent.source` | `tddy-changeset/src/session_activity.rs` | real (parsed) |
| `hook_event_fields(stdin_json) -> HashMap<String,String>` | same, re-exported at the crate root | empty |
| `AgentConversation { id, started_at_unix_ms, reason }`, `AgentConversationReason { Start, Rotate }`, `record_agent_conversation(&mut Vec, next) -> bool` | `tddy-changeset/src/agent_conversation.rs` (new) | `false`, no append |
| `SessionMetadata.agent_conversations` (`default`, skipped when empty) | `tddy-changeset/src/session_metadata.rs` | real |
| `ReportSessionStatusRequest.agent_conversation_id = 9`, `.hook_fields = 10` (`map<string,string>`) + TS gen | `tddy-service/proto/activity.proto`, `tddy-client-web/src/gen`, `tddy-rust-typescript-tests/gen` | hook sends them empty |
| `current_conversation_id(meta, spec) -> Option<&str>` (pub, crate root) | `tddy-agent-launch/src/cli_agent_steps/conversation.rs` | the session id; not yet called by `resumed_conversation` |

Design refinements pass 2 settled (green follows them):

- **A rotation is two reports, correlated** (developer decision 2026-11-13). The declared shape is

  ```yaml
  conversation:
    kind: daemon_session_id
    rotation:
      ends:   { hook: SessionEnd,   when: { reason: clear }, id_field: session_id }
      begins: { hook: SessionStart, when: { source: clear }, id_field: session_id }
  ```

  Green adds it to `claude.yaml`. `step` arms on an `ends` whose id is the session's **current**
  conversation (newest listed, else the session id) and rotates on the next `begins`; an `ends` of
  any other id — a claude-cli agent tab sharing the session's hooks — and a `begins` nothing armed
  are `Unchanged`. Where the daemon keeps `RotationState` between two reports is green's call
  (an in-memory per-session entry on `ActivityServiceImpl` suffices: the pair is milliseconds
  apart; `.session.yaml` would need a new field). This closes the agent-tab part of the DURING
  prerequisite for rotation only; activity attribution stays as the todo describes.
- **The hook matches nothing.** `session-hook` forwards `hook_event_name` (today it is set only for
  question events — green sets it on every report), `agent_conversation_id` = the event's
  `session_id`, and `hook_fields` = `hook_event_fields(stdin)` (the event's top-level string
  fields). The daemon (`hook_reports.rs`) looks the session's spec up (`for_session_type`) and asks
  `conversation().rotation()?.rotated_to(..)`. So the matching lives on `RotationSlot` in
  `tddy-coding-agent-specs`, not in `tddy-changeset` beside `HookEvent` as Prerequisites said —
  `tddy-changeset` cannot see a spec; what `tddy-changeset` holds is the field extraction, which
  keeps `session_hook.rs` (713 lines now, not 535) from growing.
- **`tddy-agent-backend` adds no hook.** `SessionStart` is already installed with matcher `""` and
  `source` arrives on stdin; its only change is to expose the event list
  `build_claude_hooks_settings` installs (today a local `[(&str,&str); 8]`) so
  `ClaudeSettingsLocal::installed_events` can answer it. Cursor's adapter answers its own.
- **A rotation lives on `daemon_session_id` only** — the one kind whose bound id a rotation
  replaces. `minted` could grow it later.
- **A legacy session (no list) rotating** is seeded with its own id as `Start` before the rotation
  is appended, so no id it had is lost (developer decision 2026-11-13).
- **`hook_fields` carries every top-level string field** of the event, unnarrowed (developer
  decision 2026-11-13) — `UserPromptSubmit`'s prompt text included.
- Existing test `cli_spec_features_unit.rs::each_builtin_cli_agent_converses_the_way_its_launch_resumes_it`
  now expects claude's rotation and is red until `claude.yaml` declares it.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes
**Concurrent with:** n1, n2, n3, n4, n5
**Blocks:** none

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [x] Spec slot + `claude.yaml` (`ends: SessionEnd {reason: clear}`, `begins: SessionStart {source: clear}`, `id_field: session_id`)
- [x] Hook parse + forward; proto fields
- [x] Record: first entry at start, append on rotation, dedupe; agent-tab reports ignored
- [x] Resume binds the newest; transcript/usage readers read the current conversation
- [x] Main's resume after auto-stop continues the post-clear conversation

## Technical Changes

### State A (Current)

Re-verified 2026-11-13 against b85a5a304 (before this node's contract):

- `claude.yaml:53` (moved from :47) `conversation: { kind: daemon_session_id }`; argv `:21`
  `{ if: resume, then: [--resume, {session_id}], else: [--session-id, {session_id}] }`. Start binds
  the session id at `cli_agent_steps/conversation.rs:95-96`, resume at `:134` (`resumed_conversation`,
  reached through `resumed_or_adopted_conversation` from `opencode_cli_spawn/resume.rs:193` and
  `svc_resume_sandboxed_cli_harness_session.rs:231` — the generic path main's resume also takes).
- `HookEvent` (`tddy-changeset/src/session_activity.rs:137-158`) parses Claude's `session_id`, drops
  `source`; `session_status_request` (`tddy-tools/src/session_hook.rs:149-210`, file 713 lines)
  forwards neither, and sets `hook_event_name` only for question events;
  `report_session_status_impl` (`tddy-session-activity/src/service/hook_reports.rs:159-222`) updates
  activity status and pending question only.
- Hook settings `tddy-agent-backend/src/claude_hooks.rs:84-133` install `SessionStart` (matcher "")
  and `SessionEnd`, among 8.
- **Moved:** `claude/transcript_usage.rs` no longer exists. Usage is `tddy-usage` reading claude.yaml
  `usage.sources` (`{agent_home}/.claude/projects/*/{conversation_id}.jsonl`), the id bound from the
  launch's `UsageSubject::new(conversation.usage_id(), ..)` (`svc_resume_sandboxed_cli_harness_session.rs:283`,
  `opencode_cli_spawn/resume.rs:242`). So once a resume binds the newest id, the usage readers follow
  with no change of their own.
- **Moved:** a project-home main is given **no** session environment (`notify_env.rs`
  `agent_notify_unless_role`), so `TDDY_SESSION_ID` cannot tell main's hook from an agent tab's.
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
- `an_agent_tab_clear_does_not_rotate_the_sessions_conversation` — a tab's clear of its own
  conversation, then the session's own clear: only the session's is listed (decision 2026-11-13)

### tddy-agent-launch — `packages/tddy-agent-launch/tests/spec_launch_conversation_rotation.rs`
- `resuming_after_a_clear_continues_the_post_clear_conversation`
- `a_session_with_no_conversation_list_resumes_by_its_session_id`

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/project_home_idle_stop_acceptance.rs` (new case)
- `main_auto_stopped_after_a_clear_resumes_the_post_clear_conversation`

### Unit / integration (pass 2, 6f)
- `tddy-changeset/src/agent_conversation.rs` (inline): append, dedupe, empty list, YAML shape
- `tddy-changeset/tests/hook_event_fields.rs`: string fields kept, non-strings dropped, non-object, `source` parsed
- `tddy-coding-agent-specs/tests/conversation_rotation_unit.rs`: `step` arms on the current end, rotates on the following begin, ignores another conversation's end, an unarmed begin and a non-clear start; `conversation_of` match / no-`when` / other event / blank id; `rotation()`; `hooks()`
- `tddy-tools/tests/session_hook_conversation_rotation.rs`: every report names its conversation
- `tddy-session-activity/tests/conversation_rotation_record.rs`: legacy session seeded with its own id; a `resume` SessionStart rotates nothing
- `tddy-agent-launch/src/cli_agent_steps/cli_agent_steps_tests.rs`: `a_resume_binds_the_conversation_the_agent_rotated_to`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Generic slot** (developer, 2026-11-10); claude is the only spec that declares one now.
- **Keep every id** (developer, 2026-11-10): history is cheap and lets a later tool read an earlier
  conversation's transcript.
- **Older daemons cannot read** a record with `agent_conversations` (`deny_unknown_fields`).
- **Tab clears: correlate `SessionEnd`** (developer, 2026-11-13): a rotation needs a `SessionEnd`
  `{reason: clear}` for the session's current conversation followed by the next `SessionStart`
  `{source: clear}`; a tab's clear ends a different id and rotates nothing. Declared in agent YAML as
  `rotation: { ends, begins }`; both hooks must be installed by the adapter, checked at load.
- **Hook fields: all top-level string fields** (developer, 2026-11-13), no narrowing.
- **Older sessions: seed the list** (developer, 2026-11-13): a session with no list is seeded with
  its own id as `Start` on its first rotation.

- **Rotation state is a marker file** (green, 2026-11-13): `RotationState` between the `SessionEnd`
  and `SessionStart` reports is `<session dir>/.conversation-rotation-armed`
  (`tddy-session-activity/src/service/conversation_rotation.rs`). Not the in-memory entry this
  changeset suggested: `activity_service()` builds a fresh `ActivityServiceImpl` per call, so memory
  does not survive from one report to the next. Not a `.session.yaml` field either — that is
  `deny_unknown_fields` and would touch every `SessionMetadata` literal again. The record is re-read
  after the status write before it is rewritten with the new conversation.
- **Load check**: `HooksAdapter::installed_events()` — claude answers `CLAUDE_INSTALLED_EVENTS`
  (`tddy-agent-backend`), cursor answers `CURSOR_HOOK_EVENTS`; `parse_section` refuses a rotation
  naming a hook outside it.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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
