# Terminal Replay — Lazy Scroll-up History (Viewport Integration)

> **Status:** Amendment — adds the viewport integration that consumes the
> `GetTerminalHistory` RPC shipped in the *Terminal Replay & PTY-over-RPC Unification*
> changeset. The backend, proto, and client primitive (`TerminalHistoryForwardLoader`) already
> exist; this doc defines the user-facing scroll-up behavior and the Ghostty shared
> component ownership model.
>
> **Related docs:**
> - [web-terminal.md](web-terminal.md) — the Ghostty terminal surface this builds on.
> - [enqueued-input-overlay.md](enqueued-input-overlay.md) — the other `StreamTerminalOutput`
>   consumer (ACK frames); unchanged by this work.
> - [terminal-sessions.md (daemon)](../daemon/terminal-sessions.md#lazy-replay--scroll-up-history-added-2026-07-28)
>   — the wire contract this consumes.

## Summary

When a user opens (or reconnects to) a terminal session, the daemon sends the **current last
frame first** and then tails live output. Older output is **not** replayed eagerly. The user
reaches earlier output by **scrolling up**: the Ghostty shared terminal component **overlays a
second, read-only older-history ghostty-web terminal behind the live one** and **progressively
fills it forward** from offset `0` toward the anchor via `GetTerminalHistory`, appending one
chunk at a time until the anchor is reached. While the fill runs, a **loading indicator** is
shown over the foreground terminal; once the fill completes, the two terminals **switch places**
— the older-history terminal becomes the foreground (scrollable through the retained capture),
and the live terminal stays mounted underneath, still receiving the stream.

This is a **progressive, append-only** fill — no resets, no prepend. The live terminal stays at
`scrollback: 0` (preserving the no-duplicate-pane fix for periodic TUI re-paints); the older
"page" terminal carries `scrollback > 0` and accumulates older output forward, so the user
scrolls through the retained capture with the terminal's own viewport. "Back to live" (or a
scroll-down-at-bottom gesture on the page terminal) swaps back to the live terminal, which has
stayed current underneath.

The `GrpcSessionTerminal → onRegisterLoadOlderHistory` indirection is removed; the Ghostty shared
component owns the scroll-up flow end-to-end, including the double-buffer paging.

## Background

`GhosttyTerminal` (the shared renderer) is constructed with `scrollback: 0` today, so there is
no native scrollback to scroll through. This is deliberate: a non-zero scrollback reintroduces a
duplicate-pane bug — periodic full-screen status-bar re-renders accumulate as duplicate panes in
the scrollback. The lazy-replay changeset added the `GetTerminalHistory` RPC and a client
primitive but left the viewport integration undone.

ghostty-web exposes a native `scrollback` option plus viewport APIs (`getViewportY`,
`scrollLines`, `scrollToTop`, `scrollToBottom`, `buffer.active.length`), but it has **no
"insert at top of scrollback" / prepend API** — `write()` only appends and `reset()` clears the
whole buffer. A reset+replay of the *live* terminal would also reintroduce the duplicate-pane
bug (the live terminal must stay at `scrollback: 0`).

The progressive forward-fill design sidesteps both constraints: older bytes are appended
**forward** to a **separate** older-history "page" terminal (which is read-only and never
receives the live TUI re-paints that cause duplicates), while the live terminal remains
untouched at `scrollback: 0`. The two terminals **overlay one another** in the same rect and
**switch places** (foreground/background) when the fill completes, so the user sees a single
seamless terminal surface rather than a split pane.

## Scope

### In scope

- `GhosttyTerminal` gains a `scrollback` prop (default `0`) and imperative handle methods
  `scrollToTop()`, `scrollToBottom()`, and `isPinnedToBottom()` so the gRPC wrapper can detect the
  scroll-up-at-top / scroll-down-at-bottom gestures without reaching into ghostty-web internals.
  A `testId` prop lets the two overlaid instances be targeted separately in tests.
