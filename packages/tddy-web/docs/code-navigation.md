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
| `WorktreeCodePane.tsx` | Composes the file tree, the preview and the overlays |

`SessionsDrawerScreen` resolves a `CodeNavigationService` client for the host that owns the selected
session (`useDaemonClientFor`) and passes it through `SessionMainPane` as `codeNavigationClient`. Absent,
the pane is the read-only preview and offers no navigation.

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

## Testing

`cypress/component/WorktreeCodePaneNavigation.cy.tsx` mounts the pane through `SessionsDrawerScreen`
with `mountWithRpc` and an in-memory backend serving `CodeNavigationService`: ctrl-click opens the
definition's file at its line (and the recorded request carries token, project, worktree, `rel_path`
and the clicked position), hover shows the type, and the references list navigates. Test ids live in
`cypress/support/testIds.ts`; the page object is `cypress/support/pages/worktreeCodePanePage.ts`.
