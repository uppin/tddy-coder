# Changeset: Remove the pr-stack workflow recipe

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Feature (removal)
**Complexity**: 4 — more than one package (≈20, web and Rust), a proto change (field 31 reserved), code moved between crates under live daemon tools, a test-coverage re-point
**Stack**: `#workqueue` 1/14 — branch `feature/workqueue/remove-pr-stack-recipe`, base `master`

## Initial Discovery

[2026-11-22-remove-pr-stack-recipe-initial-discovery.md](./2026-11-22-remove-pr-stack-recipe-initial-discovery.md)

## Affected Packages

- **tddy-workflow-recipes**: delete `pr_stack/`, `plan_pr_stack/`, `orchestrate_pr_stack/`, the `write_stack_{plan,docs}` goal protos and generated schemas, the catalogue entries in `recipe_resolve.rs` and `approval_policy.rs`
- **tddy-pr-stack**: receives `internal_status` (derive/reconcile), the merge journal (`StackOpJournal`, `MergePhase`, `recover_in_flight_stack_op`), `execute_stack_merge`/`execute_stack_repoint` and `PR_STACK_PLAN_MD_BASENAME`; loses `stack_ops/seed.rs`
- **tddy-daemon-rpc**: imports from `tddy_pr_stack` instead of `tddy_workflow_recipes::{pr_stack, orchestrate_pr_stack}`, `PR_STACK_SERVICE` from `tddy_pr_stack`
- **tddy-coder**: `--recipe` value list, `--stack-seed-base-session`, `seed_stack_and_enter_orchestrate`, `seed_pr_stack_from_base_session_if_requested`
- **tddy-agent-launch**: `stack_seed_validation.rs`, `start_request_checks` seed gate, `StackChildSpawnHandler` binding in `svc_start_claude_cli_session.rs`, `stack_child_spawn.rs`, spawn plan fields
- **tddy-agent-backend**, **tddy-presenter**: TUI recipe picker labels
- **tddy-agent-skills**: `SHIPPED_WORKFLOW_RECIPE_CLI_NAMES`
- **tddy-spawn**, **tddy-supervisor** wire (`spawn_worker`, `supervisor_spawn`): `stack_seed_base_session`
- **tddy-telegram-control**: `None` seed argument
- **tddy-tool-engine**: `PR_STACK_RECIPES`, `pr_stack_orchestrators`, `pr_stack_base_candidates`, `stack_base_session_candidates`, `StartSession` schema argument
- **tddy-tools**, **tddy-tools-cli**: tests pinned to `PR_STACK_TOOL_NAMES` and the closed goal list
- **tddy-service**: `session.proto` reserves field 31 (`pr_stack_base_session_id`); regenerated TS in **tddy-client-web** and **tddy-rust-typescript-tests**
- **tddy-session-files**: imports the plan basename from `tddy_pr_stack`
- **tddy-session-worktree**: `parent_is_pr_stack_orchestrator` keys on `stack_id` only
- **tddy-session-create-web**: `WORKFLOW_RECIPES`, `PR_STACK_RECIPES`, base-session picker, `useCreateSessionStack`, request field
- **tddy-session-web**: `workflowViews.tsx`, `attachClaim.ts`, 39 specs driven through `recipe: "pr-stack"`
- **tddy-prstack-web**: `PrStackScreen`, `PrStackStartSessionDialog`, `usePrStack{Plan,StartSession,BranchMutations}`, `stackDocAttachments`, `PlannedPrPanel`, `PrStackChat`
- **tddy-web**: PrStack presenter/chat specs, e2e `pr-stack-chat-response.cy.ts`, `startDaemonForPrStack`, `cypress:e2e:prstack`
- **tddy-storybook**: `PlannedPrPanel.stories.tsx`

## Related Feature Documentation

- PRD: [PRD-2026-11-22-remove-pr-stack-recipe.md](../../ft/coder/1-WIP/PRD-2026-11-22-remove-pr-stack-recipe.md)
- Wrap updates: `docs/ft/coder/pr-stacking.md`, `pr-stack-docs.md`, `pr-stack-live-status.md`, `workflow-recipes.md`, `exploration-artifact.md`, `docs/ft/web/session-drawer.md`, `prstack-package.md`; package docs `tddy-pr-stack/docs/architecture.md`, `tddy-daemon/docs/pr-stack-service.md`, `tddy-daemon/docs/connection-service.md` § Stack-seed base session, `tddy-prstack-web/docs/README.md`, `tddy-workflow-recipes/README.md`