- `GhosttyTerminalGrpc` (the shared component) **owns the scroll-up history flow**: it renders
  **two overlaid ghostty-web terminals** sharing one rect — a live terminal (`scrollback: 0`)
  and an older-history "page" terminal (`scrollback > 0`, read-only). It captures the absolute
  `endOffset` anchor from the initial `StreamTerminalOutput` replay frame, detects the user's
  request for earlier output, and **progressively fills the page terminal forward in the
  background** from offset `0` toward the anchor via a `historyFetcher` prop (one
  `GetTerminalHistory`-shaped call per chunk, advancing `fromOffset` to each chunk's `endOffset`,
  until a chunk arrives with `atEnd = true`). A **loading indicator** is shown while filling; once
  the fill completes the page terminal **swaps to the foreground** (landed at its bottom = the
  newest pre-anchor line, seamless). No resets; the live terminal keeps appending live bytes
  throughout and stays mounted underneath so swapping back is instant and current.
- A **"Load earlier output" affordance** (`data-testid="load-earlier-history"`) renders on the
  live pane while older history is available and not yet filled. Activating it starts the
  background fill. After the first fill, the live pane shows a **"View history"** affordance
  (`data-testid="view-history"`) instead, which swaps to the page pane instantly (no re-fetch).
  The page pane shows a **"Back to live"** affordance (`data-testid="back-to-live"`).
- A **scroll-up-on-live gesture** (wheel up while the live terminal is pinned to the bottom)
  starts the forward fill on first activation, or swaps to the page pane instantly if history is
  already filled. A **scroll-down-at-bottom gesture** on the page pane swaps back to live. Both
  wheel listeners are attached in the **capture phase** so they fire before ghostty-web's own wheel
  handler (which may stop propagation).
- `GrpcSessionTerminal` builds the `historyFetcher` from its `Client<ConnectionService>` +
  session ids and passes it (plus the full `SessionTerminalOutput` frames carrying the offset
  metadata) down to `GhosttyTerminalGrpc`. The `onRegisterLoadOlderHistory` prop is **removed**.

### Out of scope

- The LiveKit-backed `GhosttyTerminalLiveKit` path — the LiveKit transport does not yet carry
  the offset metadata. LiveKit lazy history is a separate follow-up. (The shared
  `GhosttyTerminal` `scrollback`/handle changes benefit both transports; only the gRPC arm wires
  the fetcher.)
- Persisting scroll position across session reconnects.
- A unified single-terminal surface (concatenating older + live into one scrollback) — would
  require either a prepend API or resetting the live terminal (reintroducing the duplicate-pane
  bug). The overlaid double-buffer is the chosen permanent solution.
- Paged forward-fill: today the page terminal is filled with the entire retained capture
  (`0 → anchor`), which transfers all bytes even though the terminal retains only the last
  `scrollback` lines. A future optimization can page the forward-fill to fill the scrollback
  budget only (skipping bytes that would be discarded).

## User stories

### Story 1 — Reconnect and see the latest, then scroll up to earlier output

**Given** a session that has produced a large amount of output before I opened it,
**when** I open the terminal,
**then** I see the current last screen immediately (no wait for a full replay),
**and** a subtle "Load earlier output" affordance is visible at the top edge,
**and when** I activate it (or scroll up with the wheel),
**then** a loading indicator appears while the older-history page terminal is forward-filled in
the background,
**and when** the fill completes, the page terminal swaps to the foreground (landed at its bottom,
seamless above where the live tip was) and I can scroll natively through the retained capture,
**and** live output keeps flowing into the live terminal underneath the whole time.

### Story 2 — Reach the bottom of retained history

**Given** I have started loading earlier output,
**when** a forward chunk arrives carrying `atEnd = true` (reached the anchor),
**then** the loading indicator disappears and the page terminal is foreground,
**and** further scroll-up gestures just scroll within the page terminal's scrollback (there is no
older history to fetch).

### Story 3 — Live output is not lost during the fill

**Given** the page terminal is mid-fill (a forward fetch is in flight),
**when** new live output arrives on the stream,
**then** it is written to the live terminal immediately (no buffering, no reset),
**and** the page terminal keeps appending older chunks as they resolve — the two streams are
independent.

### Story 4 — Return to live, then re-view history instantly

**Given** I am viewing the older-history page terminal,
**when** I activate "Back to live" (or scroll down at the bottom of the page terminal),
**then** the live terminal swaps back to the foreground (it has stayed current underneath),
**and** a "View history" affordance appears; activating it (or scrolling up again) swaps back to
the page terminal instantly, with no new fetch.

