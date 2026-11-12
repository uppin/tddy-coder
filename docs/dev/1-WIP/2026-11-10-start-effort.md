# Changeset: effort on session start

**Date**: 2026-11-10
**Status**: 🚧 In Progress — draft-PR contract (commit 2): surface + failing tests, awaiting USER REVIEW
**Type**: Feature
**Complexity**: 4 — a proto field, a new agent-spec slot validated at load, more than one crate plus a web package
**Stack**: `#orchestrators` 3/11 · branch `feature/orchestrators/start-effort` · base `feature/orchestrators/project-stack-store` (#879)

## Initial Discovery

[2026-11-10-start-effort-initial-discovery.md](./2026-11-10-start-effort-initial-discovery.md)

## Prerequisites

### ⚠ DURING — The agent sees StartSession's host and recipe as free strings — [`2026-10-25-start-session-mcp-schema-has-no-host-or-recipe-choices.md`](../todo/2026-10-25-start-session-mcp-schema-has-no-host-or-recipe-choices.md)

Effort levels are per agent, so the static schema cannot enumerate them either. `effort` is a string
argument, and the levels are reported by `DescribeSessionStartOptions`, exactly as models are. Not claimed.

### ⚠ DURING — start-path complexity — [`complexity-svc-start-session-core-start-session-core.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md)

The effort check goes beside `refuse_a_cli_start_without_a_model` (`start_request_checks.rs`), not into `start_here`.

### ⚠ DURING — [`oversized-file-create-session-pane.md`](../../../packages/tddy-session-create-web/docs/code-issues/oversized-file-create-session-pane.md)

The picker is `CreateSessionEffortField.tsx` beside `CreateSessionModelField.tsx`; the pane gains one line.

### ⚠ DURING — open PR #876

Pass 2 waits for #876 (developer decision 2026-11-10).

## Affected Packages

- [tddy-service](../../../packages/tddy-service/) — `StartSessionRequest.effort = 48`; start-options effort levels
- [tddy-coding-agent-specs](../../../packages/tddy-coding-agent-specs/README.md) — `effort` slot; `claude.yaml`
- [tddy-agent-launch](../../../packages/tddy-agent-launch/) — check, bind into launch, re-apply on resume
- [tddy-changeset](../../../packages/tddy-changeset/README.md) — `SessionMetadata.effort`
- [tddy-tool-engine](../../../packages/tddy-tool-engine/README.md) — `StartSession` `effort`; `DescribeSessionStartOptions` levels
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — start-options port; `ListAgents` rows' `effort_levels` / `default_effort`
- [tddy-session-create-web](../../../packages/tddy-session-create-web/README.md) — Effort picker
- tddy-session-lifecycle — tests only: the start / resume launch tests live in its `claude_cli_from_spec_acceptance.rs` world (the only one that drives a real `StartSession` / `ResumeSession` to a recorded argv)
- tddy-client-web, tddy-rust-typescript-tests — regenerated `session_pb.ts` / `catalog_pb.ts` only

## Related Feature Documentation

- PRD: [PRD-2026-11-10-start-effort.md](../../ft/daemon/1-WIP/PRD-2026-11-10-start-effort.md)
- [Coding agent specs](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md), [Claude CLI session](../../ft/daemon/claude-cli-session.md)

## Summary

`StartSessionRequest.effort` starts a session at a reasoning effort level. The agent's YAML spec
declares the levels and how its launch carries one; claude's spec maps it. The New-session form and the
exec `StartSession` tool both offer it, and the level survives resume.

## Background

`/yolo-pr-stack` sizes every subagent by model **and** effort; an orchestrator that drives sessions the
same way (developer, 2026-11-10) needs the same two knobs. The "+" composer (n8) needs the picker.
Discovery found no effort or thinking-level parameter anywhere in the protos, specs or web.

## Responsibility

- Proto field `StartSessionRequest.effort` (48) and the start-options report of levels per agent.
- `CodingAgentSection.effort: Option<EffortSlot { levels: Vec<String>, default: Option<String>, carry:
  EffortCarry::{Flag(String) | Env(String)} }>`, validated at load.
- `SessionMetadata.effort`, re-applied on resume.
- `refuse_an_effort_the_agent_cannot_take`.
- `CreateSessionEffortField` + `form.effort` in the request builder; the exec `StartSession` argument.

## Boundaries

- Does **not** add effort to harness profiles or project defaults (n11 stores composer defaults).
- Does **not** build the composer (n8).
- Does **not** change model selection.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `orchestrator-role` (#878) | role code | nothing — line predecessor only. Verified at red: no test here reaches a `TODO(orchestrator-role)` stub; `StartSessionArgs::into_request` carries both nodes' TODOs side by side (`session_role`, `effort`) in one `..Default::default()` | touch role code |
| n2 `project-stack-store` (#879) | stack store | nothing — line predecessor only. No test here reaches a `TODO(project-stack-store)` stub | touch stacks |

**Shared files with wave-1 siblings** (conflict risk, not dependencies): `StartSessionArgs` /
`into_request` (n1 `session_role`), `session.proto`'s `StartSessionRequest` tail (field 48 here). The
contract deliberately does **not** add `SessionMetadata.effort`: ~80 struct literals name every field,
and n6 (`/clear`) grows the same struct — green adds the field once, after rebasing.

## Draft PR contract

- `effort = 48` in `session.proto` + regenerated Rust/TS.
- `AgentInfo.effort_levels = 12` / `default_effort = 13` in `catalog.proto` — how the web learns an
  agent's levels (the form fills every agent picker from `ListAgents`); filled empty,
  `TODO(start-effort)` in `tddy-daemon-rpc/src/catalog/agent_rows.rs`.
- `EffortSlot { levels, default: Option<String>, carry: EffortCarry::{Flag | Env} }`
  (`tddy-coding-agent-specs/src/spec/effort_slot.rs`, re-exported), YAML
  `effort: { levels, default?, flag | env }` — naming both carries or neither is refused as the slot
  is read; `CodingAgentSection.effort: Option<EffortSlot>`; `CodingAgentSpec::effort()`;
  `validate_effort_slot` called from `parse_section`, stubbed `Ok` (`TODO(start-effort)`).
- `pub fn refuse_an_effort_the_agent_cannot_take(req, agents: Option<&HostCodingAgents>) -> Result<(), Status>`
  in `start_request_checks.rs`, stubbed `Ok`, not yet called (`TODO(start-effort)`).
- `StartSessionArgs.effort: String` (accepted, not yet offered in the schema nor carried —
  `TODO(start-effort)` in `into_request`); `StartOptions.effort_levels: Vec<AgentEffortLevels { session_type, default, levels }>`,
  filled empty in `project_home_start_options.rs` (`TODO(start-effort)`).
- `CreateSessionEffortField({ levels, effort, setEffort })` (renders nothing yet), test id
  `create-session-effort-select` (`TEST_IDS.createSessionEffortSelect`);
  `CreateSessionFormValues.effort?: string` (not yet sent).
- **Not in the contract, green adds**: `SessionMetadata.effort`, `HarnessLaunch.effort`, `claude.yaml`'s
  `effort:` slot, the launch carry, the call of the start check, the pane wiring.
- Failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes
**Concurrent with:** n1, n2, n4, n5, n6
**Blocks:** n8 `orchestrator-composer` (its effort control)

    n1 → n7, n8    n2 → n7, n9    n3 → n8    n4 → n10    n7 → n10    n8 → n11

## Scope

- [ ] Proto `effort = 48`; start-options levels
- [ ] Spec slot + load validation; `claude.yaml` declares its levels and carry
- [ ] Start check; bind into launch; record; re-apply on resume
- [ ] Exec `StartSession` `effort`; `DescribeSessionStartOptions` levels
- [ ] Web picker, hidden for agents without levels

## Technical Changes

### State A (Current)

Re-verified at red (2026-11-12, head `917bb94bb`); moved lines noted.

- No effort anywhere (grep `effort|thinking|reasoning` in protos, specs, web: only "best-effort").
- Model: `StartSessionRequest.model = 8` (`session.proto:513`), profile fill
  `harness_profile_request.rs:113-117` (was :110-114), check `refuse_a_cli_start_without_a_model`
  `start_request_checks.rs:128` (was :130), bound via `HarnessLaunch` (`harness_from_spec.rs:40`) and
  `harness_template_params` (`:72`), argv clause `claude.yaml:17-28`.
- Last `StartSessionRequest` field `run_in_tmux = 47` (`session.proto:704`); reserved 19–23 (`:710`).
- `CodingAgentSection` is `spec.rs:181-261` (was :248-352), `deny_unknown_fields`.
- Web model picker `CreateSessionModelField.tsx:15` / `useCreateSessionModel.ts:43` (was :40). Agent
  metadata reaches the form as `AgentInfo` rows of `ListAgents` (`catalog.proto:31-62`), built in
  `tddy-daemon-rpc/src/catalog/agent_rows.rs`.
- `StartSession`'s parity test (`tddy-tool-engine/tests/start_session_args_parity.rs`) reads the
  proto descriptor: adding field 48 turns it red until the schema offers `effort` (expected, this
  node's).
- **Claude's carry, confirmed** against Claude Code 2.1.311: `--effort <level>` — "Effort level for
  the current session (low, medium, high, xhigh, max)". So claude.yaml declares
  `effort: { levels: [low, medium, high, xhigh, max], flag: --effort }`, no default.

### State B (Target)

- `effort` validated against the spec's levels; bound as `HarnessLaunch.effort` → the spec's carry
  (`Flag(f)` → `f <level>` on the argv, `Env(v)` → `v=<level>` in the launch env); recorded; resumed.
  The tests pin only what reaches the process (`--effort high`), so green picks where the carry is
  rendered (a slot-rendered pair beside the other slot flags, or an argv clause binding `{effort}`).

### Delta

- Proto + gen; spec types/loader/`claude.yaml`; launch binding; metadata field; tool-engine args and
  options; daemon start-options port; web field and builder.

## Implementation Milestones

- [ ] Proto + spec slot (spec load tests)
- [ ] Launch + resume (spec launch acceptance)
- [ ] Exec tool + options
- [ ] Web picker

## Testing Plan

**Level**: spec-load unit tests; `spec_launch_acceptance`-style launch tests asserting argv/env;
exec-tool acceptance; Cypress component for the picker with `mountWithRpc` + `anInMemoryRpcBackend`.

## Acceptance Tests

File names drop `_acceptance` / `Acceptance` (fluent-tests: it names no level). Two tests moved to the
world that actually exercises them (noted per section).

### tddy-coding-agent-specs — `packages/tddy-coding-agent-specs/tests/spec_effort.rs`
- `claude_declares_its_effort_levels`
- `a_spec_whose_default_is_not_one_of_its_levels_is_refused_at_load`
- `a_spec_without_an_effort_slot_loads_as_before` (guard — passes today)

### tddy-agent-launch — `packages/tddy-agent-launch/tests/start_effort_checks.rs`
- `an_unknown_level_is_refused_naming_the_agent_and_its_levels`
- `any_effort_for_an_agent_without_levels_is_refused`

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/claude_cli_from_spec_acceptance.rs` (existing world)
- `a_claude_cli_start_at_high_effort_launches_with_that_level`
- `a_resumed_session_launches_at_its_recorded_effort`

(Planned in tddy-agent-launch; moved because only this world drives a real `StartSession` /
`ResumeSession` through `DaemonSessionHost` to a stub `claude` that records its argv.)

### tddy-tool-engine — `packages/tddy-tool-engine/tests/start_session_effort.rs`
- `start_session_offers_effort_and_passes_it_through`

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/start_options_acceptance.rs` (existing world)
- `describe_session_start_options_lists_each_agents_levels`

(Planned in tddy-tool-engine; moved because the tool engine only forwards the port's answer — the
answer is assembled from the spec registry in `project_home_start_options.rs`.)

### tddy-session-create-web — `packages/tddy-session-create-web/cypress/component/CreateSessionEffort.cy.tsx`
- `the effort picker lists the selected agent's levels and sends the choice`
- `the effort picker is hidden for an agent without levels`

## Unit / integration tests (TDD Red)

- `tddy-coding-agent-specs/src/spec/effort_slot.rs` `tests`: no levels, a repeated level, a blank
  level, a default outside the levels — each refused with its message; an `env` carry and a
  both-carries refusal (guards).
- `tddy-agent-launch/src/start_request_checks_unit_tests.rs` `effort`: a level differing only in case
  is refused; an effort on `workspace` is refused naming it; empty / blank / declared level admitted
  (guards).
- `tddy-changeset/src/session_metadata.rs` `tests`: a record with `effort: high` survives a read and a
  write; a record without one writes no key (guard).
- `tddy-daemon-rpc/tests/agent_catalog_acceptance.rs`: `the_agent_catalog_reports_each_agents_effort_levels`;
  the existing row builder now reads `effort_levels` / `default_effort` from the spec.
- `tddy-session-create-web/cypress/component/CreateSessionRequestBuilder.cy.ts`: the chosen effort
  is sent for a CLI agent; a tool session sends `effort: ""`.
- Existing tests this node turns red, on purpose: `start_session_args_parity.rs`
  (`every_start_session_request_field_is_offered_context_filled_or_withheld` — `effort` unaccounted
  until offered; green also refreshes `cypress/support/fixtures/start-session-tool-fields.json`).

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Declared by the spec, not hard-coded** — matches the developer's "agent YAML, not Claude-specific"
  rule for `/clear` (n6) and the coding-agent model.
- **String, not enum**: levels differ per agent.
- **Settled at red**: claude's CLI carries an effort as the flag `--effort <level>`, levels `low,
  medium, high, xhigh, max` (Claude Code 2.1.311 `--help`).
- **Levels reach the web through `ListAgents`** (`AgentInfo.effort_levels`), the same rows every other
  per-agent picker reads; `DescribeSessionStartOptions` reports them for the exec tool.
- `SessionMetadata` refuses unknown fields: an older daemon cannot read a record carrying `effort`.

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

n4 `feature/orchestrators/session-turn-control`.

## TODO

- [x] Record initial discovery (`2026-11-10-start-effort-initial-discovery.md`)
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
