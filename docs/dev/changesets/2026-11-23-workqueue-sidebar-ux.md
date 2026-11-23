# Changeset: workqueue-sidebar-ux

PRD (target): [`docs/ft/web/session-drawer.md`](../../ft/web/session-drawer.md) § Work queue tab,
[`docs/ft/web/agent-activity-pane.md`](../../ft/web/agent-activity-pane.md) § Right sidebar,
[`docs/ft/coder/pr-stacking.md`](../../ft/coder/pr-stacking.md) § Opening a session's work queue.

## Problem

The work queue lived in an inspector tab. It drew its task list followed by a horizontal graph
(`master → item`) whose boxes ran off the right edge of the narrow panel; nothing joined two stacked
tasks, tasks could only be moved with up/down arrows hidden in the expanded row, and a task bound to
a PR stack looked as movable as a free one.

## State B

- **Sidebar.** The queue is a right sidebar (`work-queue-sidebar`, 440 px docked, full-width overlay
  on mobile) of the session view, in the same slot as Agent Activity, behind its own toggle
  (`work-queue-button`). The two are mutually exclusive — opening one closes the other. A session
  that owns a queue (`SessionEntry.owns_work_queue`) opens on it. The inspector's **Work queue** tab
  is gone: `work-queue` leaves `INSPECTOR_TAB_NAMES` (a stale `?inspector=work-queue` degrades to
  Details) and `defaultInspectorTab` is PR when the branch has one, else Worktree. The drawer's
  Stacks row still selects the owning session, which opens on its queue.
- **Vertical.** The horizontal `WorkQueueGraph` / `WorkQueueGraphPanel` are removed. The list is one
  box per row, top to bottom, `gap-3` apart, never scrolling sideways.
- **Brackets.** Where `GetWorkQueueGraph` reports a line edge of kind `item` (a child's PR base is the
  parent's branch) a bracket joins the **right sides** of the two boxes on the right gutter
  (`work-item-connector-<parent>-<child>`, solid). Stored dependencies are dashed brackets
  (`work-item-dependency-connector-<from>-<to>`). Overlapping spans take separate lanes. The trunk and
  external bases draw nothing. "PR bases could not be read" stays as a note.
- **Drag handle.** Every task has a grip at its left edge (`work-item-drag-handle-<id>`). A task
  bound to a PR stack — it has a branch, a PR, or sits on a line edge — has the handle disabled
  with a tooltip saying why. A free task is dragged by pointer; dropping `delta` rows away sends
  `|delta|` one-step **`ReorderWorkItem`** calls (up/down) — the same RPC and the same persisted
  `display_order` the row's arrows write. No new server API: `SetWorkQueueDependencies` changes what
  an item needs, which a drop between rows does not mean. A label filter turns dragging off (row
  distances are no longer the persisted order's).

## Packages

- `tddy-workqueue-web`: `WorkItemList` (stack container, handles, `connectors`, `onMove`),
  `WorkItemConnectors`, `WorkItemDragHandle`, `useItemDrag`, `useWorkQueueGraph`; `WorkItemRow`
  takes a `dragHandle`; graph components removed.
- `tddy-session-web`: `SessionWithActivitySidebar` (`workQueuePanel`, one panel slot),
  `SessionWorkQueueSidebar`; inspector tab and its inputs removed; `SessionMainPane` builds the panel.
- `tddy-client-web`: `work-queue` leaves `INSPECTOR_TAB_NAMES`.
- `tddy-storybook`: `WorkItemList` story.

## Tests

`WorkQueueVerticalLayoutAcceptance` (layout, gap, handles, brackets, drag rules, persisted reorder),
`SessionWorkQueueSidebarAcceptance` (placement, mutual exclusion, default open),
`SessionInspectorWorkQueueTabAcceptance` (rewritten for the sidebar), `defaultInspectorTab.test.ts`.
`WorkQueueGraphAcceptance` is removed with the graph it covered.

## Docs updated at wrap

`session-drawer.md`, `agent-activity-pane.md`, `pr-stacking.md`, `create-session-package.md`, `session-pr-inspector.md`, and `docs/ft/web/changelog/2026-11-23-work-queue-sidebar.md`.

## Wrap

File-length gate: `WorkQueueView.tsx` 500 → under 500 production lines (comment compaction after the graph panel moved out); no other touched file is over budget. Scoped checks only (TypeScript packages; no Rust touched).
