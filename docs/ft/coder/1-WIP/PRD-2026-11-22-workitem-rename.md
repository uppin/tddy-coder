# Planned PRs become work items, stacks become work queues — PRD

**Date**: 2026-11-22
**PRD Type**: Refactor (rename, no behaviour change)
**Stack**: `#workqueue` 3/14 — branch `feature/workqueue/workitem-rename`, base `feature/workqueue/github-event-inbox`

## Affected Features

- **Primary Feature**: [PR stacking](../pr-stacking.md) — becomes the work-queue feature doc
- [PR stack live status](../pr-stack-live-status.md), [prstack package](../../web/prstack-package.md), [Session drawer](../../web/session-drawer.md)

## Summary

The product's vocabulary changes everywhere it appears: a **planned PR** is a **work item** and a
**stack** (as a container) is a **work queue**. Proto package, service, RPCs and messages; the agent
tools (`pr_*` → `work_*`); Rust crates, modules and types; the web package, components, routes and
test ids; and the docs. No aliases are kept for old RPC or tool names — the daemon, the web bundle and
the tools ship together. Behaviour, on-disk data and wire field numbers are unchanged.

## Background

The rest of `#workqueue` makes the container a per-session queue of work that is not always a PR (an
item may be planned, assigned an existing PR, or linked to a session). Doing the rename alone, before
behaviour changes, keeps every later diff about behaviour and keeps this one reviewable as a
mechanical change against an unchanged test baseline.

## Proposed Changes

### What's Changing

| Before | After |
|---|---|
| crate `tddy-pr-stack` | `tddy-work-queue` |
| package `tddy-prstack-web` | `tddy-workqueue-web` |
| `pr_stack.proto`, `package pr_stack`, `PrStackService` | `work_queue.proto`, `package work_queue`, `WorkQueueService` |
| RPCs `AddPlannedPr`, `RepointPlannedPr`, `ReorderPlannedPr`, `ResolveStackBase`, `LinkStackNode`, `ListProjectStacks`, `GetProjectStack`, `CreateProjectStack` | `AddWorkItem`, `RepointWorkItem`, `ReorderWorkItem`, `ResolveWorkItemBase`, `LinkWorkItem`, `ListProjectWorkQueues`, `GetWorkQueue`, `CreateWorkQueue` (`GetPrStatus`, `QueryBranch`, `PullBaseIntoBranch` keep their names — they are about PRs and branches) |
| tools `pr_stack_list`, `pr_stack_create`, `pr_stack_status` | `work_queue_list`, `work_queue_create`, `work_queue_status` |
| tools `pr_add_planned`, `pr_update_planned`, `pr_delete_planned`, `pr_set_status`, `pr_adopt` | `work_item_add`, `work_item_update`, `work_item_delete`, `work_item_set_status`, `work_item_adopt` |
| tools `pr_merge`, `pr_close`, `pr_repoint`, `pr_set_parents`, `pr_resolve_conflicts`, `pr_spawn_child` | `work_item_merge`, `work_item_close`, `work_item_repoint`, `work_item_set_parents`, `work_item_resolve_conflicts`, `work_item_spawn_session` |
| tools `pr_read`, `pr_search`, `pr_comments` | `work_pr_read`, `work_pr_search`, `work_pr_comments` |
| `Stack`, `StackNode`, `StackRecord`, `ProjectStackStore` | `WorkQueue`, `WorkItem`, `WorkQueueRecord`, `WorkQueueStore` |
| `ProjectStackView`, `PlannedPrList`, `PlannedPrRow*`, `AddPlannedPrForm`, `ProjectStacksRow` | `WorkQueueView`, `WorkItemList`, `WorkItemRow*`, `AddWorkItemForm`, `ProjectWorkQueuesRow` |
| route `/sessions/project/:projectId/stack/:stackId` | `/sessions/project/:projectId/queue/:queueId` |
| test ids `pr-stack-…`, `project-stack-…` | `work-item-…`, `work-queue-…` |

The full symbol map is the contract commit's table; every name is final there.

### What's Staying the Same

- Behaviour, every test's assertion, and the test count per package.
- On-disk data: `projects/<project_id>/stacks/<id>.yaml`, the YAML keys and `changeset.yaml`'s
  `stack_id` keep their spelling (serde renames); n5 replaces this storage.
- Proto field numbers (`SessionEntry` and `StartSessionRequest` fields are renamed, not renumbered).
- The `.agents/` developer PR-stack workflow — a different thing, untouched.

## Impact Analysis

### Technical Impact

Roughly 1,900 code occurrences across ~25 packages (Exploration 1 §3); two crate/package directory
renames; regenerated proto code in two TS copies; closed name lists updated together.

### User Impact

The UI and agents say "work queue" and "work item". Agents' tool names change; the orchestrator
prompt names the new tools.

## Implementation Plan

One node, `feature/workqueue/workitem-rename` (#workqueue 3/14).

## Acceptance Criteria

- [ ] No product surface (proto, tool catalogue, crate and package names, web test ids, routes) carries the planned-PR or stack-container vocabulary.
- [ ] Every test suite of the touched packages passes with the same count before and after.
- [ ] Old RPC and tool names are refused (no alias).
- [ ] Existing stack records and changesets on disk still load.