## Summary

The product's `pr-stack` workflow recipe — recipe, aliases, goals, screen, seed flag, child spawn
handler and catalogue entries — is deleted. What the daemon's live stack tools still use moves into
`tddy-pr-stack`. Stacks are planned and driven through the project stack record only.

## Background

See the PRD. The recipe has been a legacy path since `#orchestrators` made stacks project records and
gated the tools by role (`stack_tools.rs:117-119`: "the pr-stack recipe is deprecated and never
decides a role"; `daemon-rpc/pr_stack/guards.rs:5-7`).

## Responsibility

- Deleting every recipe surface listed under Affected Packages.
- Moving the surviving stack helpers (internal status, merge journal, merge/repoint execution, plan
  basename) into `tddy-pr-stack` and repointing their consumers.
- Reserving `StartSessionRequest` field 31.
- Re-pointing the planned-PR row specs whose behaviour `ProjectStackView` keeps.
- A resume refusal that names the removed recipe.

## Boundaries

- Renames nothing: `PlannedPr`, `ProjectStack`, `pr_*`, `PrStackService` keep their names (n3 renames).
- Does not change the stack store, its key, the tools' role gating, or `ProjectStackView` behaviour.
- Does not touch GitHub polling (n2) or the inspector (n4).
- Does not migrate changesets that record the old recipe — they stay as they are on disk (operator decision: they stop resuming).
- The `.agents/` developer PR-stack workflow (commands and skill) is a different thing and is untouched.

## Dependencies

None — this is a root node, cut from `master`.

## Draft PR contract

The first push of this PR, before its implementation: the moved module surface in `tddy-pr-stack`
(`internal_status::{derive_internal_status, reconcile_internal_status}`, `op_journal::{StackOpJournal,
MergePhase, recover_in_flight_stack_op}`, `merge_repoint::{execute_stack_merge,
execute_stack_repoint}`, `PR_STACK_PLAN_MD_BASENAME`) as moved code, `reserved 31` in `session.proto`,
and the failing acceptance tests below. `/green` then deletes the recipe surfaces in this PR.

## Green wave

**Wave:** 1 of 8
**Greenable independently:** yes — removal plus moves; no test needs another node's behaviour
**Concurrent with:** n2 github-event-inbox
**Blocks:** n3 workitem-rename (it renames the vocabulary that survives here; renaming first would rename code this node deletes)

Real dependency edges (the line is n1 · n2 · n3 · … · n14; `a → b` means b needs a's behaviour):

    n1 remove-pr-stack-recipe → n3 workitem-rename
    n2 github-event-inbox     → n4 session-pr-tabs, n6 pr-and-session-link
    n3 workitem-rename        → n5 session-work-queue
    n4 session-pr-tabs        → n11 commit-inbox-outbox
    n5 session-work-queue     → n6 pr-and-session-link, n7 badges-labels, n8 move-activities
    n6 pr-and-session-link    → n9 lineage-graph
    n9 lineage-graph          → n10 waves-line, n11 commit-inbox-outbox, n12 leak-linearity
    n10 waves-line            → n13 schedules
    n11 commit-inbox-outbox   → n13 schedules
    n13 schedules             → n14 land-propagate

Waves: 1 = n1, n2 · 2 = n3, n4 · 3 = n5 · 4 = n6, n7, n8 · 5 = n9 · 6 = n10, n11, n12 · 7 = n13 ·
8 = n14 (run along the line `1 1 2 2 3 4 4 4 5 6 6 6 7 8` — never steps back). Inside each wave the
blockers lead: n1 (12 transitive dependents) before n2 (8); n3 (10) before n4 (3); n6 (6) before n7
and n8 (0 each); n10 and n11 (2 each — tie, kept in plan order) before n12 (0). The sweep node is
reserved and planned after the pipelines drain.

## Prerequisites

### ✅ RESOLVED HERE — [2026-11-14-retire-stack-child-spawn-handler.md](../todo/2026-11-14-retire-stack-child-spawn-handler.md)

`StackChildSpawnHandler` and its binding are deleted with the recipe.

### ✅ RESOLVED HERE — [2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md](../todo/2026-08-29-stack-progress-json-is-documented-as-a-host-guarantee-but-nothing-writ.md)

The contract was never implemented; the doc section is deleted at wrap.

### ✅ RESOLVED HERE — [2026-08-13-pr-stack-a-managed-claude-cli-cursor-cli-session-may-record-a-goal-id.md](../todo/2026-08-13-pr-stack-a-managed-claude-cli-cursor-cli-session-may-record-a-goal-id.md)

The recipe and its `analyze-stack` start goal go.

### ✅ RESOLVED HERE — [2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base.md](../todo/2026-08-13-pr-stack-an-externally-located-worktree-is-refused-as-a-stack-base.md)

The seed flag and its base check go.

### ✅ RESOLVED HERE — [2026-08-13-pr-stack-seeding-a-stack-from-several-existing-sessions.md](../todo/2026-08-13-pr-stack-seeding-a-stack-from-several-existing-sessions.md)

Seeding from a session goes; adding existing PRs is `pr_adopt` (and n6's assign).

### ⚠ DURING — [2026-08-13-tddy-daemon-generalize-pr-stack-spawn-args-to-all-optional-spawn-flags.md](../todo/2026-08-13-tddy-daemon-generalize-pr-stack-spawn-args-to-all-optional-spawn-flags.md)

`pr_stack_spawn_args` loses its seed tuple; the generalisation stays deferred.

### ⚠ DURING — [2026-10-25-new-session-form-offers-8-of-10-recipes.md](../todo/2026-10-25-new-session-form-offers-8-of-10-recipes.md)

The daemon list shrinks by three names; the form drops `pr-stack`. The 8-of-N mismatch is re-measured at wrap, not fixed here.

### ⚠ DURING — [squatting-pr-stack-data-model.md](../../../packages/tddy-workflow-recipes/docs/code-issues/squatting-pr-stack-data-model.md)

Every remaining consumer stops reaching `tddy-pr-stack` through `tddy-workflow-recipes`; the record is expected to close here — measured at wrap.

### ⚠ DURING — [complexity-bridge-execute-stack-repoint.md](../../../packages/tddy-workflow-recipes/docs/code-issues/complexity-bridge-execute-stack-repoint.md)

The function moves to `tddy-pr-stack` unchanged; its record moves with it (n14 rewrites it into the propagation engine).

### ⚠ DURING — [misplaced-tests-github-rest-client.md](../../../packages/tddy-workflow-recipes/docs/code-issues/misplaced-tests-github-rest-client.md)

The three suites test `tddy_github` through `orchestrate_pr_stack::github`; they move to `tddy-github` with the module's deletion.

### Related — [2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md](../todo/2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md)

The flaking suite is deleted with `StackChildSpawnHandler`; the entry closes if nothing else carries the test.

## Scope

- [ ] Recipe, aliases, goals, goal schemas and every recipe-name list entry removed
- [ ] `PrStackScreen` and its private modules removed; legacy-recipe sessions open in the workflow chat view
- [ ] Stack base-session seeding removed end to end; `StartSessionRequest` field 31 reserved
- [ ] `StackChildSpawnHandler` removed
- [ ] Surviving helpers moved into `tddy-pr-stack`; consumers repointed
- [ ] Resume of a legacy-recipe session refused with the recipe named
- [ ] Planned-PR row specs re-pointed at `ProjectStackView`
- [ ] Feature and package docs updated through the wrap

## Technical Changes

### State A (Current)

The recipe is defined in `tddy-workflow-recipes/src/pr_stack/mod.rs:134` (`analyze-stack →
write-stack-plan → write-stack-docs → orchestrate`), aliased in `recipe_resolve.rs:110`, listed in six
catalogues, seeded through `--stack-seed-base-session` (web picker → `session.proto:737` field 31 →
`stack_seed_validation.rs` → spawn plumbing → `tddy-coder/src/run.rs:1738-1779`), viewed through
`PrStackScreen` (`session-web/workflowViews.tsx:106`). The daemon's live stack tools import
`orchestrate_pr_stack::{internal_status, transient, bridge}` and `pr_stack::stack_ops` re-exports.

### State B (Target)

No recipe surface names a stack recipe. The stack crate holds every stack helper the daemon uses. The
only ways to plan or drive a stack are `ProjectStackView` and the `pr_*` tools.

### Delta

- `tddy-workflow-recipes`: −3 modules, −2 goal protos/schemas, catalogue entries; generated
  `goal_registry.rs` and `schema-manifest.json` regenerated (commit the manifest).
- `tddy-pr-stack`: +`internal_status.rs`, +`op_journal.rs`, +`merge_repoint.rs` (moved verbatim),
  −`stack_ops/seed.rs`.
- Consumers: import-path changes in daemon-rpc, session-files, tools tests.
- Web: dropdown, picker, request field, screen and specs as listed.

## Implementation Milestones

- [ ] Move the surviving helpers into `tddy-pr-stack`; daemon-rpc and session-files compile against them
- [ ] Delete the recipe modules and goal schemas; regenerate goal registry and manifest
- [ ] Delete seed plumbing CLI → spawn → daemon → proto (reserve 31) → MCP schema → web
- [ ] Delete `PrStackScreen` and private modules; route legacy-recipe sessions to the workflow chat view
- [ ] Re-point surviving planned-PR row specs at `ProjectStackView`; delete the rest with a list in this changeset
- [ ] Resume refusal names the removed recipe

## Testing Plan

Removal is proved by catalogue tests (what is offered) and by the existing stack tool suites staying
green after the move (`stack_plan_tools_acceptance.rs` 10, `stack_git_tools_acceptance.rs` 7,
`project_stacks_rpc_acceptance.rs` 7). Web: one Cypress component spec per removed surface, plus the
re-pointed row specs. No e2e is added; the PrStack e2e is deleted with the screen.

## Acceptance Tests

`packages/tddy-workflow-recipes/tests/recipe_catalogue_acceptance.rs`:
- [ ] `no_recipe_name_resolves_to_a_stack_recipe` — `pr-stack`, `plan-pr-stack`, `orchestrate-pr-stack` all give the unknown-recipe error
- [ ] `the_supported_recipe_list_names_no_stack_recipe`
- [ ] `the_recipes_crate_holds_no_stack_module` (replaces `pr_stack_crate_shape.rs`)

`packages/tddy-coder/src/run.rs` (unit):
- [ ] `the_recipe_flag_offers_no_stack_recipe`
- [ ] `the_cli_has_no_stack_seed_flag`

`packages/tddy-agent-launch/tests/legacy_stack_recipe_resume_acceptance.rs`:
- [ ] `resuming_a_session_recorded_with_the_removed_stack_recipe_is_refused_naming_it`

`packages/tddy-session-activity/tests/legacy_stack_recipe_listing_acceptance.rs`:
- [ ] `a_session_recorded_with_the_removed_stack_recipe_still_lists`

`packages/tddy-tool-engine/tests/start_session_schema.rs`:
- [ ] `start_session_offers_no_stack_base_session_argument`

`packages/tddy-service/tests/session_proto_reserved_fields.rs`:
- [ ] `start_session_request_reserves_the_removed_stack_seed_field`

`packages/tddy-session-create-web/cypress/component/CreateSessionRecipeChoicesAcceptance.cy.tsx`:
- [ ] `the recipe dropdown offers no PR stack recipe`
- [ ] `the start form shows no stack base-session picker for any recipe`

`packages/tddy-session-web/cypress/component/LegacyStackRecipeSessionAcceptance.cy.tsx`:
- [ ] `a session recorded with the removed stack recipe opens in the workflow chat view`

Regression guards that must stay green unchanged: the three daemon-rpc stack suites above,
`tddy-pr-stack/tests/stack_migration_sweep_acceptance.rs`, `tddy-daemon/tests/startup_stack_migration_acceptance.rs`,
`session-web/cypress/component/SessionsDrawerProjectStacksAcceptance.cy.tsx`.

## Decisions & Trade-offs

- **Move, don't delete, the helpers the daemon uses** (internal status, merge journal, merge/repoint
  execution). Deleting them would break `pr_merge`/`pr_repoint`; rewriting them belongs to n14.
- **Legacy sessions are not migrated.** The operator accepted that they stop resuming; listing and
  opening must keep working (listing never resolves the recipe — verified in Exploration 4).
- **Row coverage is re-pointed, not dropped**, where `ProjectStackView` renders the same row
  (`PlannedPrList`/`PlannedPrRow*` are shared). Specs of behaviour only `PrStackScreen` had (the chat
  pane, the presenter room, plan-panel toggles) are deleted, listed here when `/green` makes the cut.
- **Field 31 is reserved**, never reused.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-remove-pr-stack-recipe-initial-discovery.md`)
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
