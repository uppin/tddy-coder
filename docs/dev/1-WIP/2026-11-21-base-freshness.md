# Changeset: An orchestrator checks and refreshes its base, and a start reports the commit it was cut from

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — two tools over existing git helpers, one new field through the start ledger
**PR**: [#982](https://github.com/uppin/tddy-coder/pull/982)
**Stack**: `#orchestra` 4/9 — branch `feature/orchestra/base-freshness`, base `feature/orchestra/session-files`

## Initial Discovery

[2026-11-21-base-freshness-initial-discovery.md](./2026-11-21-base-freshness-initial-discovery.md)

## Affected Packages

- **tddy-worktree-service**: `base_freshness` (fetch + ahead/behind of the home worktree vs the project's trunk tip) and `refresh_home` (`fast_forward_home_worktree`)
- **tddy-tool-engine**: `CheckBaseFreshness`, `RefreshHome` tools; start ledger records `base_commit` and `base_behind`
- **tddy-daemon-rpc**: role guard + serving
- **tddy-service**: `StartWorktreeReady.base_commit` / `base_behind` (fields 3, 4); TS regenerated in `tddy-client-web`, `tddy-rust-typescript-tests`
- **tddy-session-files**: `worktree_ready` carries the base

## Related Feature Documentation

- PRD: [PRD-2026-11-21-base-freshness.md](../../ft/daemon/1-WIP/PRD-2026-11-21-base-freshness.md)

## Summary

The orchestrator runs the freshness checks itself: `CheckBaseFreshness` fetches the project's trunk and reports its home worktree's commit, the remote tip, and ahead/behind; `RefreshHome` fast-forwards the home to the tip. Every start the orchestrator makes records and reports the base commit it was cut from and how far that commit was behind the remote tip at cut time (`GetSessionStart`).

## Responsibility

- `CheckBaseFreshness`, `RefreshHome` and their worktree-service backing.
- `base_commit` / `base_behind` in the start ledger and `GetSessionStart`.

## Boundaries

- No polling of the remote (n5).
- No refusal of stale starts — reporting only (developer).

## Dependencies

None inside `#orchestra`. It builds on its base, `#target-parity` 7/7 (#975), and on what is on master.

## Draft PR contract

The first push of this PR, before its implementation: `BaseFreshness`, `worktree_service::{base_freshness, refresh_home}`, the two tool definitions, `StartLedgerRecord.{base_commit, base_behind}`, proto `StartWorktreeReady.{base_commit, base_behind}`; failing tests below.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes
**Concurrent with:** n1, n2, n3, n5
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

### ⚠ DURING — [complexity-svc-start-session-core-start-session-core.md](../../../packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md)

The base commit is read where the worktree is cut and returned up, not computed inside `start_session_core`.

### ⚠ DURING — [oversized-file-service.md](../../../packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md)

New functions go in their own module.

## Scope

- [ ] `CheckBaseFreshness` and `RefreshHome` for orchestrator roles
- [ ] Base commit + behind count on every start record
- [ ] Feature doc `docs/ft/daemon/project-concept.md` § Orchestrator sessions updated through the wrap

## Technical Changes

### State A (Current)

`home_worktree.rs:41-77` creates and fast-forwards at start; `base_sync.rs` never fetches; the start ledger has no base fields.

### State B (Target)

The orchestrator checks and refreshes on demand; starts record their base.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/orchestrator_base_freshness_acceptance.rs` (real `ExecuteTool`):
- [ ] `check_base_freshness_reports_the_orchestrators_home_behind_after_the_trunk_moves`
- [ ] `refresh_home_brings_the_orchestrators_home_to_the_tip`
- [x] `a_worker_session_is_refused_the_freshness_tools` — regression guard

`packages/tddy-worktree-service/tests/base_freshness_acceptance.rs` (real git: bare origin, pusher clone, detached home):
- [ ] `a_current_home_is_zero_behind`
- [ ] `freshness_reports_the_home_behind_after_the_remote_trunk_moves`
- [ ] `refresh_fast_forwards_the_home_to_the_remote_tip`
- [ ] `refresh_refuses_a_diverged_home_and_leaves_it_untouched`

`packages/tddy-tool-engine/tests/start_ledger_base.rs`:
- [ ] `a_started_session_records_the_base_commit_it_was_cut_from`

Unit — `packages/tddy-tool-engine/src/base_freshness_tools.rs`:
- [ ] `both_tools_are_defined_with_their_names_and_take_no_arguments`

The surface they specify (bodies `TODO(base-freshness)`): `tddy_worktree_service::base_freshness::{BaseFreshness, RefreshRefusal, base_freshness, refresh_home}`, `tddy_tool_engine::base_freshness_tools::{CHECK_BASE_FRESHNESS, REFRESH_HOME, is_base_freshness_tool, base_freshness_tool_defs, BaseFreshnessPort, execute_base_freshness_tool}`, `StartLedgerRecord.{base_commit, base_behind}`, and proto `StartWorktreeReady.base_commit` (3) / `base_behind` (4), with the committed TypeScript in `tddy-client-web` and `tddy-rust-typescript-tests` regenerated. `attachment_progress.rs`'s `worktree_ready` sends them empty until the implementation fills them (`TODO(base-freshness)`).

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- The orchestrator runs the checks itself, by tool; the daemon does not refresh before every start (developer, 2026-11-21).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-base-freshness-initial-discovery.md`)
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