## Wire contract (recap)

Consumed from the unification changeset (forward-chunk shape):

- `SessionTerminalOutput` initial replay frame carries `end_offset` (the anchor) and
  `at_oldest`. Live tail frames carry `0` for the offsets.
- `GetTerminalHistory(session_token, session_id, terminal_id, from_offset, until_offset,
  max_bytes)` — server-streaming; yields one
  `TerminalHistoryChunk{data, start_offset, end_offset, at_oldest, at_end}` starting at
  `from_offset`, then closes. The client appends it to the older terminal, advances
  `from_offset` to the chunk's `end_offset`, and calls again until `at_end = true` (reached
  `until_offset` / the capture tip).

## Component contract

### `GhosttyTerminal`

- New `scrollback` prop (default `0`). The live terminal passes `0` (no duplicates); the
  older-history page terminal passes a large value (e.g. `50000`) so the full retained capture fits.
- New `testId` prop (default `"ghostty-terminal"`); the page terminal passes
  `"ghostty-terminal-older"` so tests can target each instance.
- Imperative handle gains `scrollToTop()`, `scrollToBottom()`, and `isPinnedToBottom()` (viewport
  at the bottom) so the gRPC wrapper can detect the scroll-up-on-live / scroll-down-on-page
  gestures and land the page terminal at its bottom after a swap.

### `GhosttyTerminalGrpc`

- New prop: `historyFetcher?: (fromOffset: bigint, untilOffset: bigint) =>
  Promise<HistoryChunk | null>` — built by `GrpcSessionTerminal` from
  `createForwardHistoryFetcher(client, …)`. When absent, no lazy history (the component behaves
  as today, minus the indirection, and no page terminal is rendered).
- The `GrpcStream.onMessage` payload type is widened from `Uint8Array` to the full
  `SessionTerminalOutput`-shaped frame `{ data: Uint8Array; endOffset: bigint; atOldest: boolean }`
  so the anchor reaches the shared component. (No backwards compatibility —
  `GrpcSessionTerminal` is the only producer and is updated in lockstep.)
- On the first frame with `endOffset > 0`, the component captures the anchor (in state, so the
  affordance re-renders immediately) and constructs an internal `TerminalHistoryForwardLoader`.
- Renders a second, **hidden** older-history page terminal (`scrollback > 0`, `testId =
  "ghostty-terminal-older"`, `preventFocusOnTap`) **overlaid behind** the live one in the same
  rect. Both terminals are always mounted (so the page terminal can be written to while hidden and
  the live terminal stays current while hidden). Foreground/background is driven by a `view` state
  (`"live" | "page"`) controlling `visibility`/`z-index`/`pointer-events`; each pane exposes
  `data-foreground` for test assertions.
- Forward-fill algorithm (triggered by the "Load earlier output" affordance or a scroll-up-on-live
  gesture when history is not yet filled):
  1. If `filling` or `filled`, no-op.
  2. Set `loading` (show the loading indicator over the foreground pane).
  3. While `!loader.done`: `await loader.loadNext(fetcher)`; if the chunk is non-null and
     non-empty, write it to the page terminal (or buffer it until the page terminal is ready).
  4. Clear `loading`; set `filled`; set `view = "page"`; on the next frame, `scrollToBottom()` the
     page terminal so the user lands on the newest pre-anchor line (seamless).
  5. Live bytes continue flowing to the live terminal throughout — no buffering, no reset.
- "Load earlier output" renders while `historyFetcher` is provided, the anchor is captured and not
  `atOldest`, and `!filled && !loading`. "View history" renders on the live pane once `filled`.
  "Back to live" renders on the page pane.
- Scroll-up-on-live wheel listener (capture phase): if `filled`, swap to the page pane instantly;
  else if pinned to bottom, start the forward fill. Scroll-down-at-bottom wheel listener on the
  page pane (capture phase): swap back to the live pane.

### `GrpcSessionTerminal`

- Builds `historyFetcher = createForwardHistoryFetcher(client, { sessionToken, sessionId,
  terminalId })` (memoized) and passes it to `GhosttyTerminalGrpc`.
