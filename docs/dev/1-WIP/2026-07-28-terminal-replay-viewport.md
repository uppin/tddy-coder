# Changeset: Terminal Replay — Lazy Scroll-up Viewport Integration

**Date**: 2026-07-28
**Status**: ✅ Implemented — overlay double-buffer paging, acceptance + unit tests green
**Type**: Feature

## Planning artifacts

- [x] PRD: [docs/ft/web/terminal-replay-lazy-scroll.md](../../docs/ft/web/terminal-replay-lazy-scroll.md)
- [x] Changeset: this document.
- [x] Acceptance tests (Cypress component): `cypress/component/GhosttyTerminalGrpcLazyHistory.cy.tsx`
  (10 tests) + driver `cypress/support/drivers/ghosttyTerminalGrpcLazyHistoryDriver.tsx`; and
  `cypress/component/GrpcSessionTerminalLazyHistory.cy.tsx` (forward-fill wiring).
- [x] Unit tests (bun): `src/lib/terminalHistoryLoader.test.ts` (6 tests) for the
  `TerminalHistoryForwardLoader` pure state machine.

## Affected Packages

- `tddy-terminal-rpc` / `tddy-service` — proto: `GetTerminalHistory` switched to forward chunking
  (`from_offset` + `until_offset` + `at_end` on `TerminalHistoryChunk`).
- `tddy-task` — `terminal_capture::replay_from(from_offset, until_offset, max_bytes)` (forward
  replay) + TDD.
- `tddy-terminal-rpc` — `serve_get_terminal_history_with` uses `replay_from` and maps `at_end`.
- `tddy-daemon` / `tddy-coder` — `get_terminal_history` handlers map `from_offset`/`until_offset`.
- `tddy-web` — `GhosttyTerminal` (`scrollback` + `testId` props, viewport handle API incl.
  `scrollToBottom`), `GhosttyTerminalGrpc` (owns overlay double-buffer paging: two interchangeable
  overlaid terminals, loading indicator, swap, back-to-live, encapsulated gestures),
  `GrpcSessionTerminal` (builds forward fetcher, forwards full frames, drops
  `onRegisterLoadOlderHistory`), Cypress component tests + bun unit tests.

## Related Feature Documentation

- [terminal-replay-lazy-scroll.md](../../docs/ft/web/terminal-replay-lazy-scroll.md) — the WHAT (this changeset implements it).

## Summary

Wires the `GetTerminalHistory` RPC into the Ghostty shared terminal component so the user can
scroll up to load older output. The component renders **two interchangeable, overlaid
ghostty-web terminals** sharing one rect: a live terminal (`scrollback: 0`) that always stays
mounted and keeps receiving the stream, and an older-history "page" terminal (`scrollback > 0`,
read-only). On a scroll-up-at-top gesture (or the "Load earlier output" affordance) the page
terminal is **forward-filled in the background** while a **loading indicator** is shown; once
the fill completes the two terminals **switch places** (the page terminal becomes foreground,
scrollable through the retained capture; the live terminal stays mounted underneath and current).
"Back to live" (or a scroll-down-at-bottom gesture on the page terminal) swaps back instantly.
All paging logic is encapsulated inside `GhosttyTerminalGrpc`. The live terminal stays at
`scrollback: 0`, preserving the no-duplicate-pane fix. Removes the
`GrpcSessionTerminal → onRegisterLoadOlderHistory` indirection.

## Scope

- [x] `GhosttyTerminal`: `scrollback` prop (default `0`); `testId` prop (default
  `"ghostty-terminal"`); imperative handle gains `scrollToTop()`, `scrollToBottom()`, and
  `isPinnedToBottom()`.
- [x] `GrpcStream.onMessage` payload widened to the full `SessionTerminalOutput`-shaped frame
  `{ data, endOffset, atOldest }`. `GrpcSessionTerminal` updated in lockstep (no backwards compat).
- [x] `GhosttyTerminalGrpc`: new `historyFetcher` prop; captures anchor (in state) from first
  frame with `endOffset > 0`; renders an overlaid older-history page terminal (`scrollback > 0`,
  `testId="ghostty-terminal-older"`) behind the live one; `load-earlier-history` affordance before
  first fill, `view-history` after, `back-to-live` on the page pane; progressive forward-fill loop
  (append chunks oldest→anchor until `atEnd`) run in the background with a loading indicator, then
  swap to foreground; scroll-up-on-live and scroll-down-at-page gestures (capture-phase wheel
  listeners) drive fill/swap.
- [x] `GrpcSessionTerminal`: builds `historyFetcher` via `createForwardHistoryFetcher(client, …)`,
  forwards full frames, removes `onRegisterLoadOlderHistory` prop + `historyLoaderRef`.
- [x] Proto: `GetTerminalHistory` forward chunking (`from_offset`/`until_offset`/`at_end`).
- [x] Backend: `replay_from` forward replay + handler mappings.
- [x] Acceptance tests (Cypress component) for criteria 1–9.
- [x] Bun unit tests for the forward loader state machine.

## Technical Changes

### State A (before)

- `GhosttyTerminal` constructed with `scrollback: 0` → no native scrollback; viewport APIs are
  no-ops.
- `GrpcSessionTerminal` owns the lazy loader: captures `endOffset` from the initial
  `StreamTerminalOutput` frame, builds a backward `TerminalHistoryLoader`, and exposes
  `loadNext` via `onRegisterLoadOlderHistory` to the runtime. Nothing consumes it (no viewport
  trigger, no render).
- `GrpcStream.onMessage` delivers only `Uint8Array` (raw `output.data`); offset metadata is
  discarded at the `GrpcStream` boundary.
- `GetTerminalHistory` used backward chunking (`before_offset`).

### State B (after)

