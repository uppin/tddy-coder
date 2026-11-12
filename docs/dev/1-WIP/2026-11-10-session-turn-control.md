# Changeset: orchestrators drive a session's turns

**Date**: 2026-11-10
**Status**: 🚧 In Progress — draft-PR contract (commit 2): surface + failing tests
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
- [tddy-tools](../../../packages/tddy-tools/README.md) — `exec_tool_admission.rs` closed-world list gains the two tools (developer, 2026-11-12)

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

Verified at pass 2 (2026-11-12): **no test of this node reaches a `TODO(orchestrator-role)`,
`TODO(project-stack-store)` or `TODO(start-effort)` stub.** The daemon-rpc suite's caller is main
(`session_role: project-home`, already privileged on master), and its resume world starts the
claude-cli child through `StartSession` **without** `session_role`/`effort`. It reuses n1's test
helper `support::a_host_running_claude_as` — implemented test infrastructure, not a stub.

**Closed-world lists green must extend** when it registers the two tools in the project-home
catalogue (deliberately not done by the contract, so master's lists stay green):
`packages/tddy-daemon-rpc/tests/project_home_session_tools_acceptance.rs`
(`a_project_home_session_is_offered_its_read_only_tools_and_the_project_home_tools`) and
`packages/tddy-tools/src/exec_tool_admission.rs`
(`a_project_home_session_is_advertised_its_read_only_tools_and_the_project_home_tools`) — the
latter is in **`tddy-tools`, a package not in Affected Packages**.

## Draft PR contract

Published in commit 2 (all bodies `TODO(session-turn-control)`):

- **coding-agent-specs** — `TurnInputSlot { TerminalPaste { submit_key } | AcpPrompt }`
  (`src/spec/turn_input_slot.rs`, YAML `turn_input: { kind: terminal_paste, submit_key: Enter }` /
  `{ kind: acp_prompt }`, re-exported from the crate root); `CodingAgentSection.turn_input`;
  `CodingAgentSpec::turn_input()`; load-time `validate_turn_input_slot` (stub: accepts all).
  `claude.yaml` is **not** yet changed — that is green's.
- **tool-engine** — `project_home_turn_tools.rs`: `SEND_SESSION_MESSAGE`, `RESUME_SESSION`,
  `is_project_home_turn_tool`, `project_home_turn_tool_defs()` (schemas),
  `TurnDelivery { TerminalPaste, AcpPrompt }` (`delivered_via`), `ResumeOutcome { Resumed, Running }`
  (`outcome`), port `ProjectHomeTurnPort: ProjectHomePort` with `send_session_message(target, text)`
  and `resume_session(target, message)` (defaults refuse), `execute_project_home_turn_tool` (stub
  refuses). **Not** registered in `is_project_home_tool` / the catalogue / `is_project_home_target_tool`
  — green's (see Dependencies for the closed-world lists).
- **tool-engine inbox** — `started_by_me` (boolean) in both notification tools' schemas;
  `ProjectHomeInboxPort::{read,wait}_notifications` take `started_by_me: bool` (the executor passes
  `false` until green reads the argument).
- **session-activity** — `ProjectNotificationRecord.orchestrator_session_id: Option<String>` (DTO
  field, `serde(default)`, omitted when `None`, so old lines replay); `ProjectNotificationScope.started_by:
  Option<String>` (filter stubbed; `deliver` records `None`).
- **daemon-rpc** — `impl ProjectHomeTurnPort for DaemonProjectHome {}` (`exec_tool/project_home/turn.rs`,
  trait defaults); `scope_of(home, started_by_me)` (stub passes `started_by: None`).
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

Re-verified against code at pass 2 (base `d1bd6070a`, 2026-11-12):

- Turn input exists only as `SendSessionKeys` after `ClaimSessionTerminal` (steals the lease, browser
  drops to watch-only) — `tddy-tool-engine/src/project_home_terminal_tools.rs`,
  `tddy-daemon-rpc/src/exec_tool/project_home/terminal.rs` (lease verify + `send_input` at :65-87).
  Text is bracketed-pasted when the target's `TextGrid.bracketed_paste` is set (`terminal_keys::encode_keys`).
- `ResumeSession` RPC exists (`tddy-service/proto/session.proto:38`,
  `tddy-agent-launch/src/svc_resume_session.rs:34` `resume_session_at_session_coordinate`); the
  session type → agent is `recorded_session_launch` (`harness_from_spec.rs:222`) and
  `CodingAgentRegistry::for_session_type` (`registry.rs:127`). No exec tool.
- The project notification log (`tddy-session-activity/src/project_notification_log.rs`, DTOs in
  `project_notification_log/storage.rs`, `project_file()` :34) records `activity` /
  `attention_required` (`tddy-daemon-kernel/src/session_notification.rs:8-13`, "finished a turn and
  is waiting") with no orchestrator field; `deliver` reads only the session's metadata. Scope
  (`ProjectNotificationScope`) is project + OS user − caller. Wait/Read return every session's
  records. The orchestrator a child was started by is `changeset.orchestrator_session_id`
  (read the same way by `exec_tool/project_home/caller.rs:84-86`).
- Target tools forwarded across hosts are those `is_project_home_target_tool` lists
  (`tddy-tool-engine/src/project_home_target_host.rs:33`), served by `served_on_target_host`.
- Moved since pass 1: coding-agent spec slots no longer live in `spec.rs` at the cited
  `:795`/`:1104` — `spec.rs` is now 621 lines with `CodingAgentSection` at :188 and the slots split
  into `src/spec/*.rs` (`effort_slot.rs` is n3's model for this node's `turn_input_slot.rs`).

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

All written at pass 2, failing for this node's missing implementation (see Exploration 2).

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_home_turn_control_acceptance.rs`
- `main_sends_a_running_claude_cli_session_a_turn_without_taking_its_terminal`
- `a_message_to_a_stopped_session_is_refused_naming_resume_session`
- `a_message_to_a_session_waiting_on_a_question_is_refused_naming_the_answer_tools`
- `a_message_to_an_agent_without_turn_input_is_refused_naming_the_agent` (codex-cli; asserts `codex` and `turn_input`)
- `resume_session_with_a_message_resumes_it_and_delivers_the_message` (stub `claude` really spawned; second launch carries `--resume <id>`)
- `resume_session_on_a_running_session_answers_running`
- `both_tools_reach_a_session_on_another_host`

### tddy-coding-agent-specs — `packages/tddy-coding-agent-specs/tests/spec_turn_input_acceptance.rs`
- `claude_declares_terminal_paste_turn_input`
- `an_unknown_turn_input_kind_is_refused_at_load` — ⚠ **passes on the contract**: the serde-tagged
  `TurnInputSlot` already refuses an unknown kind, naming it. Kept as the changeset's regression.
- `a_terminal_paste_whose_submit_key_is_not_a_key_is_refused_at_load` (added)

### tddy-session-activity — `packages/tddy-session-activity/tests/project_notification_started_by.rs`
- `a_turn_end_record_names_the_sessions_orchestrator`
- `started_by_me_returns_only_the_callers_sessions` (a wait)
- `a_read_started_by_me_skips_the_records_of_sessions_it_did_not_start`,
  `a_record_of_a_session_nobody_started_names_no_orchestrator_and_the_json_leaves_it_out`,
  `the_orchestrator_a_record_names_survives_a_restart` (6f)

### Unit (6f) — tddy-tool-engine
- `tests/project_home_turn_tools.rs` (fake port): argument → port, delivered_via/outcome answers,
  empty text and other-project refusals before the port, port refusal under the tool name, both tools
  are target tools, both offered to a project-home session.
- `tests/project_home_inbox_tools.rs` (+3): `started_by_me` in both schemas (⚠ passes on the
  contract), reaches the port for wait/read and defaults to false, non-boolean refused.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **6e review (developer, 2026-11-12):**
  - `tddy-tools` joins this node: registering the tools updates its closed-world admission list.
  - The terminal key-name set **moves into `tddy-coding-agent-specs`**; `tddy-tool-engine::terminal_keys`
    reads it from there, so the `turn_input.submit_key` is validated at load against one list.
  - ACP turn input (`acp_prompt`) is covered by the tool-engine unit test with a fake port only, until an
    ACP agent's YAML declares it.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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
