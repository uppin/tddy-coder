# Remove the pr-stack workflow recipe — PRD

**Date**: 2026-11-22
**PRD Type**: Removal
**Stack**: `#workqueue` 1/14 — branch `feature/workqueue/remove-pr-stack-recipe`, base `master`

## Affected Features

- **Primary Feature**: [PR stacking](../pr-stacking.md) — the recipe sections go; the project stack record, `ProjectStackView` and the stack tools stay
- [PR stack docs](../pr-stack-docs.md) — the `write-stack-docs` goal goes
- [PR stack live status](../pr-stack-live-status.md) — the session-addressed `PrStackScreen` goes
- [Workflow recipes](../workflow-recipes.md) — three recipe names leave the catalogue
- [Session drawer](../../web/session-drawer.md), [prstack package](../../web/prstack-package.md)

## Summary

The `pr-stack` workflow recipe is deleted with its legacy aliases (`plan-pr-stack`,
`orchestrate-pr-stack`), its goals (`analyze-stack`, `write-stack-plan`, `write-stack-docs`,
`orchestrate`), the session-addressed `PrStackScreen`, the "Base the stack on" start option
(`--stack-seed-base-session` from web form to CLI), and every recipe-name list entry. Stacks are
planned and driven only through the project stack record: `ProjectStackView` and the `pr_*` agent
tools, which already work without the recipe.

## Background

Since `#orchestrators`, stacks are project records (`ProjectStackStore`) and the agent tools are
gated by role, not by recipe. The recipe is a second, legacy way into the same store, with its own
screen, goals, seed flag and spawn handler (`StackChildSpawnHandler`, already unreachable). Keeping
both doubles every change the `#workqueue` stack makes to the stack model. The operator accepted that
sessions created with the recipe stop resuming.

## Proposed Changes

### What's Changing

- The recipe catalogue (`recipe_resolve`, `approval_policy`, the TUI recipe picker, `/start-<recipe>`
  slash labels, `tddy-coder --recipe`, the web recipe dropdown, `PR_STACK_RECIPES` in tool-engine and
  web) no longer names `pr-stack`, `plan-pr-stack` or `orchestrate-pr-stack`.
- `PrStackScreen` and its private modules are deleted; a session that still records the old recipe
  opens in the generic workflow chat view.
- The "Base the stack on" picker, `StartSessionRequest.pr_stack_base_session_id` (field 31, now
  reserved), the MCP `StartSession` argument, the daemon validation, the spawn plumbing and
  `--stack-seed-base-session` are deleted.
- `StackChildSpawnHandler` is deleted (its `ChildSpawnHandler` trait stays — grill-me uses it).
- Stack code the daemon still uses moves out of the recipes crate into `tddy-pr-stack`: internal
  status derivation, the merge journal (`StackOpJournal`, `MergePhase`), merge/repoint execution, and
  the stack-plan basename; consumers import it from there.
- The documented-but-never-written child `stack-progress` contract is removed from the docs.

### What's Staying the Same

- `ProjectStackStore` records, `ProjectStackView`, `ListProjectStacks`/`GetProjectStack`/
  `CreateProjectStack`, `QueryBranch`, the 17 `pr_*` tools and their role gating.
- `WorkflowChatScreen` for every other tool recipe.
- The startup migration of legacy `Changeset.stack` into project records.

## Impact Analysis

### Technical Impact

About 40 production files lose recipe code; `tddy-workflow-recipes` loses three modules and two goal
schemas; `session.proto` reserves field 31; the daemon-rpc stack tools change imports only.

### User Impact

- The New Session form and the TUI no longer offer the PR-stack recipe or the stack base picker.
- Resuming a session created with the recipe is refused with a message naming the removed recipe;
  the session still lists and opens read-only.

## Implementation Plan

One node, `feature/workqueue/remove-pr-stack-recipe` (#workqueue 1/14).

## Acceptance Criteria

- [ ] No recipe surface (CLI, TUI, slash commands, web dropdown, MCP `StartSession`) offers `pr-stack` or its aliases.
- [ ] The start form shows no stack base-session picker, and `StartSessionRequest` carries no seed field.
- [ ] A session recorded with the removed recipe lists, opens in the workflow chat view, and its resume is refused with the recipe named.
- [ ] The daemon's stack plan and git tools behave as before (their acceptance suites pass unchanged).
- [ ] `tddy-workflow-recipes` holds no stack module; the stack helpers the daemon uses live in `tddy-pr-stack`.
- [ ] Planned-PR row behaviour still covered by Cypress runs against `ProjectStackView`.