- `GhosttyTerminal` accepts `scrollback` (default `0`) and `testId`; handle exposes
  `scrollToTop()`, `scrollToBottom()`, and `isPinnedToBottom()`.
- `GrpcStream.onMessage` delivers the full frame `{ data, endOffset, atOldest }`.
- `GrpcSessionTerminal` builds `historyFetcher = createForwardHistoryFetcher(client, …)` and
  passes it to `GhosttyTerminalGrpc`; it forwards full frames and keeps ACK handling. The
  `onRegisterLoadOlderHistory` prop and `historyLoaderRef` are gone.
- `GhosttyTerminalGrpc` owns the overlay double-buffer paging: renders two overlaid terminals
  (live `scrollback: 0` always mounted & streaming; page `scrollback > 0` hidden until swapped),
  captures the anchor in state, renders the affordances, runs the progressive forward fill in the
  background with a loading indicator, swaps the page terminal to the foreground (landed at its
  bottom) on `atEnd`, and swaps back on "Back to live" / scroll-down-at-bottom. Live bytes keep
  flowing to the live terminal throughout (no buffering, no reset).
- `GetTerminalHistory` uses forward chunking (`from_offset` + `until_offset` + `at_end`).

### Delta

- `proto/terminal_session.proto`, `proto/connection.proto`: forward `GetTerminalHistoryRequest`
  + `at_end` on `TerminalHistoryChunk`; regenerated stubs.
- `tddy-task/src/terminal_capture.rs`: `replay_from` + `terminal_capture_replay_from.rs` TDD.
- `tddy-terminal-rpc/src/bridge.rs`: `serve_get_terminal_history_with` uses `replay_from`.
- `tddy-daemon/src/connection_service.rs`, `tddy-coder/src/session_participant/mod.rs`: handler
  mappings.
- `GhosttyTerminal.tsx`: `scrollback`/`testId` props; handle methods incl. `scrollToBottom`.
- `GhosttyTerminalGrpc.tsx`: `historyFetcher` prop, anchor state, overlay double-buffer layout
  (live + page panes), `view`/`loading`/`filled` state, loading indicator, swap, `view-history`/
  `back-to-live` affordances, forward fill, capture-phase wheel gestures on both panes.
- `GrpcSessionTerminal.tsx`: build forward fetcher, forward full frames, drop indirection.
- `terminalHistoryLoader.ts`: `TerminalHistoryForwardLoader` + `createForwardHistoryFetcher`
  (replaces the backward rebuild controller).
- Cypress: `GhosttyTerminalGrpcLazyHistory.cy.tsx` + driver; `GrpcSessionTerminalLazyHistory.cy.tsx`.
- `cypress/support/testIds.ts`: new ids (`view-history`, `back-to-live`,
  `terminal-history-loading`, `terminal-live-pane`, `terminal-page-pane`).
- Bun: `terminalHistoryLoader.test.ts`.

## Acceptance Tests

Mapping to PRD acceptance criteria (1–9). All Cypress component tests use a fake `GrpcStream`
+ `historyFetcher` double (or RPC intercepts for the wiring test) — never `cy.intercept` for the
history flow. Fluent-tests style (Given/When/Then, driver helpers, one behavior per test).

1. `GhosttyTerminalGrpcLazyHistory.cy.tsx` — "shows the load-earlier-history affordance when the
   initial frame carries endOffset and atOldest is false" (live pane foreground).
2. … — "hides the affordance when the initial frame reports atOldest".
3. … — "shows the loading indicator and fetches forward from offset 0 when the affordance is
   activated".
4. … — "appends older bytes to the background page terminal and chains the next forward chunk".
5. … — "swaps the page terminal to the foreground once a chunk reports atEnd (reached the
   anchor)".
6. … — "keeps live bytes flowing to the live terminal during the forward fill (no reset, no
   interruption)".
7. … — "triggers the forward fill on a scroll-up-at-top gesture on the live pane".
8. … — "swaps back to the live pane on the Back-to-live affordance, then re-views history
   instantly".
9. … — "swaps back to live on a scroll-down-at-bottom gesture on the page pane" + "swaps to the
   page pane instantly on a scroll-up gesture once history is already filled".
- `GrpcSessionTerminalLazyHistory.cy.tsx` — "forwards a forward GetTerminalHistory
   (from_offset=0, until_offset=anchor) when the user loads earlier output" (the
   `onRegisterLoadOlderHistory` prop is gone; the runtime does not participate).

Bun unit tests (`terminalHistoryLoader.test.ts`):
- forward loader: done-when-anchor-zero / atOldest; fetches forward advancing `fromOffset`;
  stops at `atEnd`; null/empty chunk terminates.

## Technical Debt & Production Readiness

- The page terminal is filled with the entire retained capture (`0 → anchor`), which transfers
  all bytes even though the terminal retains only the last `scrollback` lines. A future
  optimization can page the forward-fill to fill the scrollback budget only.
- The scroll-up-on-live gesture detection relies on `getViewportY()` / `isPinnedToBottom()`;
  with `scrollback: 0` the live terminal is always pinned, so the gesture fires on any wheel-up.

## Decisions & Trade-offs

See PRD § Decisions & trade-offs.

## References

- [PRD](../../docs/ft/web/terminal-replay-lazy-scroll.md)
- [terminal-sessions.md (daemon)](../../docs/daemon/terminal-sessions.md#lazy-replay--scroll-up-history-added-2026-07-28)
- `packages/tddy-web/src/lib/terminalHistoryLoader.ts`
- `packages/tddy-web/src/components/GhosttyTerminal.tsx`
- `packages/tddy-web/src/components/GhosttyTerminalGrpc.tsx`
- `packages/tddy-web/src/components/sessions/GrpcSessionTerminal.tsx`
