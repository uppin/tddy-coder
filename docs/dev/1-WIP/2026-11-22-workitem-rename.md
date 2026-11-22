# Changeset: Planned PRs become work items, stacks become work queues

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — mechanical, but ~25 packages, a proto/contract change (package, service, RPC and message names), two crate/package renames, closed name lists in several crates
**Stack**: `#workqueue` 3/14 — branch `feature/workqueue/workitem-rename`, base `feature/workqueue/github-event-inbox`

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

The first push of this PR, before its implementation: the complete old → new symbol map (a table in
this changeset), the renamed closed lists as **failing** catalogue/vocabulary tests (the tests below,
asserting the new names while the code still has the old ones), and the before-baseline test counts.
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

`packages/tddy-service/tests/unbundle_service_split.rs` (existing, changed):
- [ ] `work_queue_service_declares_every_work_queue_method` — the closed list now includes `CreateWorkQueue`
- [ ] `no_service_is_named_for_stacks_or_planned_prs`

`packages/tddy-tool-engine/tests/work_queue_tools_catalogue.rs` (renamed from `stack_tools_catalogue.rs`):
- [ ] `every_work_queue_tool_is_offered_under_its_work_name`
- [ ] `no_tool_in_the_catalogue_is_named_pr_underscore`

`packages/tddy-daemon-rpc/tests/work_queue_tools_acceptance.rs` (renamed from `stack_plan_tools_acceptance.rs`):
- [ ] `an_old_pr_tool_name_is_refused_as_unknown`

`packages/tddy-changeset/tests/work_queue_store_acceptance.rs` (renamed from `project_stack_store_acceptance.rs`):
- [ ] `a_record_written_before_the_rename_still_loads`

`packages/tddy-workqueue-web/cypress/component/WorkQueueViewAcceptance.cy.tsx` (renamed from `ProjectStackViewAcceptance.cy.tsx`; same two tests, new ids)

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
