# PRD — Update-Docs Goal

**Date**: 2026-03-10
**Status**: ✅ Complete
**Type**: Feature Addition
**Product Area**: Coder

## Summary

Add a new `update-docs` goal to the TDD workflow that runs after `refactor` as the final phase before workflow completion. The agent reads planning artifacts (PRD.md, progress.md, changeset.yaml, acceptance-tests.md, evaluation-report.md) and directly updates the target repo's documentation (feature docs, dev docs, changelogs, READMEs) per that repo's guidelines.

## Background

The TDD workflow currently ends at `refactor`. After refactoring, documentation in the target repo is stale — it doesn't reflect the changes that were just implemented. Today this is a manual step performed by the developer or by invoking `/wrap-context-docs` in Cursor. By making `update-docs` a first-class goal in the workflow, the documentation update becomes automated and consistent.

**Key insight**: tddy-coder is a tool that runs inside *other* repos. The documentation being updated is in the target repo, not in tddy-coder itself. The agent must read the target repo's doc structure and update it accordingly.

## Affected Features

- [1-OVERVIEW.md](../1-OVERVIEW.md) — Core Capabilities table, workflow chain description
- [implementation-step.md](../implementation-step.md) — Goal sequence, state machine
- [planning-step.md](../planning-step.md) — Workflow overview

## Proposed Changes

### New Goal: `update-docs`

| Aspect | Value |
|--------|-------|
| CLI flag | `--goal update-docs` |
| Graph position | `refactor → update-docs → end` |
| Changeset state | `RefactorComplete → UpdatingDocs → DocsUpdated` |
| CursorBackend | Supported (only needs Read/Write/Edit) |
| Permission mode | `acceptEdits` |

### What Stays the Same

- All existing goals (plan through refactor) unchanged
- Changeset.yaml structure (new states appended)
- Schema validation pattern (new schema file)
- StubBackend pattern (new stub response)

### Input Artifacts (Agent Reads)

The agent reads these from `plan_dir`:
- `PRD.md` — Product requirements (what was built)
- `progress.md` — Implementation status
- `changeset.yaml` — Workflow state, sessions, metadata
- `acceptance-tests.md` — Test definitions
- `evaluation-report.md` — Change analysis
- `refactoring-plan.md` — What was refactored

The agent also reads the target repo's existing documentation structure to understand where and how to update.

### Output (Agent Produces)

- **Direct file modifications** to target repo docs (feature docs, dev docs, changelogs, READMEs)
- **Structured response** with summary and count of docs updated

### Allowlist

Read, Write, Edit, Glob, Grep, SemanticSearch — file operations only, no shell commands.

## Acceptance Criteria

1. [x] `Goal::UpdateDocs` variant exists and is wired through all backends (Claude, Cursor, Stub)
2. [x] `--goal update-docs` is accepted by both `tddy-coder` and `tddy-demo` CLI
3. [x] `update-docs.schema.json` exists and validates the structured response
4. [x] `parse_update_docs_response()` extracts `UpdateDocsOutput` from agent output
5. [x] `update_docs` module provides `system_prompt()` and `build_prompt()`
6. [x] Full workflow graph chains: `refactor → update-docs → end`
7. [x] `next_goal_for_state("RefactorComplete")` returns `Some("update-docs")`
8. [x] `next_goal_for_state("DocsUpdated")` returns `None`
9. [x] `TddWorkflowHooks` implements `before_update_docs` and `after_update_docs`
10. [x] `after_update_docs` updates changeset state to `DocsUpdated`
11. [x] CursorBackend does NOT reject `Goal::UpdateDocs`
12. [x] StubBackend returns a valid stub response for `Goal::UpdateDocs`
13. [x] Architecture docs (`architecture.md`) updated with UpdateDocs info
14. [x] All existing tests continue to pass

## Testing Plan

**Level**: Integration tests (same pattern as refactor/validate goals)

**Tests**:
- Parser: `parse_update_docs_response` extracts valid output
- Parser: rejects wrong goal field
- Schema: validates against `update-docs.schema.json`
- CLI: `--goal update-docs` accepted
- CLI: `--goal update-docz` rejected (typo)
- Workflow graph: `update-docs` task exists in full graph
- Full workflow: chains all 9 steps (plan → ... → refactor → update-docs)
- Full workflow: skip-demo variant includes update-docs
- State machine: `next_goal_for_state("RefactorComplete")` = `"update-docs"`
- State machine: `next_goal_for_state("DocsUpdated")` = `None`
- Hooks: `before_update_docs` sets prompt from artifacts
- Hooks: `after_update_docs` updates changeset state
- CursorBackend: does not reject `Goal::UpdateDocs`
- StubBackend: returns valid response for `Goal::UpdateDocs`

## References

- `@.cursor/commands/wrap-context-docs.md` — Behavioral reference for documentation update logic
- `packages/tddy-core/src/workflow/refactor.rs` — Pattern to follow (system prompt + build_prompt)
- `packages/tddy-core/src/workflow/tdd_graph.rs` — Graph builder
- `packages/tddy-core/src/workflow/tdd_hooks.rs` — Hooks pattern
