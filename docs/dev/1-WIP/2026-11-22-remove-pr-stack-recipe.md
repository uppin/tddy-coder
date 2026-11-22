# Changeset: Remove the pr-stack workflow recipe

**Date**: 2026-11-22
**Status**: 🚧 In Progress — green phase
**Type**: Feature (removal)
**Complexity**: 4 — more than one package (≈20, web and Rust), a proto change (field 31 reserved), code moved between crates under live daemon tools, a test-coverage re-point
**Stack**: `#workqueue` 1/14 — branch `feature/workqueue/remove-pr-stack-recipe`, base `master`
**PR**: [#1002](https://github.com/uppin/tddy-coder/pull/1002)

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

Published (pass 2, commit 2): **the failing acceptance tests below — no new surface.** A removal node
owns no symbol a child compiles against, and the helper moves cannot be published early as re-exports:
`tddy-pr-stack` cannot depend on `tddy-workflow-recipes` (the recipes crate depends on it), so the move
*is* the implementation. `/green` moves `internal_status`, the merge journal (`StackOpJournal`,
`MergePhase`, `recover_in_flight_stack_op`), `execute_stack_merge`/`execute_stack_repoint` and
`PR_STACK_PLAN_MD_BASENAME` into `tddy-pr-stack`, reserves field 31, and deletes the recipe surfaces.
Children (n3) depend on that **behaviour**, not on a published signature.

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
- [x] `no_recipe_name_resolves_to_a_stack_recipe` — `pr-stack`, `plan-pr-stack`, `orchestrate-pr-stack` all give the unknown-recipe error
- [x] `the_supported_recipe_list_names_no_stack_recipe`
- [x] `the_recipes_crate_holds_no_stack_module` (replaces `pr_stack_crate_shape.rs`)

`packages/tddy-coder/src/run.rs` (unit):
- [x] `the_recipe_flag_offers_no_stack_recipe`
- [x] `the_cli_has_no_stack_seed_flag`

`packages/tddy-agent-launch/src/svc_resume_sandboxed_cli_harness_session.rs` (unit, `legacy_stack_recipe_tests`):
- [x] `resuming_a_session_recorded_with_the_removed_stack_recipe_is_refused_naming_it` — drives the
  private `recorded_managed_recipe` the resume path calls, so no daemon harness is needed

Listing is not given a test: Exploration 4 shows no listing path resolves the recipe, so a listing
test would pass before and after (a guard, not a red test). The session-web spec below covers
opening such a session.

`packages/tddy-tool-engine/tests/start_session_schema.rs`:
- [x] `start_session_offers_no_stack_base_session_argument`

`packages/tddy-service/tests/session_proto_reserved_fields.rs`:
- [x] `start_session_request_reserves_the_removed_stack_seed_field`

`packages/tddy-session-create-web/cypress/component/CreateSessionRecipeChoicesAcceptance.cy.tsx`:
- [x] `the recipe dropdown offers no PR stack recipe`
- [x] `the start form shows no stack base-session picker for any recipe`

`packages/tddy-session-web/cypress/component/LegacyStackRecipeSessionAcceptance.cy.tsx`:
- [x] `a session recorded with the removed stack recipe opens in the workflow chat view`

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
- **`session-web/cypress/component/PrStackInspectorAcceptance.cy.tsx`** drives a `pr-stack` session's
  screen and is deleted with it (found in pass 2).
- **Code issues claimed by this PR** (pass 2): `squatting-pr-stack-data-model.md` and
  `misplaced-tests-github-rest-client.md` now carry `**Claimed by:** #1002`.

## Deleted tests and files (green)

Every entry tested a surface this node removes. Tests of a surviving helper were kept, their imports
repointed, their assertions unchanged.

**Rust — whole files deleted**

- `tddy-workflow-recipes/tests/`: `orchestrate_pr_stack_bridge_acceptance` (the plan→stack seed),
  `orchestrate_pr_stack_github_on_demand` (the recipe's engine tasks), `plan_pr_stack_acceptance`
  (the plan goal's parser), `pr_management_actions_acceptance` (the recipe's `pr_actions`),
  `pr_stack_artifact_paths_acceptance`, `pr_stack_docs_acceptance`, `pr_stack_docs_validation_acceptance`,
  `pr_stack_free_prompting_acceptance`, `pr_stack_seeding_acceptance`, `pr_stack_store_resume_acceptance`
  (the recipe's hooks and graph), `pr_stack_base_session_seed_acceptance` (`seed_stack_with_base_session`),
  `pr_stack_crate_shape` (replaced by `recipe_catalogue_acceptance`)
- `tddy-coder/tests/pr_stack_seed_start_acceptance.rs` (`--stack-seed-base-session`)
- `tddy-tools-cli/tests/stack_docs_submit_contract_acceptance.rs` (the `write-stack-docs` goal)

**Rust — tests deleted from files that stay**

- `tddy-workflow-recipes`: `a_planned_pr_from_the_agent_plan_keeps_its_branch_name_as_a_suggestion_only`
  (`pr_stack_branch_link_acceptance`), `reseeding_an_unspawned_plan_renumbers_from_the_new_plan_order`
  (`pr_stack_display_order_acceptance`), the three `resolver_*pr_stack*` alias tests in `recipe_resolve.rs`,
  the two tests in `approval_policy.rs`
- `tddy-coder` `run.rs`: `recipe_value_parser_accepts_pr_stack_recipes`
- `tddy-tools-cli` `submit_goal_routing_acceptance`: the three `write-stack-plan` schema cases
- `tddy-tool-engine/tests/start_options.rs`: the stack parents, stack bases and no-project-base cases;
  `tddy-daemon-rpc/tests/start_options_acceptance.rs`: `the_sessions_that_can_seed_a_stack_on_this_host_are_offered`
- `tddy-session-lifecycle`: `tests/pr_stack_base_session_spawn_acceptance.rs` is now `tests/pr_stack_spawn_args.rs`,
  keeping the `--stack-parent` and `--stack-node-id` cases and dropping every seed case (forwarding, the
  pre-spawn refusals, the repository checks, the `StartSession` field check); `stack_child_spawn_tests.rs` loses the four
  `a_child_the_agent_spawned_*` cases; `stack_child_spawn_agent_tests.rs` loses the two `StackChildSpawnHandler` cases (the
  no-coding-agents refusal is re-pointed at the conversation handler)
- Moved, not deleted: `github_pr_acceptance`, `github_qualified_head_acceptance` and `pr_status_view_acceptance`
  now live in `tddy-github/tests/`; the `PR_STACK_SERVICE` value assertion now lives in `tddy-pr-stack/src/rpc.rs`

**Web — specs deleted (behaviour only the removed screen, chat or picker had)**

- `tddy-session-create-web`: `CreateSessionPrStackBaseSessionAcceptance`, `stackParentsParity` (and its
  `stack-parent-candidates.json` fixture), the `CreateSessionPane` "stack parent picker" suite (6), three re-parenting cases in
  `CreateSessionStackNodeIdAcceptance` (the form no longer offers a parent picker; the node id still travels with
  the `stackParent` the caller opens it for), two seed cases in `CreateSessionRequestBuilder`, the `stackBaseSessionCandidates`
  suite in `stackParents.test.ts`
- `tddy-session-web`: `PrStackChatAcceptance`, `PrStackChatElicitationAcceptance`, `PrStackChatFeedbackAcceptance`,
  `PrStackChatSilentFailureAcceptance`, `PrStackChatStreamingAcceptance`, `PrStackChatSystemMessagesAcceptance`,
  `PrStackChatWorkflowErrorAcceptance` (the chat pane), `PrStackInspectorAcceptance`, `PrStackViewRoutingAcceptance`,
  `PrStackPlannedPrPanelAcceptance` (the panel toggles), `PrStackStartSessionDocAttachmentsAcceptance` (the screen's dialog
  attached documents; `ProjectStackView`'s start dialog does not), the `pr-stack` case in `WorkflowChatViewRoutingAcceptance`
  and `WorktreeCodePaneAcceptance`, two dormant-session cases in `InactiveSessionActivitiesAcceptance`, one `attachClaim` case,
  and the orchestrator case in `SessionsDrawerProjectStacksAcceptance` (the stack view case is untouched)
- `tddy-prstack-web`: `PrStackChatConnectingOverlayAcceptance`, `PrStackChatDirectMountAcceptance`,
  `PrStackScreenDirectMountAcceptance`, `prStackOrchestrators.test.ts`
- `tddy-web`: `PrStackChatNoRoomAcceptance`, `PrStackChatTokenFailureAcceptance`, `PrStackPresenterRoomAcceptance`,
  e2e `pr-stack-chat-response.cy.ts`, its `startDaemonForPrStack` / `startPrStackSession` tasks and the
  `cypress:e2e:prstack` script, `support/pages/prStackScreenPage.ts`
- `tddy-storybook`: `PlannedPrPanel.stories.tsx` (`storyPlacement.test.ts` no longer expects a `tddy-prstack-web` story)

**Web — re-pointed at `ProjectStackView`** (the sessions drawer at a stack's route over `GetProjectStack`, assertions unchanged):
`PlannedPrRowInternalStatusAcceptance` and the `PrStack*Acceptance` row specs in `tddy-session-web` that were
driven through a `recipe: "pr-stack"` session, and `PlannedPrListStartSessionDirectMountAcceptance` in `tddy-prstack-web`.

## Deletions beyond the plan

- `tddy-tool-engine` `StartOptions.stack_parents` / `pr_stack_base_candidates` and `CandidateSession`: their only
  source was `PR_STACK_RECIPES`, which the plan deletes.
- `tddy-session-create-web`: the "PR stack parent" picker (`CreateSessionStackParentSelect`) and `useCreateSessionStack`, for the
  same reason; `initialValues.stackParent` / `stackNodeId` still reach the request, and the host that owns the parent is
  still found from the session list.
- `tddy-pr-stack` `repoint_reaches_github`: its only caller was the deleted engine task.

## Contract (pass 2)

Commit 2 adds the failing acceptance tests only (see `## Draft PR contract`). Red at commit time, each
failing on this node's own missing work:

| Test | Package | Fails because |
|---|---|---|
| `recipe_catalogue_acceptance` ×3 | tddy-workflow-recipes | the names still resolve, are still listed, the modules still exist |
| `the_recipe_flag_offers_no_stack_recipe`, `the_cli_has_no_stack_seed_flag` | tddy-coder (lib) | clap still accepts them |
| `resuming_a_session_recorded_with_the_removed_stack_recipe_is_refused_naming_it` | tddy-agent-launch (lib) | the recipe still resolves |
| `start_session_offers_no_stack_base_session_argument` | tddy-tool-engine | the schema still offers it |
| `start_session_request_reserves_the_removed_stack_seed_field` | tddy-service | field 31 still declared |
| `CreateSessionRecipeChoicesAcceptance` ×2 | tddy-session-create-web (Cypress) | `pr-stack` offered; picker shown |
| `LegacyStackRecipeSessionAcceptance` ×1 | tddy-session-web (Cypress) | `PrStackScreen` still claims the session |

`recipe_value_parser_accepts_pr_stack_recipes` (tddy-coder) asserts the opposite and is deleted by
`/green`. Baseline before the tests: `cargo check -p tddy-workflow-recipes -p tddy-tool-engine -p
tddy-service -p tddy-coder --tests` clean.

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
