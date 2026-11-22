# Changeset: Planned PRs become work items, stacks become work queues

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — mechanical, but ~25 packages, a proto/contract change (package, service, RPC and message names), two crate/package renames, closed name lists in several crates
**Stack**: `#workqueue` 3/14 — branch `feature/workqueue/workitem-rename`, base `feature/workqueue/github-event-inbox`
**PR**: [#1004](https://github.com/uppin/tddy-coder/pull/1004)

## Initial Discovery

[2026-11-22-workitem-rename-initial-discovery.md](./2026-11-22-workitem-rename-initial-discovery.md)

## Affected Packages

Every package in Exploration 1 §3's count table that survives n1, chiefly:

- **tddy-pr-stack → tddy-work-queue** (crate rename)
- **tddy-prstack-web → tddy-workqueue-web** (package rename; dependents' `package.json`)
- **tddy-service**: `pr_stack.proto` → `work_queue.proto`; `session.proto` field names (`stack_plan_json`, `stack_id`, `stack_parent`, `stack_parent_daemon_instance_id`, `stack_node_id`); codegen keys in `build.rs`, `build/unbundled_families.rs`, `build/descriptors.rs`; `tests/unbundle_service_split.rs`
- **tddy-client-web**, **tddy-rust-typescript-tests**: regenerated `work_queue_pb.ts`; `appRoutes.ts` route and segment
- **tddy-changeset**: `Stack`, `StackNode`, `StackRecord`, `StackStoreError`, `ProjectStackStore`, `project_stack_store` module, `stack_fixtures`
- **tddy-daemon-rpc**: `pr_stack/` module → `work_queue/`; `exec_tool/stack_tools*`, `stack_git_tools*`; families; tests
- **tddy-session-lifecycle**: `pr_stack_rpc.rs`, `rpc_families::pr_stack_handler`
- **tddy-daemon**: local socket server and runtime mounts
- **tddy-tool-engine**: tool names, `stack_tools.rs`/`stack_git_tools.rs` modules and types, `read_only.rs`, catalogue test
- **tddy-tools**, **tddy-tools-cli**: `exec_tool_admission.rs` closed list, dispatch, audits
- **tddy-agent-launch**: `project_home_prompt.md` tool names, `StackBaseLookup`/`StackNodeLink`/`StackParentHost`/`SpawnStackParent`
- **tddy-session-worktree**: `parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn`, `StackParentRoute`
- **tddy-session-activity**, **tddy-session-files**, **tddy-workflow-recipes** (residual references)
- **tddy-session-web**, **tddy-session-create-web**, **tddy-web**, **tddy-storybook**: components, props (`prStackClient`), test ids, page objects, specs

## Related Feature Documentation

- PRD: [PRD-2026-11-22-workitem-rename.md](../../ft/coder/1-WIP/PRD-2026-11-22-workitem-rename.md)
- Wrap updates: `docs/ft/coder/pr-stacking.md` → `work-queue.md`, `pr-stack-live-status.md` → `work-queue-live-status.md`, `pr-stack-docs.md` (what remains of it after n1), `docs/ft/web/prstack-package.md` → `workqueue-package.md`, `session-drawer.md`; package docs move with their crates

## Summary

A behaviour-preserving rename of the stack vocabulary — proto, tools, crates, types, web, test ids,
docs — with no aliases. The test baseline is the same before and after.

## Background

See the PRD. Operator decisions: full domain rename, no aliases (2026-11-22); crate, package and proto
file names included (operator answer, 2026-11-22).

## Responsibility

- Every rename in the PRD's table, and the full symbol map in the contract commit.
- Keeping on-disk spellings (serde renames on the record and changeset fields) and proto field numbers.
- Updating every closed name list in the same commit: `STACK_TOOL_NAMES`, `STACK_GIT_TOOL_NAMES`,
  `read_only.rs:152-168`, `exec_tool_admission.rs:211-227`, `stack_tools_catalogue.rs`,
  `svc_list_exec_tools_wiring.rs:250`, `caller_records.rs:405`, `mcp_tool_advertisement_audit.rs`,
  `project_home_prompt.md:72-75`, `PR_STACK_METHODS` in `unbundle_service_split.rs` (adding the missing
  `CreateProjectStack` → `CreateWorkQueue` entry).
- Moving the crate's code-issue records with the crate (`tddy-pr-stack/docs/code-issues/*` →
  `tddy-work-queue/docs/code-issues/`) and updating their file paths.
- A before/after test-count baseline per touched package, recorded in this changeset.

## Boundaries

- No behaviour change of any kind: no new field, no new RPC, no gating change, no storage change.
- Does not rename on-disk directories, YAML keys or `changeset.yaml` keys (n5 replaces the storage).
- Does not rename `GetPrStatus`, `QueryBranch`, `PullBaseIntoBranch`, `BranchResolution`,
  `PrStatusView`, `GithubPrStatus`, `PrInternalStatus` — they describe PRs and branches, not the container.
- Does not touch the `.agents/` workflow docs, `tddy-code-restructuring`'s unrelated `Stack` types,
  the terminal pane's z-stack test ids (`sessions-terminal-pane-stack-…`), or `docs/dev/changesets/` history.
- Does not touch n2's inbox.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` remove-pr-stack-recipe (#1002) | the recipe and its surfaces deleted; surviving helpers (`internal_status`, merge journal, merge/repoint execution) moved into `tddy-pr-stack` | renames only what survives n1 | rename or re-add anything n1 deletes; move code between crates |
| `n2` github-event-inbox (#1003) | inbox-backed PR leg in `pr_stack/pr_status.rs` and stack `status` | renames the module path around it | change the inbox, its API or its call sites' behaviour |

## Draft PR contract

Published (pass 2, commit 2): the complete old → new symbol map (`## Before-baseline (pass 2)

Test counts on this branch before the rename (`cargo test -p <pkg> -- --list`):

| Package | Tests |
|---|---|
| tddy-pr-stack | 75 |
| tddy-changeset | 277 (incl. the new guard) |
| tddy-tool-engine | 320 (incl. the 3 new vocabulary tests) |
| tddy-service | 177 (incl. the 2 replaced/new proto tests) |
| tddy-prstack-web (Cypress) | 7 specs, 15 `it` |
| tddy-session-web (Cypress) | 155 specs, 881 `it` (before n1's deletions) |

`tddy-daemon-rpc` and `tddy-session-lifecycle` were **not** counted: listing builds every test binary
of both, which filled the volume on 2026-11-22. `/green` counts them on its base (n2's tip) with
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` before renaming, and again after.

## Symbol map` below — every name
there is final), the renamed closed lists as **failing** vocabulary tests that read only the catalogue,
the proto text and the packages directory (so they compile the same before and after the rename), one
guard test for the on-disk spelling, and the before-baseline test counts.
`/green` performs the rename in this PR, preferring rust-analyzer renames through `tddy-tools
restructure` for Rust symbols and engine moves for the crate rename; an engine refusal is put to the
operator before any hand edit.

## Green wave

**Wave:** 2 of 8
**Greenable independently:** no — it needs n1 green and merged-ready first; renaming code n1 is about to delete wastes both PRs
**Concurrent with:** n4 session-pr-tabs
**Blocks:** n5 session-work-queue (all later queue nodes use the new vocabulary)

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

### ⚠ DURING — [oversized-file-pr-stack-ports.md](../../../packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md)

`pr_stack/ports.rs` becomes `work_queue/ports.rs`; the record's path is updated, its line count must
not change.

### ⚠ DURING — [complexity-repoint-repoint-planned-pr-node.md](../../../packages/tddy-pr-stack/docs/code-issues/complexity-repoint-repoint-planned-pr-node.md), [complexity-pull-base-pull-base-into-node-branch.md](../../../packages/tddy-pr-stack/docs/code-issues/complexity-pull-base-pull-base-into-node-branch.md)

Move with the crate; their symbol names are updated (`repoint_work_item`, …).

### ⚠ DURING — [2026-11-07-session-web-specs-do-not-typecheck.md](../todo/2026-11-07-session-web-specs-do-not-typecheck.md)

The renamed specs must not add type errors; the count is re-measured at wrap.

### Related — [2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host.md](../todo/2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host.md), [2026-08-30-pr-stack-querybranch-s-local-only-legs.md](../todo/2026-08-30-pr-stack-querybranch-s-local-only-legs.md)

Rename surface only; behaviour stays deferred. Their tool and file names go stale — updated at wrap so
they remain findable.

## Scope

- [ ] Proto package, service, RPCs, messages and `session.proto` field names renamed; TS regenerated
- [ ] Agent tools renamed; every closed list and the orchestrator prompt updated
- [ ] Crate `tddy-work-queue` and package `tddy-workqueue-web`; Rust modules and types renamed
- [ ] Web components, props, routes, test ids, page objects and specs renamed
- [ ] On-disk spellings preserved; existing records load
- [ ] Test counts equal before and after, per package
- [ ] Docs renamed and updated through the wrap

## Technical Changes

### State A (Current)

Vocabulary as listed in Exploration 1 §2–§3: `pr_stack.PrStackService` (11 RPCs, 31 messages), 17
`pr_*` tools, `StackNode`/`Stack`/`StackRecord`/`ProjectStackStore`, `tddy-pr-stack`,
`tddy-prstack-web` (59 test ids prefixed `pr-stack-`), route segment `stack`.

### State B (Target)

The same system under the work-queue vocabulary; `work_queue.WorkQueueService`; `work_*` tools.

### Delta

Renames only (see the PRD table). Serde attributes keep `stacks/`, `stack_id` and the record keys on
disk. Generated code regenerated; `schema-manifest.json` committed.

## Implementation Milestones

- [ ] Before-baseline: `./test -p` counts for every touched Rust package; spec counts for touched web packages
- [ ] Proto + codegen + Rust consumers
- [ ] Tools + closed lists + prompt
- [ ] Crate and package directory renames
- [ ] Web rename (components, props, routes, test ids, page objects)
- [ ] After-baseline equal; on-disk load test green

## Testing Plan

The existing suites are the behaviour proof — same assertions, same counts. New tests pin the
vocabulary at the closed lists (so a later node cannot add a `pr_*` tool or a stack-named RPC
unnoticed) and pin that old names are refused. One load test proves an existing record and changeset
still deserialize. No textual grep tests over `src/` (they trip on fixture strings).

## Acceptance Tests

`packages/tddy-service/tests/unbundle_service_split.rs` (existing; `PR_STACK_METHODS` and its test replaced):
- [x] `work_queue_service_declares_every_work_queue_method` — the closed list now includes `CreateWorkQueue`
- [x] `no_service_is_named_for_stacks_or_planned_prs`

`packages/tddy-tool-engine/tests/work_queue_vocabulary.rs` (new; reads `project_home_offered_catalog()` and the packages dir):
- [x] `every_work_queue_tool_is_offered_under_its_work_name`
- [x] `no_tool_in_the_catalogue_is_named_pr_underscore` — also proves no alias is kept (an old name in
  the catalogue fails it); `stack_tools_catalogue.rs` is renamed with the module by `/green`
- [x] `the_crates_and_packages_carry_the_work_queue_names`

`packages/tddy-changeset/tests/project_stack_store_acceptance.rs` (guard, green before and after — renamed with the module):
- [x] `a_record_written_before_the_rename_still_loads`

Web: `ProjectStackViewAcceptance.cy.tsx` → `WorkQueueViewAcceptance.cy.tsx` (same two tests, new ids) is
part of the rename itself; its unchanged assertions are the proof.

## Symbol map

Old → new; field numbers never change. Anything not listed keeps its name.

**Crates, packages, files**

| Old | New |
|---|---|
| crate `tddy-pr-stack` (`tddy_pr_stack`) | `tddy-work-queue` (`tddy_work_queue`) |
| package `tddy-prstack-web` | `tddy-workqueue-web` |
| `tddy-service/proto/pr_stack.proto`, `package pr_stack` | `work_queue.proto`, `package work_queue` |
| `tddy-changeset/src/project_stack_store.rs`, `changeset/stack.rs`, `stack_fixtures.rs` | `work_queue_store.rs`, `changeset/work_queue.rs`, `work_queue_fixtures.rs` |
| `tddy-work-queue/src/stack_ops/`, `stack_migration.rs` | `item_ops/`, `queue_migration.rs` (n5 deletes it) |
| `tddy-daemon-rpc/src/pr_stack/`, `exec_tool/stack_tools*`, `exec_tool/stack_git_tools*` | `work_queue/`, `exec_tool/work_queue_tools*`, `exec_tool/work_item_git_tools*` |
| `tddy-tool-engine/src/stack_tools.rs`, `stack_tools/`, `stack_git_tools.rs` | `work_queue_tools.rs`, `work_queue_tools/`, `work_item_git_tools.rs` |
| `tddy-session-lifecycle/src/pr_stack_rpc.rs` | `work_queue_rpc.rs` |
| `tddy-client-web/src/gen/pr_stack_pb.ts` (and the tests copy) | `work_queue_pb.ts` |

**Proto** (`work_queue.WorkQueueService`)

| Old | New |
|---|---|
| `PrStackService` | `WorkQueueService` |
| `AddPlannedPr`, `RepointPlannedPr`, `ReorderPlannedPr` | `AddWorkItem`, `RepointWorkItem`, `ReorderWorkItem` |
| `ResolveStackBase`, `LinkStackNode` | `ResolveWorkItemBase`, `LinkWorkItem` |
| `ListProjectStacks`, `GetProjectStack`, `CreateProjectStack` | `ListProjectWorkQueues`, `GetWorkQueue`, `CreateWorkQueue` |
| messages `*PlannedPr*`, `*StackNode*`, `*ProjectStack*`, `*StackBase*` (requests/responses) | same pattern: `*WorkItem*`, `*WorkItem*`, `*WorkQueue*`, `*WorkItemBase*` |
| `StackAddress`, `StackNodeCounts`, `ProjectStackSummary`, `UnreachableStackHost` | `WorkQueueAddress`, `WorkItemCounts`, `WorkQueueSummary`, `UnreachableWorkQueueHost` |
| fields `stack_id`, `node_id`, `stack_plan_json` | `queue_id`, `item_id`, `work_queue_json` |
| `SessionEntry.stack_plan_json` (23), `.stack_id` (38) | `.work_queue_json`, `.work_queue_id` |
| `StartSessionRequest.stack_parent` (15), `.stack_parent_daemon_instance_id` (36), `.stack_node_id` (37) | `.work_queue_parent`, `.work_queue_parent_daemon_instance_id`, `.work_item_id` |
| `PR_STACK_SERVICE = "pr_stack.PrStackService"` | `WORK_QUEUE_SERVICE = "work_queue.WorkQueueService"` |

**Agent tools** — the 17 in `tests/work_queue_vocabulary.rs` (`EVERY_WORK_QUEUE_TOOL`), mapped as in the PRD; argument keys `stack_id` → `queue_id`, `node_id` → `item_id`.

**Rust types and functions**

| Old | New |
|---|---|
| `Stack`, `StackNode`, `StackRecord`, `StackStoreError`, `ProjectStackStore` | `WorkQueue`, `WorkItem`, `WorkQueueRecord`, `WorkQueueStoreError`, `WorkQueueStore` |
| `StackHome`, `StoredStack`, `SessionStackHome` | `WorkQueueHome`, `StoredWorkQueue`, `SessionWorkQueueHome` |
| `AddPlannedPrInput`, `UpdatePlannedPrInput` | `AddWorkItemInput`, `UpdateWorkItemInput` |
| `add_planned_pr_node`, `update_planned_pr_node`, `delete_planned_pr_node`, `move_planned_pr_node`, `repoint_planned_pr_node`, `set_stack_node_parents`, `adopt_pr_into_stack`, `link_stack_node_to_child_session` | `add_work_item`, `update_work_item`, `delete_work_item`, `move_work_item`, `repoint_work_item`, `set_work_item_parents`, `adopt_pr_as_work_item`, `link_work_item_to_session` |
| `PrStackHandler`, `PrStackServiceImpl`, `PrStackRpcHandler` | `WorkQueueHandler`, `WorkQueueServiceImpl`, `WorkQueueRpcHandler` |
| `StackCaller`, `StackCallerRole`, `StackScope`, `StackOp`, `StackToolPort` | `WorkQueueCaller`, `WorkQueueCallerRole`, `WorkQueueScope`, `WorkQueueOp`, `WorkQueueToolPort` |
| `StackGitOp`, `StackGitToolPort`, `StackOpJournal` | `WorkItemGitOp`, `WorkItemGitToolPort`, `WorkQueueOpJournal` |
| `StackTarget`, `StackGithub`, `StackGithubClients`, `RealStackGithubClients` | `WorkQueueTarget`, `WorkQueueGithub`, `WorkQueueGithubClients`, `RealWorkQueueGithubClients` |
| `StackBaseLookup`, `StackNodeLink`, `StackParentHost`, `SpawnStackParent`, `StackParentRoute` | `WorkItemBaseLookup`, `WorkItemLink`, `WorkQueueParentHost`, `SpawnWorkQueueParent`, `WorkQueueParentRoute` |
| `parent_is_pr_stack_orchestrator`, `pr_stack_node_for_spawn` | `parent_owns_a_work_queue`, `work_item_for_spawn` |
| `Changeset.stack_id` | `Changeset.work_queue_id` (`#[serde(rename = "stack_id")]`; n5 removes it) |

**Web**

| Old | New |
|---|---|
| `ProjectStackView` (`ProjectStackAddress`), `PlannedPrList`, `PlannedPrRow`, `PlannedPrRowBadges`, `PlannedPrRowDetails`, `AddPlannedPrForm`, `ProjectStacksRow` | `WorkQueueView` (`WorkQueueAddress`), `WorkItemList`, `WorkItemRow`, `WorkItemRowBadges`, `WorkItemRowDetails`, `AddWorkItemForm`, `ProjectWorkQueuesRow` |
| `stackPlan.ts` (`StackNode`, `parseStackPlan`, `orderStackNodes`, `topoSortStackNodes`), `hydrateStackNodes`, `stackChildSessions` | `workQueue.ts` (`WorkItem`, `parseWorkQueue`, `orderWorkItems`, `topoSortWorkItems`), `hydrateWorkItems`, `workItemSessions` |
| prop `prStackClient` | `workQueueClient` |
| test ids `pr-stack-<x>-${nodeId}`, `pr-stack-planned-pr-list`, `pr-stack-add-planned-pr-<x>`, `pr-stack-dirty-worktree-<x>` | `work-item-<x>-${itemId}`, `work-item-list`, `work-item-add-<x>`, `work-item-dirty-worktree-<x>` |
| `project-stack-view`, `project-stack-new-stack-btn`, `sessions-drawer-project-stacks-*`, `sessions-drawer-project-stack-*` | `work-queue-view`, `work-queue-new-btn`, `sessions-drawer-project-queues-*`, `sessions-drawer-project-queue-*` |
| route `/sessions/project/:projectId/stack/:stackId`, `PROJECT_STACK_SEGMENT = "stack"`, `sessionsProjectStackPath` | `/sessions/project/:projectId/queue/:queueId`, `PROJECT_QUEUE_SEGMENT = "queue"`, `sessionsProjectQueuePath` |

## Decisions & Trade-offs

- **No aliases** (operator): daemon, web and tools ship together; an old name is an unknown tool/RPC.
- **Crates, packages and proto file renamed too** (operator answer, 2026-11-22).
- **On-disk spellings preserved** to keep this a no-behaviour change; n5 drops the old storage
  (operator answer: old stack records are deleted on upgrade), so no data rename is ever needed.
- **PR- and branch-describing names keep "PR"** (`GetPrStatus`, `QueryBranch`, `PrStatusView`,
  `work_pr_read`): they describe a pull request, which a work item may or may not have.
- **`pr_spawn_child` → `work_item_spawn_session`**: the tool starts a session for an item; "child" was
  the recipe's word.

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-workitem-rename-initial-discovery.md`)
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
