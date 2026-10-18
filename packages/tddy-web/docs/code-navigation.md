# Code navigation in the session code pane

Go-to-definition, hover and references for Rust files previewed in the worktree
[Code pane](../../../docs/ft/web/session-code-pane.md), answered by
`code_navigation.CodeNavigationService` on the host that owns the session
([contract](../../tddy-service/docs/code-navigation-proto.md)).

## Where the code lives

All under `src/components/session/`:

| File | Responsibility |
|---|---|
| `CodeBlock.tsx` | The highlighted preview. Gives every line and every identifier its position, reports pointer dwell (250 ms), ctrl/cmd-click and a plain click on a definable identifier. The highlighted tree is memoised (`HighlightedCode` re-renders only for content, language, target line, theme or file); each `Identifier` reads its own definable flag with `useSyncExternalStore`, so a probe answer re-renders that identifier alone |
| `useCodeSession.ts` | `useCodeSession(client, {sessionToken, projectId, worktreePath})`: holds the pane's `CodeSession` open while mounted and exposes `status` (a `CodeIndexStatus`) and `session` (`ask`, `cancel`). `CodeSessionFollower` owns the stream and its reopening |
| `streamReopening.ts` | `StreamReopener` and `streamEndOf`: the reopen-with-backoff schedule and the refusal set, shared with the header's `WatchCodeIndex` follower |
| `codeProbes.ts` | `ProbeStore`: probes per file + position, the definable flags `CodeBlock` reads, and `forget` |
| `useCodeNavigation.ts` | `useCodeNavigation(api, session)`: composes the selected file, the transient UI (hover card, location list, notice), the latest-query slots, the probes and the lookups below |
| `navigationLookup.ts`, `useDefinitionLookup.ts`, `useHoverQuery.ts`, `useReferencesLookup.ts` | One `NavigationContext` shared by the three lookups; `useLookup` asks a definition or references lookup, shows it held, and reads its answer against the index state |
| `CodeNavigationOverlays.tsx` | `HoverCard` (with `useCardPosition`, `HoverCardBody`, `HoverCardActions`), `LocationListCard` and `NavigationNotice` |
| `renderSimpleMarkdown.tsx` | The hover card's and markdown preview's renderer: headings (`#`–`######`), fenced code (an unclosed fence runs to the end), lists and paragraphs, one reader per block kind |
| `codeNavigationApi.ts` | `CodePosition`, `CodeLocationRef` and `toLocationRef` |
| `WorktreeCodePane.tsx` | Composes the file tree, the preview, the overlays, the code-index status strip along the bottom, and for a plan file the "Open as plan" entry and the plan dialog |
| `CodeIndexStatusStrip.tsx`, `useCodeIndexProgress.ts` | The strip showing where the worktree's code index stands, and the status classification it shares with the header's indicator ([indexing indicator](session-start-and-indexing-progress.md#indexing-indicator)) |
| `restructurePlanApi.ts` | `createRestructurePlanApi` (the three plan calls with the session token, project id and worktree path bound once) and `isRestructurePlanFile` |
| `RestructurePlanDialog.tsx` | The plan dialog: operation table, stale notice, Run |

`SessionsDrawerScreen` resolves a `CodeNavigationService` client for the host that owns the selected
session (`useDaemonClientFor`) and passes it through `SessionMainPane` as `codeNavigationClient`. Absent,
the pane is the read-only preview and offers no navigation and no strip. The same client feeds the
header's [indexing indicator](session-start-and-indexing-progress.md#indexing-indicator).

## The code session

`WorktreeCodePane` opens one `code_navigation.CodeSession` for its worktree while it is mounted with a
navigation client, keyed by client, token, project and worktree; a change of any of them closes the old
session and opens a fresh one. Its first frame is `open`; the host then warms the worktree's index and
pushes `CodeIndexProgress` for as long as the stream is open.

- **Queries.** `session.ask(kind, relPath, position, {onHeld})` sends a `CodeQuery` with a client-chosen
  `request_id` and returns a handle (`id`, `outcome`). A `held` event calls `onHeld`; the `answer` with the
  same id settles the outcome. `session.cancel(id)` sends `CancelCodeQuery` and settles the outcome as
  cancelled. Hover and the definition / references lookup each keep one **latest** slot
  (`useLatestQuery`): a newer query, closing the card or opening another file cancels the older one on
  the host, and only the latest outcome is delivered.
- **Reconnecting.** When the stream ends or fails under the pane, `CodeSessionFollower` reopens it on the
  `StreamReopener` schedule — 1 s, doubling to 30 s, back to 1 s only once a stream has delivered and
  stayed open 10 s (`STABLE_STREAM_MS`), so a host that accepts and drops at once is backed off. `status`
  reads `reconnecting` with the reason meanwhile, and every query still waiting — held, or awaiting its
  answer, probes included — is asked again on the new stream with the same id, so the navigation hooks
  never see the reconnect. A refusal (`INVALID_ARGUMENT`, `NOT_FOUND`, `PERMISSION_DENIED`,
  `UNAUTHENTICATED`, `FAILED_PRECONDITION`, `UNIMPLEMENTED`) is not reopened: `status` reads `failed` and
  waiting queries reject. Unmounting stops reopening.
- **Ending it.** The LiveKit transport never reads an `AbortSignal`, so closing the request stream is what
  ends the session on every transport; the signal ends it sooner where it is read, and a `stopped` flag
  drops anything that arrives after.

## Positions

`CodeBlock` renders each line as `worktree-code-line-<n>` and each identifier as
`worktree-code-identifier-<line>-<column>`, carrying `data-identifier-at="<line>:<column>"`. Columns are
one-based **UTF-8 byte** columns, the daemon's coordinates, not JavaScript string indices: the block
counts bytes while it walks the syntax tree, so a file with multi-byte characters before an identifier
still addresses it correctly.

## Behaviour

- **Rust only.** Navigation is offered when the open file ends in `.rs` and a navigation client
  exists, because the index serves Rust and refuses other files. Other files render as before, without
  identifier handlers.
- **Dwell.** After the pointer has rested on an identifier for 250 ms the pane probes it and asks its
  hover.
- **Probe and underline.** `ProbeStore` asks a `PROBE` once per file + position (not again while in flight
  or once answered). `DEFINABLE` marks the identifier `data-definable="true"` with `underline` and
  `cursor-pointer`, remembering its locations; `NONE` leaves it plain; `UNKNOWN` (the index is loading) or
  a failure is not remembered, so the next dwell asks again. Opening a file — including a location in the
  same file — calls `forget`: answers are dropped, probes in flight are cancelled on the host, and their
  late answers are ignored (a generation counter). An index that goes back to loading after an edit does
  not clear the remembered answers.
- **Go to definition.** A plain click on a definable identifier navigates with the probe's cached
  locations, asking nothing; ctrl/cmd-click on any identifier asks a `DEFINITION`. One location opens in
  the same pane, scrolled to its line, which carries `data-navigation-target="true"`. Several open a
  "Definitions" list; none from a `READY` index shows "No definition found". A location outside the
  worktree is listed but disabled. A plain click on an identifier that is not definable asks nothing.
- **Hover.** The hover card (`worktree-code-hover`) is positioned by `useCardPosition` inside the
  preview's positioned, non-scrolling wrapper: it finds its identifier by `data-identifier-at` at every
  layout (so a re-rendered preview never leaves it on a detached element), sits just below it, flips above
  when it would run past the bottom and there is room above, and is clamped horizontally. It is
  `max-width: min(40rem, calc(100% - 2rem))`, `max-height: 20rem`; its body (`worktree-code-hover-body`)
  scrolls and renders the markdown with `renderSimpleMarkdown`; **References** and **Close**
  (`worktree-code-hover-close`) sit in a non-scrolling footer. It is positioned when it opens and on each
  re-render, and does not follow the preview while it scrolls.
- **Held.** A hover the index holds opens the card in a loading state (`worktree-code-hover-loading`); a
  held definition or references lookup shows the held notice (`worktree-code-notice`). Both read *Index
  loading — `<phase>` `<n>`%* from the session's live progress (*Index loading* without a phase). Moving
  the pointer to another identifier drops a loading card and cancels its query.
- **Empty answers.** A definition or references answer that found nothing from an index that is not
  `READY` reads *The code index has not loaded — nothing found yet*, never "No definition found".
- **References** lists every location (`worktree-code-references`); selecting one opens its file at that
  line.
- A failed call — including a held query on an index that then failed, answered `error` — shows an alert
  notice naming the action; opening another file dismisses the card, list and notice of the previous one.

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

The code-session specs mount the pane against `cypress/support/rpc/codeSessionBackend.ts`
(`aCodeIndexHost`), a stateful fake of the daemon's `CodeSession` handler that records every frame, can
hold queries until it reports ready, fail the load (`failLoading`), drop or refuse sessions, and refuses
bad input (a query without a position ends the session `INVALID_ARGUMENT`; an undeclared file is
`NOT_FOUND`). Fluent host assertions live in `codeSessionAssertions.ts` (`expectTheHost(host)…`); the
shared geometry worktree fixture in `cypress/support/fixtures/geometryWorktree.ts`. The specs import
`src/index.css` so Tailwind utilities (underline, cursor) apply.

| Spec | Pins |
|---|---|
| `WorktreeCodePaneCodeSession.cy.tsx` | opening the pane opens a session and shows live progress until ready; every running phase; held hover and definition; "No definition found" only from a loaded index; underline and plain click; closing ends the session; cancels on card close and on moving off; a held definition reporting the failure; probe caching, `UNKNOWN` re-probing, `forget` cancelling probes in flight; a plain click on a plain identifier asks nothing |
| `WorktreeCodePaneReconnect.cy.tsx` (7, `cy.clock`) | reopening after 1 s with *Reconnecting…*; doubling to 30 s; back to 1 s after a stable 10 s; a host that accepts and drops at once is backed off; a held hover asked again on the reopened session; closing stops reopening; a refused session is not reopened |
| `CodeHoverCard.cy.tsx` | the card opens beside its identifier; a long hover stays within its bounds with its actions visible; markdown headings; a fenced rust block; an unclosed fence |
| `CodeIndexStatusStrip.cy.tsx` | the strip's own states (see [indexing indicator](session-start-and-indexing-progress.md#code-pane-strip)) |
| `WorktreeCodePaneNavigation.cy.tsx` | mounted through `SessionsDrawerScreen` with `mountWithRpc`, the drawer serving its sessions and the `CodeSession` from one backend: ctrl-click opens the definition's file at its line (the recorded frames carry token, project, worktree, `rel_path` and the clicked position), hover shows the type, and the references list navigates |

Test ids live in `cypress/support/testIds.ts`; the page object is
`cypress/support/pages/worktreeCodePanePage.ts`, and the pane driver
(`cypress/support/drivers/worktreeCodePaneDriver.tsx`) exposes `codePreview.{dwellOn, goToDefinitionOf,
plainClick, listReferences, closeHoverCard}`.

`cypress/component/RestructurePlanDialog.cy.tsx` mounts the pane the same way with a stub
`CodeNavigationService` serving the plan calls (the run test holds its stub at a gate to assert the
mid-run state): opening a plan file shows Open as plan and another `.jsonl` does not; the dialog lists
operations with status and group; a stale operation disables Run and names it; running turns each row
applied as its event arrives; a failing group shows its operations rolled back and no other row. The page
object is `cypress/support/pages/restructurePlanDialogPage.ts`.
