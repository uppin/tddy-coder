# Changeset: The orchestrator is told to run all coding through tracked tddy sessions

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 2 — prompt rewrite with role-specific wording and tests pinning each instruction
**Stack**: `#orchestra` 9/9 — branch `feature/orchestra/orchestrator-prompt`, base `feature/orchestra/orchestrator-lsp`

## Initial Discovery

[2026-11-21-orchestrator-prompt-initial-discovery.md](./2026-11-21-orchestrator-prompt-initial-discovery.md)

## Affected Packages

- **tddy-agent-launch**: `project_home_prompt.md` rewritten; role-specific opening for main vs orchestrator

## Related Feature Documentation

- PRD: [PRD-2026-11-21-orchestrator-prompt.md](../../ft/daemon/1-WIP/PRD-2026-11-21-orchestrator-prompt.md)

## Summary

The role prompt tells an orchestrator: do all coding work by opening tddy sessions; track each one (watches, transcript, activity, messages) until it ends; tell every child to report progress and status periodically over the back channel its guidance names; reach children by their listed route; check and refresh the base before starting work and react to trunk-commit activities; read beyond defaults and write only through `RequestPathAccess`; use the LSP tools and roots; release terminals it claimed. Main and orchestrator sessions get their own opening line.

## Responsibility

- `project_home_prompt.md` and the role-specific opening.

## Boundaries

- No new tools; it documents n1–n8's.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1–n8` | the tools and behaviours the prompt describes | names them and how to use them | add or change any tool |

## Draft PR contract

The first push of this PR, before its implementation: `project_home_prompt(role)` signature; failing tests below.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — its catalogue-coverage test needs every tool from n1–n8 to exist
**Concurrent with:** none
**Blocks:** none

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7 · n8 · n9):

    n1 path-reach → n6 access-elicitation, n8 orchestrator-lsp
    n6 access-elicitation → n8 orchestrator-lsp
    n2 child-back-channel → n7 orchestrator-delivery
    n1 … n8 → n9 orchestrator-prompt

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7 · 3 = n8 · 4 = n9 (run `1 1 1 1 1 2 2 3 4`).
Inside wave 1, blockers lead: n1 (3 transitive dependents: n6, n8, n9), n2 (2: n7, n9), then n3, n4, n5
(1 each: n9) in foundational order. Inside wave 2: n6 (2: n8, n9) before n7 (1: n9). n3–n5 and n7 sit
in the line only because `gh stack` needs a line; none depends on its neighbour.

## Scope

- [ ] Role prompt rewrite with main / orchestrator openings
- [ ] Feature doc `project-concept.md` § Role prompt updated through the wrap

## Technical Changes

### State A (Current)

`packages/tddy-agent-launch/src/project_home_prompt.md` (44 lines, main wording only); test `project_home_launch_tests.rs:143-163`.

### State B (Target)

A prompt that drives delegation, tracking, reporting and freshness.

## Acceptance Tests

`packages/tddy-agent-launch/src/project_home_launch/project_home_launch_tests.rs (unit)`:
- [ ] `an_orchestrators_prompt_opens_as_an_orchestrator_and_mains_as_main`
- [ ] `the_prompt_sends_coding_work_through_tracked_sessions`
- [ ] `the_prompt_tells_the_orchestrator_to_have_children_report_over_their_back_channel`
- [ ] `every_tool_the_orchestrator_catalogue_offers_is_named_in_the_prompt`

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- The prompt is its own last node rather than spread over n1–n8, so the nodes do not all conflict on one file.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-orchestrator-prompt-initial-discovery.md`)
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