- Forwards the full `SessionTerminalOutput` frame (data + offsets) through the `GrpcStream`
  instead of just `output.data`.
- Removes the `onRegisterLoadOlderHistory` prop and the `historyLoaderRef` ownership (now lives
  in `GhosttyTerminalGrpc`). ACK handling (`ackedInputOffset`) stays in `GrpcSessionTerminal`.

## Acceptance criteria

1. **Affordance appears when older history exists.** Mount `GhosttyTerminalGrpc` with a
   `historyFetcher` and a stream whose first frame carries `endOffset > 0` and `atOldest = false`:
   the `load-earlier-history` affordance becomes visible and the live pane is foreground.
2. **Affordance absent when no older history.** First frame with `atOldest = true` (or
   `endOffset = 0`): no `load-earlier-history` affordance.
3. **Activating the affordance shows the loading indicator and issues a forward
   `GetTerminalHistory` from offset 0 bounded by the anchor.** The loading indicator is visible
   and the first `historyFetcher` call receives `fromOffset === 0n` and
   `untilOffset === <initial endOffset>`.
4. **Older bytes are appended to the background page terminal.** After the first chunk resolves,
   the page terminal buffer contains the chunk's text; a second fetch is issued forward from the
   chunk's `endOffset`.
5. **`atEnd` swaps the page terminal to the foreground.** When a chunk carries `atEnd = true`, no
   further fetch is issued, the loading indicator is gone, the page terminal holds both chunks in
   order, the page pane is foreground (live pane hidden underneath), and "Back to live" is
   visible.
6. **Live bytes keep flowing during the fill.** Output pushed on the stream while a forward
   fetch is in flight is present in the **live** terminal buffer (and the page terminal
   independently holds the older chunk) — no reset, no loss.
7. **Scroll-up-on-live gesture triggers the fill** when the live terminal is pinned to the
   bottom and history is not yet filled; once filled, the same gesture swaps to the page pane
   instantly (no new fetch).
8. **Back to live + re-view history is instant.** "Back to live" swaps to the live pane (which
   has stayed current); "View history" (or a scroll-up gesture) swaps back to the page pane with
   no new fetch. A scroll-down-at-bottom gesture on the page pane also swaps back to live.
9. **`GrpcSessionTerminal` no longer exposes `onRegisterLoadOlderHistory`.** The prop is gone;
   the runtime does not participate in history loading.

## Decisions & trade-offs

- **Two overlaid, interchangeable terminals over a split pane or a single rebuilt terminal** —
  chosen because ghostty-web has no prepend API and resetting the *live* terminal would reintroduce
  the duplicate-pane bug. The two terminals share one rect and switch foreground/background, so the
  user sees a single seamless surface (no split-pane seam). Cost: a brief loading indicator while
  the background page terminal is populated before the swap. Benefit: the live terminal stays at
  `scrollback: 0` (no duplicates), older bytes are appended forward (no reset flash), the two
  streams are fully independent, and swapping back to live is instant and current.
- **Progressive forward fill over one-shot reset+replay** — append-only, no resets. The user
  sees older output stream in as chunks resolve, and live output never pauses. The fill
  terminates naturally at the anchor (`atEnd`).
- **`historyFetcher` callback over `Client` injection** — keeps `GhosttyTerminalGrpc` decoupled
  from `ConnectionService` and unit-testable with a plain function double.
- **Affordance + gesture** — the affordances are the deterministic, testable triggers; the
  scroll-up/scroll-down gestures are the natural desktop paths. Both drive the same swap/fill
  logic.
- **Capture-phase wheel listeners** — ghostty-web's own wheel handler may stop propagation; a
  bubble-phase React `onWheel` would never see the event. The capture-phase listeners fire first
  and reliably detect the scroll-up-at-top / scroll-down-at-bottom intent on each pane.
- **Live terminal always mounted & streaming** — the live terminal is never unmounted or reset, so
  returning to the live tip is always instant and current, even after browsing history for a while.

## Future scope

- LiveKit transport: carry offset metadata on the LiveKit terminal frames so
  `GhosttyTerminalLiveKit` can use the same flow.
- Persisted scroll position across reconnects.
- A unified single-terminal surface if a future ghostty-web release adds a prepend/insert-at-top
  API that does not require a live-terminal reset.
