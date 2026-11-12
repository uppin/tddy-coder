# Changeset: orchestrators drive a session's turns

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
**Type**: Feature
**Complexity**: 4 — more than one crate (coding-agent-specs, tool-engine, daemon-rpc, agent-launch, session-activity), a new spec slot, cross-host forwarding, input ordering against a live terminal
**Stack**: `#orchestrators` 4/11 · branch `feature/orchestrators/session-turn-control` · base `feature/orchestrators/start-effort` (#880)

## Initial Discovery

[2026-11-10-session-turn-control-initial-discovery.md](./2026-11-10-session-turn-control-initial-discovery.md)

## Prerequisites

### ⚠ DURING — A spawner reads only the children on its own host — [`2026-10-29-spawner-read-scope-is-same-host-only.md`](../todo/2026-10-29-spawner-read-scope-is-same-host-only.md)

The two new tools are project-home (orchestrator-role) tools, forwarded by `served_on_target_host` like
the terminal tools, so they are not subject to the spawner scope check. A conversation spawner is not
offered them. Not claimed.

### ⚠ DURING — A conversation id is not bound to the session that opened it — [`2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`](../todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md)

`SendSessionMessage` addresses a **session** (project-scoped, target rules apply), never a
conversation id, so it does not widen that gap. Not claimed.

### ⚠ DURING — A claude-cli agent tab reports its activity as the session's — [`2026-10-24-claude-agent-tab-shares-session-hooks.md`](../todo/2026-10-24-claude-agent-tab-shares-session-hooks.md)

A turn ending in an agent tab is reported as the session's; `started_by_me` filtering inherits that
attribution. Not claimed.

### ⚠ DURING — A claude-cli session's transcript holds tool calls only — [`2026-10-25-claude-cli-transcript-has-no-conversation-text.md`](../todo/2026-10-25-claude-cli-transcript-has-no-conversation-text.md)

After a turn ends, an orchestrator reads the result with `ReadSessionTranscript` / `ReadSessionTerminal`;
for claude-cli it sees tool calls and the screen, not the reply text. Known limit of "read result";
recorded, not claimed.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10).

## Affected Packages

- [tddy-coding-agent-specs](../../../packages/tddy-coding-agent-specs/README.md) — `turn_input` slot; `claude.yaml`
- [tddy-tool-engine](../../../packages/tddy-tool-engine/README.md) — `SendSessionMessage`, `ResumeSession`, `started_by_me`
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — ports, routing, forwarding
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — resume with a first message
- [tddy-session-activity](../../../packages/tddy-session-activity/README.md) — records carry the orchestrator; filter

## Related Feature Documentation

- PRD: [PRD-2026-11-10-session-turn-control.md](../../ft/daemon/1-WIP/PRD-2026-11-10-session-turn-control.md)
- [Project concept § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools), [Session notifications](../../ft/daemon/session-notifications.md)

## Summary

`SendSessionMessage` gives a running session's agent a new turn, by the method its YAML spec declares.
`ResumeSession` resumes a stopped session, optionally with a first message. Project notification
records name the session's orchestrator, and the two notification tools take `started_by_me` — so an
orchestrator can do what `/yolo-pr-stack` does with `SendMessage` and task notifications.

## Background

The developer wants home and orchestrator sessions to control sessions "as if they were subagents"
(2026-11-10). Today they can start, follow, answer, type keys and terminate, but cannot hand a running
session a turn without taking its terminal lease, cannot resume one, and must read every session's
notifications to learn that their own child finished.

## Responsibility

- `CodingAgentSection.turn_input: Option<TurnInputSlot>` = `TerminalPaste { submit_key } | AcpPrompt`
  (validated at load); `claude.yaml` declares `TerminalPaste { submit_key: Enter }`.
- Tools `SendSessionMessage`, `ResumeSession` (tool-engine module `project_home_turn_tools.rs`, daemon
  port, catalogue, routing, forwarding).
- Delivery: terminal paste through the terminal server **without** a lease change; ACP prompt through
  the session's ACP connection.
- Refusals: no `turn_input`, stopped (→ `ResumeSession`), pending question/permission (→ answer tools).
- `ProjectNotificationRecord.orchestrator_session_id`; `started_by_me` on Wait/Read notifications.

## Boundaries

