# Changeset — agent `usageNotes` (`agent-usage-notes`)

**Date:** 2026-09-27
**Status:** 🚧 In Progress
**Type:** Feature
**Stack:** `subagent-control` node 3 of 5 (`(#subagent-control 3/5)`), wave 1. **PR:** [#555](https://github.com/uppin/tddy-coder/pull/555) (draft).
**Branch:** `feature/subagent-control/agent-usage-notes` · **Base:** `feature/subagent-control/grep-context`

**PRD:** [`2026-09-27-agent-usage-notes-prd.md`](2026-09-27-agent-usage-notes-prd.md)
**Initial discovery:** [`2026-09-27-agent-usage-notes-initial-discovery.md`](2026-09-27-agent-usage-notes-initial-discovery.md)

## Prerequisites

- **`tddy-web` has no `docs/code-issues/` directory** — the package is unanalyzed, which is not the
  same as clean. Named here; `/analyze-code-issues` is worth running when web changes land.
- No TODO-backlog or code-issue entries are touched by this change (the scan found none in the
  model-registry / models-proto / web-dialog path).

## Affected packages

- [`tddy-discovery`](../../../packages/tddy-discovery/README.md) — `SpecializedAgentDef` field + Debug
- [`tddy-service`](../../../packages/tddy-service/README.md) — `models.proto` (AssistantEntry, Create/UpdateAssistantRequest)
- [`tddy-model-registry`](../../../packages/tddy-model-registry/README.md) — store column + migration, service handlers, `assistant_to_agent_def` projection
- [`tddy-web`](../../../packages/tddy-web/README.md) — create/edit dialogs, panel row, RPC client fan-out

## Responsibility

- `usageNotes` end to end: def field (serde + Debug), proto fields, SQLite column + migration,
  registry create/update/list, def projection, web dialogs + panel + RPC client, YAML doc example.

## Boundaries

- Not injected into system prompts, not on `subagent_new_session`, not on `SessionAgentEntry` /
  `SubagentInfo` pickers.
- No agent-resolution, spawn or turn-behaviour changes.
- No YAML-writing tooling.

## Dependencies

None — every surface it touches exists on `master`. Its position after `grep-context` is line
order only.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-previews | `resultSummary` | nothing — order only | touch `result_summary.rs`, `MessageDescriptor` |
| `n2` grep-context | Grep context args | nothing — order only | touch `tool_grep`, `grep_limited` |

## Draft PR contract

First push (wave 2, commit 2): the field surfaces — `SpecializedAgentDef.usage_notes` (+ Debug),
the proto fields, `NewAssistant`/store column signature, `assistant_to_agent_def` mapping, and the
web dialog field testids — plus failing acceptance tests (store round-trip, projection, Cypress
dialog submit/edit). Dependents: none, but the surface must compile before wave 3.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — store/projection tests use the registry's own harness; Cypress
component tests mount the dialogs with an in-memory RPC backend. No other node's behaviour.
**Concurrent with:** `tool-previews` (#553), `grep-context` (#554).
**Blocks:** nothing.

Real dependency edges, as opposed to the branch line:

    tool-previews → yield-conditions → resume-replacement      (agent-usage-notes: no edges)

## State A

`SpecializedAgentDef` has no free-text operator field; the hand-written `Debug` redacts only
`api_key`. Web-created agents are registry assistants: `AssistantEntry` (models.proto:115-132),
`CreateAssistantRequest` (:213-223), `UpdateAssistantRequest` (:228-237) — no notes field. The
SQLite `assistant` table has no such column; `assistant_to_agent_def`
(tddy-model-registry/src/assistant_def.rs:24-60) maps fields explicitly. `CreateAssistantDialog`
submits `{name, label, systemPrompt, tools, replaces}`; `EditAssistantDialog` edits
`{label, systemPrompt, tools, replaces}`.

## State B

`usage_notes` is optional everywhere, following the `api_key` serde precedent and Update's
carried-whole rule. The dialogs expose a multi-line field; the panel row shows a truncated note
with full text on demand. Spawn, system prompts and turn behaviour are untouched.

## Delta

- `tddy-discovery`: `SpecializedAgentDef.usage_notes: Option<String>` + Debug line + YAML doc
  example (`docs/ft/coder/specialized-subagents.md`).
- `tddy-service`: three proto fields (`usage_notes`).
- `tddy-model-registry`: column + migration, `NewAssistant`, create/update/list, projection.
- `tddy-web`: dialogs, panel row, `useModelRegistryFanOut` create/update, generated TS types
  (`session_agents_pb.ts` regeneration not needed — `models.proto` generates separately), testids.

## Implementation milestones

- [ ] Def field + Debug + YAML doc
- [ ] Proto fields + generated TS
- [ ] Store column + migration + NewAssistant + create/update/list
- [ ] Projection mapping
- [ ] Web dialogs + panel + RPC client
- [ ] Cypress component tests

## Testing plan

- `packages/tddy-model-registry/tests/model_registry_store_unit.rs` (existing harness):
  create-with-notes → list-returns-notes → update-changes-notes round-trip.
- Projection unit test in `tddy-model-registry`.
- Cypress component (existing models specs' harness, `mountWithRpc` + `anInMemoryRpcBackend`):
  create dialog submits notes; edit dialog pre-fills and updates; panel row displays.

## Acceptance tests

1. `a_created_assistant_round_trips_its_usage_notes` —
   `packages/tddy-model-registry/tests/model_registry_store_unit.rs`
2. `an_updated_assistant_changes_its_usage_notes` — same file
3. `a_registry_assistant_projects_usage_notes_into_its_agent_def` — projection unit test,
   `packages/tddy-model-registry/src/assistant_def.rs` test module
4. `the_create_dialog_submits_usage_notes` —
   `packages/tddy-web/cypress/component/models/AssistantUsageNotesAcceptance.cy.tsx`
5. `the_edit_dialog_pre_fills_and_updates_usage_notes` — same file
6. `the_panel_row_shows_a_usage_note` — same file

## Technical debt & production readiness

(populated during development)

## Decisions & trade-offs

- Operator-facing only (approved scope decision): stored + displayed, never injected into machine
  context. Roster/picker surfacing deliberately out of scope.
- One field carried whole on Update, matching the `tools`/`replaces` precedent — no partial-update
  semantics.

## Refactoring needed

(none planned)

## Validation results

(to be filled by `/validate-changes`)

## TODO

- [x] Record initial discovery (`2026-09-27-agent-usage-notes-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — 3/3 Cypress failures (create submit, edit pre-fill, panel display), each the missing web wiring; the backend store/projection acceptance tests pass on the published surface
- [x] USER REVIEW — acceptance tests — waived by the developer ("finish the remaining ones without stopping", 2026-09-27)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-27-agent-usage-notes-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
