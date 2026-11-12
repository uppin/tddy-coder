# Changeset: effort on session start

**Date**: 2026-11-10
**Status**: 🚧 In Progress — planning (commit 1)
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
- [tddy-daemon-rpc](../../../packages/tddy-daemon-rpc/README.md) — start-options port
- [tddy-session-create-web](../../../packages/tddy-session-create-web/README.md) — Effort picker

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
| n1 `orchestrator-role` (#878) | role code | nothing — line predecessor only | touch role code |
| n2 `project-stack-store` (#879) | stack store | nothing — line predecessor only | touch stacks |

## Draft PR contract

- `effort = 48` in `session.proto` + regenerated Rust/TS.
- `EffortSlot`/`EffortCarry` types and the `effort:` key accepted by the spec loader (validation `TODO(start-effort)`).
- `CreateSessionEffortField` (renders nothing yet), test id `create-session-effort-select`.
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

- No effort anywhere (grep `effort|thinking|reasoning` in protos, specs, web: only "best-effort").
- Model: `StartSessionRequest.model = 8`, profile fill `harness_profile_request.rs:110-114`, check
  `start_request_checks.rs:130`, bound via `HarnessLaunch.model` (`harness_from_spec.rs:40-72`), argv
  clause `claude.yaml:17-28`.
- Last `StartSessionRequest` field `run_in_tmux = 47`; reserved 19–23.
- Web model picker `CreateSessionModelField.tsx:15` / `useCreateSessionModel.ts:40`.

### State B (Target)

- `effort` validated against the spec's levels; bound as `HarnessLaunch.effort` → the spec's carry
  (argv clause `{ if: effort, then: [<flag>, "{effort}"] }` or env); recorded; resumed.

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

### tddy-coding-agent-specs — `packages/tddy-coding-agent-specs/tests/spec_effort_acceptance.rs`
- `claude_declares_its_effort_levels`
- `a_spec_whose_default_is_not_one_of_its_levels_is_refused_at_load`
- `a_spec_without_an_effort_slot_loads_as_before`

### tddy-agent-launch — `packages/tddy-agent-launch/tests/spec_launch_effort_acceptance.rs`
- `a_claude_cli_start_at_high_effort_launches_with_that_level`
- `a_resumed_session_launches_at_its_recorded_effort`
- `an_unknown_level_is_refused_naming_the_agent_and_its_levels`
- `any_effort_for_an_agent_without_levels_is_refused`

### tddy-tool-engine — `packages/tddy-tool-engine/tests/start_session_effort.rs`
- `start_session_offers_effort_and_passes_it_through`
- `describe_session_start_options_lists_each_agents_levels`

### tddy-session-create-web — `packages/tddy-session-create-web/cypress/component/CreateSessionEffortAcceptance.cy.tsx`
- `the effort picker lists the selected agent's levels and sends the choice`
- `the effort picker is hidden for an agent without levels`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Declared by the spec, not hard-coded** — matches the developer's "agent YAML, not Claude-specific"
  rule for `/clear` (n6) and the coding-agent model.
- **String, not enum**: levels differ per agent.
- **Open for red**: the exact carry claude's CLI accepts for an effort level (an argv flag vs an env
  variable). The red step confirms it against the installed `claude --help` and records it here.
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