- Does **not** add marks, delete or worktree removal (n5).
- Does **not** change `SendSessionKeys`, `ClaimSessionTerminal`, the answer tools or target rules.
- Does **not** add orchestrator-role sessions (n1 does; these tools reach them through n1's predicate
  once both are merged — no code here depends on it).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `orchestrator-role` (#878) | `is_orchestrator_role` | nothing directly — tools are registered as project-home tools, which n1's predicate already routes | re-implement role checks |
| n2 (#879), n3 (#880) | stack store, effort | nothing — line predecessors | touch them |

## Draft PR contract

- `TurnInputSlot` and the `turn_input:` key in the spec loader.
- Tool defs `SendSessionMessage`, `ResumeSession` (names, schemas) and `is_project_home_turn_tool`;
  port methods `send_session_message`, `resume_session` (`TODO(session-turn-control)`).
- `started_by_me` in the two notification tool schemas; `orchestrator_session_id` on the record DTO.
- Failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — tests use main (project-home) as the caller
**Concurrent with:** n1, n2, n3, n5, n6
**Blocks:** n10 `stack-git-tools` (`pr_resolve_conflicts { delegate }` sends the node's session a message)

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] `turn_input` slot + `claude.yaml`
- [ ] `SendSessionMessage` (terminal paste + submit, ACP prompt), refusals
- [ ] `ResumeSession` (+ first message)
- [ ] Notification records name the orchestrator; `started_by_me`
- [ ] Cross-host forwarding for both tools
- [ ] Docs via wrap

## Technical Changes

### State A (Current)

- Turn input exists only as `SendSessionKeys` after `ClaimSessionTerminal` (steals the lease, browser
  drops to watch-only) — `project_home_terminal_tools.rs`, `exec_tool/project_home/terminal.rs`.
- `ResumeSession` RPC exists (`session.proto:38`, `svc_resume_session.rs:34`); no exec tool.
- The project notification log (`tddy-session-activity/src/project_notification_log/`) records
  `activity` / `attention_required` (the latter "blocked on the operator, or finished a turn and is
  waiting") with no orchestrator field; Wait/Read return every session's records.

### State B (Target)

- `SendSessionMessage` writes the text then the submit key to the session's main terminal input,
  bypassing the lease (the daemon is the writer; no broadcast change), or sends an ACP prompt.
- `ResumeSession` = `resume_session_at_session_coordinate` + optional first message delivered once the
  agent's terminal reports ready.
- Records carry `orchestrator_session_id`; `started_by_me` filters to the caller.

### Delta

- Specs: slot + `claude.yaml`. Tool engine: new module + schemas. Daemon-rpc: port + routing.
  Agent-launch: resume-then-message. Session-activity: record field + filter.

## Implementation Milestones

- [ ] Slot + spec tests
- [ ] SendSessionMessage terminal path
- [ ] ACP path
- [ ] ResumeSession
- [ ] Notification filter
- [ ] Forwarding

## Testing Plan

**Level**: exec-tool acceptance in `tddy-daemon-rpc` against the project-home world with a scripted
terminal agent (assert the bytes the terminal received and that no lease change was broadcast); peer
forwarding over the in-memory forwarder.

## Acceptance Tests

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_home_turn_control_acceptance.rs`
- `main_sends_a_running_claude_cli_session_a_turn_without_taking_its_terminal`
- `a_message_to_a_stopped_session_is_refused_naming_resume_session`
- `a_message_to_a_session_waiting_on_a_question_is_refused_naming_the_answer_tools`
- `a_message_to_an_agent_without_turn_input_is_refused_naming_the_agent`
- `resume_session_with_a_message_resumes_it_and_delivers_the_message`
- `resume_session_on_a_running_session_answers_running`
- `both_tools_reach_a_session_on_another_host`

### tddy-coding-agent-specs — `packages/tddy-coding-agent-specs/tests/spec_turn_input_acceptance.rs`
- `claude_declares_terminal_paste_turn_input`, `an_unknown_turn_input_kind_is_refused_at_load`

### tddy-session-activity — `packages/tddy-session-activity/tests/project_notification_started_by.rs`
- `a_turn_end_record_names_the_sessions_orchestrator`
- `started_by_me_returns_only_the_callers_sessions`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Agent-YAML-declared delivery** (developer, 2026-11-10) rather than terminal keys only.
- **No new `turn_finished` kind**: the log already records turn end as `attention_required`; adding the
  orchestrator to the record and a filter is the smaller change (PRD adjusted during pass 1).
- **No lease for a message**: a message is one turn, not control of the terminal; a browser holding
  control keeps it. Interleaving with a person typing at the same moment is accepted (recorded risk).

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

n5 `feature/orchestrators/session-lifecycle-tools`.

## TODO

- [x] Record initial discovery (`2026-11-10-session-turn-control-initial-discovery.md`)
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
