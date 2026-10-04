# Code navigation in the session code pane

Go-to-definition, hover and references for Rust files previewed in the worktree
[Code pane](../../../docs/ft/web/session-code-pane.md), answered by
`code_navigation.CodeNavigationService` on the host that owns the session
([contract](../../tddy-service/docs/code-navigation-proto.md)).

## Where the code lives

All under `src/components/session/`:

| File | Responsibility |
|---|---|
| `CodeBlock.tsx` | The highlighted preview. Gives every line and every identifier its position, and reports ctrl/cmd-click and pointer dwell |
| `useCodeNavigation.ts` | `useCodeNavigation(api, navigation)`: the selected file, the hover card, the location list and the notice, with the handlers (`navigate`, `showHover`, `showReferences`, `openLocation`, `openFile`) that act on them |
| `CodeNavigationOverlays.tsx` | `HoverCard`, `LocationListCard` and `NavigationNotice` |
| `codeNavigationApi.ts` | `createCodeNavigationApi`: binds session token, project id and worktree path once, so the pane speaks only worktree-relative paths and positions |
| `WorktreeCodePane.tsx` | Composes the file tree, the preview, the overlays and, for a plan file, the "Open as plan" entry and the plan dialog |
| `restructurePlanApi.ts` | `createRestructurePlanApi` (the three plan calls with the session token, project id and worktree path bound once) and `isRestructurePlanFile` |
| `RestructurePlanDialog.tsx` | The plan dialog: operation table, stale notice, Run |

`SessionsDrawerScreen` resolves a `CodeNavigationService` client for the host that owns the selected
session (`useDaemonClientFor`) and passes it through `SessionMainPane` as `codeNavigationClient`. Absent,
the pane is the read-only preview and offers no navigation. The same client feeds the header's
[indexing indicator](session-start-and-indexing-progress.md#indexing-indicator).

## Positions

`CodeBlock` renders each line as `worktree-code-line-<n>` and each identifier as
`worktree-code-identifier-<line>-<column>`. Columns are one-based **UTF-8 byte** columns, the daemon's
coordinates, not JavaScript string indices: the block counts bytes while it walks the syntax tree, so a
file with multi-byte characters before an identifier still addresses it correctly.

## Behaviour

- **Rust only.** Navigation is offered when the open file ends in `.rs` and a navigation client
  exists, because the index serves Rust and refuses other files. Other files render as before, without
  identifier handlers.
- **Ctrl/cmd-click** on an identifier requests its definition. One location opens in the same pane,
  scrolled to its line, which carries `data-navigation-target="true"`. Several locations open a
  "Definitions" list; none shows "No definition found". A location outside the worktree is listed but
  disabled.
- **Hover** requests the identifier's hover after the pointer has rested on it for 250 ms. Only the
  latest request may show its card; an older answer arriving late is dropped. The card
  (`worktree-code-hover`) shows the markdown in pre-wrapped text and carries a **References** action and
  **Close**.
- **References** lists every location (`worktree-code-references`); selecting one opens its file at that
  line.
- A failed call shows an alert notice naming the action; opening another file dismisses the card, list
  and notice of the previous one.

The overlays sit over the bottom of the preview rather than being anchored to the identifier.

## Restructure plans

`isRestructurePlanFile(relPath, content)` decides whether the open preview is a plan: the path ends in
`.jsonl` and the first non-blank line is a JSON object with a numeric `v` and a `snapshot` object
(schema v1) or a `files` object (schema v2), the header `Plan::parse` reads. An event log, whose lines
are not headers, is never offered as a plan. `WorktreeCodePane` shows **Open as plan**
(`worktree-code-open-as-plan`) above the preview when the open file is a plan, its content loaded
without error, and a `navigationClient` exists; without the client the pane never offers it.

`RestructurePlanDialog` (`restructure-plan-dialog`) takes the `RestructurePlanApi` and the plan's
worktree-relative path. On mount it calls `open`, then follows `watch` until it is closed; each snapshot
replaces the rows. The table has one row per operation (`restructure-plan-row-<id>`, with `data-status`)
and the columns `id`, `op`, `item` (the item with its file, or the file alone for a range anchor),
`group`, `status` and `stale`. Statuses are `pending`, `in_flight`, `applied`, `failed` and
`rolled_back`.

- **Stale.** An operation with a stale reason shows `stale — <reason>`; the dialog lists each one as
  `<id>: stale — <reason>` in `restructure-plan-stale-notice`, and **Run** is disabled.
- **Run.** `restructure-plan-run` calls `run`, which streams one update per applied operation. Each
  `applied` update turns its row applied as it arrives, overriding the watched snapshot. Run is disabled
  while a run is under way, while the plan has no rows, and while any operation is stale. A run is always
  the whole plan.
- **Failure.** A failed run shows its message in `restructure-plan-error`. When it names a group, the
  `rolled_back` operations' rows show `rolled_back`; operations applied outside the group stay
  `applied`.
- **Close** (`restructure-plan-close`) closes the dialog and ends the watch.

## Testing

`cypress/component/WorktreeCodePaneNavigation.cy.tsx` mounts the pane through `SessionsDrawerScreen`
with `mountWithRpc` and an in-memory backend serving `CodeNavigationService`: ctrl-click opens the
definition's file at its line (and the recorded request carries token, project, worktree, `rel_path`
and the clicked position), hover shows the type, and the references list navigates. Test ids live in
`cypress/support/testIds.ts`; the page object is `cypress/support/pages/worktreeCodePanePage.ts`.

`cypress/component/RestructurePlanDialog.cy.tsx` mounts the pane the same way with a stub
`CodeNavigationService` serving the plan calls (the run test holds its stub at a gate to assert the
mid-run state): opening a plan file shows Open as plan and another `.jsonl` does not; the dialog lists
operations with status and group; a stale operation disables Run and names it; running turns each row
applied as its event arrives; a failing group shows its operations rolled back and no other row. The page
object is `cypress/support/pages/restructurePlanDialogPage.ts`.
