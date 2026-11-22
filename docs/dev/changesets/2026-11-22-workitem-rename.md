# 2026-11-22 — Planned PRs are work items and stacks are work queues, end to end
**Type:** Refactor

`#workqueue` 3/14 (#1004). A rename with no behaviour change, driven by one old → new symbol map.

- **Crates and packages:** `tddy-pr-stack` → `tddy-work-queue` (`stack_ops/` → `item_ops/`,
  `stack_migration` → `queue_migration`); `tddy-prstack-web` → `tddy-workqueue-web`.
- **Proto:** `pr_stack.proto` → `work_queue.proto`, `PrStackService` → `WorkQueueService`; RPCs and
  messages renamed (`*PlannedPr*`/`*StackNode*` → `*WorkItem*`, `*ProjectStack*` → `*WorkQueue*`);
  fields `stack_id`/`node_id`/`stack_plan_json` → `queue_id`/`item_id`/`work_queue_json`;
  `SessionEntry.work_queue_json` (23), `.work_queue_id` (38); `StartSessionRequest.work_queue_parent`
  (15), `.work_queue_parent_daemon_instance_id` (36), `.work_item_id` (37). Field numbers unchanged.
  TypeScript regenerated.
- **Agent tools:** the 17 `pr_*` tools → `work_queue_*`, `work_item_*`, `work_pr_*`; arguments
  `stack_id` → `queue_id`; every closed list updated; no aliases.
- **Rust types:** `Stack`/`StackNode`/`StackRecord`/`ProjectStackStore` → `WorkQueue`/`WorkItem`/
  `WorkQueueRecord`/`WorkQueueStore`, handler and port types likewise; `Changeset.work_queue_id`.
- **Web:** components (`WorkQueueView`, `WorkItemList`, `WorkItemRow*`, `AddWorkItemForm`,
  `ProjectWorkQueuesRow`), `workQueue.ts`, prop `workQueueClient`, route segment `queue`, test ids
  `work-item-…` / `work-queue-…`.
- **On disk unchanged:** `serde(rename)` keeps `stack_id` (changeset and store record) and
  `stack_node_id` (session metadata); the `stacks/` directory and the removed recipe names are untouched.
- **How:** a scripted, whole-word rename reading the changeset's symbol map (operator-approved instead
  of the restructure engine: ~700 identifiers over ~250 files across Rust, TS, proto and fixtures).
- **Test counts, before → after** (`cargo test -p <pkg> -- --list`): daemon-rpc 700 → 700,
  session-lifecycle 1068 → 1068, changeset 277 → 277, service 177 → 177, tool-engine 317 → 317,
  work-queue 81 → 81 (`#[test]` attribute counts identical at the base and after); web `it` counts equal
  (workqueue-web 7 in 4 specs, session-web 830 in 144 specs).
- **Code issues moved, not closed:** `oversized-file-work-queue-ports` (876 production lines, unchanged),
  `complexity-repoint-repoint-work-item`, `complexity-pull-base-pull-base-into-node-branch` (bodies
  unchanged).
- **Type checks:** `src/` type-checks clean in tddy-session-web, tddy-session-create-web, tddy-workqueue-web
  and tddy-client-web; the standing Cypress-spec debt did not grow (session-create-web 257 → 251,
  session-web 177 against 182 recorded). Fixed here: `WorkQueueView`'s own `WorkQueueAddress` and the
  wire `WorkQueueAddress` now coexist through an import alias (`WireWorkQueueAddress`).
- **Backlog:** `2026-11-14-node-session-not-listed-reads-when-its-stack-is-on-another-host` and
  `2026-08-30-pr-stack-querybranch-s-local-only-legs` carry the new tool and file names; none resolved.
