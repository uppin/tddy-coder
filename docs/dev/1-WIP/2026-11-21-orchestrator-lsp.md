# Changeset: An orchestrator queries the LSP of any tddy worktree and adds or removes roots

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — LSP tools offered to a new role with an explicit root, an index-daemon unload RPC, root requests through n6's elicitation
**PR**: [#986](https://github.com/uppin/tddy-coder/pull/986)
**Stack**: `#orchestra` 8/9 — branch `feature/orchestra/orchestrator-lsp`, base `feature/orchestra/orchestrator-delivery`

## Initial Discovery

[2026-11-21-orchestrator-lsp-initial-discovery.md](./2026-11-21-orchestrator-lsp-initial-discovery.md)

## Affected Packages

- **tddy-lsp-executor**: `Lsp*` tools take an optional `root`; `IndexLspExecutor` binds to it
- **tddy-session-activity**: `SessionLspRoots` — the roots a session added, persisted in its directory
- **tddy-tool-engine**: `Lsp*` in the orchestrator catalogue; `AddLspRoot`, `RemoveLspRoot`, `ListLspRoots`
- **tddy-daemon-rpc**: role guard admits the tools; root added outside reach goes through n6's request flow
- **tddy-agent-launch**: orchestrator wiring offers the LSP tools

## Related Feature Documentation

- PRD: [PRD-2026-11-21-orchestrator-lsp.md](../../ft/daemon/1-WIP/PRD-2026-11-21-orchestrator-lsp.md)

## Summary

Orchestrator-role sessions get `LspDiagnostics`, `LspDefinition`, `LspReferences`, `LspHover` and `LspSymbols` with a `root` argument naming any worktree in their reach (n1) — another session's worktree, another tddy project. `AddLspRoot` warms the index for a root; a root outside reach goes through the same operator request as a path (n6). `RemoveLspRoot` unloads a root this session added, without asking. `ListLspRoots` shows what the session added.

## Responsibility

- `root` on the LSP tools, the three root tools, `UnloadRoot`.
- Offering LSP tools to orchestrator roles.

## Boundaries

- Does not change the request flow (n6) or reach (n1).
- No restructure tools for orchestrators.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1 path-reach` | `OrchestratorReach::contains` | decides whether a root needs a request | change reach |
| `n6 access-elicitation` | the pending request + answer flow | a root outside reach is requested through it | add a second elicitation kind's UI or change the answer RPC |

## Draft PR contract

The first push of this PR, before its implementation: `lsp_root_for` (the `root` an LSP call runs on), `AddLspRoot`/`RemoveLspRoot`/`ListLspRoots` with their port, `SessionLspRoots`; failing tests below.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — the out-of-reach root test exercises n6's request flow and n1's reach
**Concurrent with:** none
**Blocks:** n9

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7 · n8 · n9):

    n1 path-reach → n6 access-elicitation, n8 orchestrator-lsp
    n6 access-elicitation → n8 orchestrator-lsp
    n2 child-back-channel → n7 orchestrator-delivery
    n1 … n8 → n9 orchestrator-prompt

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7 · 3 = n8 · 4 = n9 (run `1 1 1 1 1 2 2 3 4`).
Inside wave 1, blockers lead: n1 (3 transitive dependents: n6, n8, n9), n2 (2: n7, n9), then n3, n4, n5
(1 each: n9) in foundational order. Inside wave 2: n6 (2: n8, n9) before n7 (1: n9). n3–n5 and n7 sit
in the line only because `gh stack` needs a line; none depends on its neighbour.

## Prerequisites

### Related — [2026-10-04-read-lints-is-refused-through-the-warm-index.md](../todo/2026-10-04-read-lints-is-refused-through-the-warm-index.md)

Orchestrators already have `ReadLints`; through the warm index it is refused. Not fixed here; `LspDiagnostics` per file is the route.

### Related — [2026-10-17-index-daemon-toolchain-is-one-environment-for-every-root.md](../todo/2026-10-17-index-daemon-toolchain-is-one-environment-for-every-root.md)

Other projects' roots load with the daemon's one toolchain; a root whose toolchain differs may index poorly. Stays deferred.

### ⚠ DURING — [poisoned-warm-latch-on-interrupted-index.md](../../../packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md)

`UnloadRoot` during a warm must clear the latch; the test covers it.

## Scope

- [ ] LSP tools for orchestrator roles with a `root`
- [ ] `AddLspRoot` / `RemoveLspRoot` / `ListLspRoots`, with operator request outside reach
- [ ] Removal leaves unloading to the index daemon's idle reaping (no `UnloadRoot`)
- [ ] Feature docs `docs/ft/coder/reusable-lsp.md`, `warm-code-intelligence-daemon.md` updated through the wrap

## Technical Changes

### State A (Current)

`lsp_tools.rs:13-72` gated by `TDDY_LSP_TOOLS`; `index_backed.rs:93-108` binds to the session worktree; `code_index.proto` has no unload.

### State B (Target)

Orchestrators query any root in reach and manage the roots they add.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/orchestrator_lsp_acceptance.rs` (real `ExecuteTool` and `ActivityService`):
- [ ] `an_orchestrators_lsp_tool_naming_a_sibling_root_is_not_refused_as_read_only`
- [ ] `adding_a_root_inside_reach_lists_it_without_a_request` — needs n1's reach
- [ ] `adding_a_root_outside_reach_asks_the_operator_and_allow_adds_it` — needs n6's request flow (kept real: asking the operator is the point)
- [ ] `removing_a_root_this_session_added_drops_it_without_asking`
- [ ] `removing_a_root_this_session_did_not_add_is_refused`

Unit — `packages/tddy-session-activity/src/lsp_roots.rs`:
- [ ] `added_roots_are_listed_oldest_first_and_survive_reopening`
- [ ] `adding_a_root_twice_keeps_one`
- [ ] `removing_an_added_root_drops_it`
- [ ] `removing_a_root_never_added_changes_nothing`

Unit — `packages/tddy-tool-engine/src/lsp_root_tools.rs`:
- [ ] `a_call_naming_no_root_runs_on_the_home_worktree`
- [ ] `a_root_in_reach_is_used_as_named`
- [ ] `an_added_root_outside_reach_is_used_as_named`
- [ ] `a_root_outside_reach_that_was_never_added_is_refused_naming_add_lsp_root`
- [ ] `the_three_root_tools_are_defined_with_their_names`

The surface they specify (bodies `TODO(orchestrator-lsp)`): `tddy_session_activity::lsp_roots::{SessionLspRoots, AddedLspRoot, LSP_ROOTS_FILE}`, `tddy_tool_engine::lsp_root_tools::{ADD_LSP_ROOT, REMOVE_LSP_ROOT, LIST_LSP_ROOTS, is_lsp_root_tool, lsp_root_tool_defs, lsp_root_for, AddLspRootArgs, AddedRoot, LspRootPort, execute_lsp_root_tool}`.

**Contract-time decision:** no `UnloadRoot` RPC on the index daemon. Removing a root drops it from this session's roots; the index daemon's idle reaping unloads the server once no session uses it. An explicit unload from one orchestrator could stop a root another session is still querying, and would add a method to five `CodeIndexService` implementations. `tddy-index-daemon` is therefore not touched.

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- Removing a root the session added needs no request (developer requirement 6).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-orchestrator-lsp-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (delegated: the user said "Finish the remaining ones without stopping")
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
