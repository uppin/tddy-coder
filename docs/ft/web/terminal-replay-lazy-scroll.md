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
reaches earlier output by **scrolling up**: the Ghostty shared terminal component reveals a
second, read-only **older-history** ghostty-web terminal above the live one and **progressively
fills it forward** from offset `0` toward the anchor via `GetTerminalHistory`, appending one
chunk at a time until the anchor is reached.

This is a **progressive, append-only** fill — no resets, no prepend. The live terminal stays at
`scrollback: 0` (preserving the no-duplicate-pane fix for periodic TUI re-paints); the older
terminal carries `scrollback > 0` and accumulates older output forward, so the user scrolls
through the retained capture with the terminal's own viewport.

The `GrpcSessionTerminal → onRegisterLoadOlderHistory` indirection is removed; the Ghostty shared
component owns the scroll-up flow end-to-end.

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
**forward** to a **separate** older-history terminal (which is read-only and never receives the
live TUI re-paints that cause duplicates), while the live terminal remains untouched at
`scrollback: 0`.

## Scope

### In scope

- `GhosttyTerminal` gains a `scrollback` prop (default `0`) and imperative handle methods
  `scrollToTop()` and `isPinnedToBottom()` so the gRPC wrapper can detect the scroll-up-at-top
  gesture without reaching into ghostty-web internals. A `testId` prop lets the two stacked
  instances be targeted separately in tests.
- `GhosttyTerminalGrpc` (the shared component) **owns the scroll-up history flow**: it renders
  **two stacked ghostty-web terminals** — a live terminal (`scrollback: 0`) and an
  initially-hidden older-history terminal (`scrollback > 0`, read-only). It captures the
  absolute `endOffset` anchor from the initial `StreamTerminalOutput` replay frame, detects the
  user's request for earlier output, and **progressively fills the older terminal forward**
  from offset `0` toward the anchor via a `historyFetcher` prop (one `GetTerminalHistory`-shaped
  call per chunk, advancing `fromOffset` to each chunk's `endOffset`, until a chunk arrives with
  `atEnd = true`). No resets; the live terminal keeps appending live bytes throughout.
- A **"Load earlier output" affordance** (`data-testid="load-earlier-history"`) renders while
  older history is available (`!done` and `!filling`). Activating it reveals the older terminal
  and starts the forward fill. The affordance hides once the fill is in flight or complete.
- A **scroll-up-on-live gesture** (wheel up while the live terminal is pinned to the bottom)
  also starts the forward fill, so desktop users get the natural scroll-up without clicking the
  affordance. The wheel listener is attached in the **capture phase** so it fires before
  ghostty-web's own wheel handler (which may stop propagation).
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
  bug). The dual-terminal stack is the chosen permanent solution.

## User stories

### Story 1 — Reconnect and see the latest, then scroll up to earlier output

**Given** a session that has produced a large amount of output before I opened it,
**when** I open the terminal,
**then** I see the current last screen immediately (no wait for a full replay),
**and** a subtle "Load earlier output" affordance is visible at the top edge,
**and when** I activate it (or scroll up with the wheel),
**then** an older-history terminal appears above the live one and progressively fills forward
with older output, oldest first,
**and** I can scroll natively through the retained capture in the older terminal while live
output keeps flowing into the live terminal below.

### Story 2 — Reach the bottom of retained history

**Given** I have started loading earlier output,
**when** a forward chunk arrives carrying `atEnd = true` (reached the anchor),
**then** the "Load earlier output" affordance disappears,
**and** further scroll-up gestures do nothing (there is no older history).

### Story 3 — Live output is not lost during the fill

**Given** the older terminal is mid-fill (a forward fetch is in flight),
**when** new live output arrives on the stream,
**then** it is written to the live terminal immediately (no buffering, no reset),
**and** the older terminal keeps appending older chunks as they resolve — the two streams are
independent.

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
  older-history terminal passes a large value (e.g. `50000`) so the full retained capture fits.
- New `testId` prop (default `"ghostty-terminal"`); the older terminal passes
  `"ghostty-terminal-older"` so tests can target each instance.
- Imperative handle gains `scrollToTop()` and `isPinnedToBottom()` (viewport at the bottom) so
  the gRPC wrapper can detect the scroll-up-on-live gesture.

### `GhosttyTerminalGrpc`

- New prop: `historyFetcher?: (fromOffset: bigint, untilOffset: bigint) =>
  Promise<HistoryChunk | null>` — built by `GrpcSessionTerminal` from
  `createForwardHistoryFetcher(client, …)`. When absent, no lazy history (the component behaves
  as today, minus the indirection, and no older terminal is rendered).
- The `GrpcStream.onMessage` payload type is widened from `Uint8Array` to the full
  `SessionTerminalOutput`-shaped frame `{ data: Uint8Array; endOffset: bigint; atOldest: boolean }`
  so the anchor reaches the shared component. (No backwards compatibility —
  `GrpcSessionTerminal` is the only producer and is updated in lockstep.)
- On the first frame with `endOffset > 0`, the component captures the anchor (in state, so the
  affordance re-renders immediately) and constructs an internal `TerminalHistoryForwardLoader`.
- Renders a second, initially-hidden older-history terminal (`scrollback > 0`, `testId =
  "ghostty-terminal-older"`, `preventFocusOnTap`) above the live one. It is revealed when the
  fill starts.
- Forward-fill algorithm (triggered by the affordance or a scroll-up-on-live gesture):
  1. If `filling` or `fillDone`, no-op.
  2. Reveal the older terminal; set `filling`.
  3. While `!loader.done`: `await loader.loadNext(fetcher)`; if the chunk is non-null and
     non-empty, write it to the older terminal (or buffer it until the older terminal is ready).
  4. Clear `filling`; set `fillDone` from `loader.done`.
  5. Live bytes continue flowing to the live terminal throughout — no buffering, no reset.
- The "Load earlier output" affordance renders while `historyFetcher` is provided, the anchor
  is captured and not `atOldest`, and `!filling && !fillDone`.
- The scroll-up-on-live wheel listener is a capture-phase native listener on the live container
  (so it fires before ghostty-web's own wheel handler).

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
   the `load-earlier-history` affordance becomes visible.
2. **Affordance absent when no older history.** First frame with `atOldest = true` (or
   `endOffset = 0`): no `load-earlier-history` affordance.
3. **Activating the affordance issues a forward `GetTerminalHistory` from offset 0 bounded by
   the anchor.** The first `historyFetcher` call receives `fromOffset === 0n` and
   `untilOffset === <initial endOffset>`.
4. **Older bytes are appended to the older-history terminal.** After the first chunk resolves,
   the older terminal buffer contains the chunk's text; a second fetch is issued forward from
   the chunk's `endOffset`.
5. **`atEnd` terminates the fill.** When a chunk carries `atEnd = true`, no further fetch is
   issued and the older terminal holds both chunks in order; the affordance is gone.
6. **Live bytes keep flowing during the fill.** Output pushed on the stream while a forward
   fetch is in flight is present in the **live** terminal buffer (and the older terminal
   independently holds the older chunk) — no reset, no loss.
7. **Scroll-up-on-live gesture triggers the fill** when the live terminal is pinned to the
   bottom and older history is available.
8. **`GrpcSessionTerminal` no longer exposes `onRegisterLoadOlderHistory`.** The prop is gone;
   the runtime does not participate in history loading.

## Decisions & trade-offs

- **Two stacked terminals over a single rebuilt terminal** — chosen because ghostty-web has no
  prepend API and resetting the *live* terminal would reintroduce the duplicate-pane bug. Cost:
  a visual seam between the older and live terminals (a border). Benefit: the live terminal
  stays at `scrollback: 0` (no duplicates), older bytes are appended forward (no reset flash),
  and the two streams are fully independent.
- **Progressive forward fill over one-shot reset+replay** — append-only, no resets. The user
  sees older output stream in as chunks resolve, and live output never pauses. The fill
  terminates naturally at the anchor (`atEnd`).
- **`historyFetcher` callback over `Client` injection** — keeps `GhosttyTerminalGrpc` decoupled
  from `ConnectionService` and unit-testable with a plain function double.
- **Affordance + gesture** — the affordance is the deterministic, testable trigger; the
  scroll-up-on-live gesture is the natural desktop path. Both start the same forward fill.
- **Capture-phase wheel listener** — ghostty-web's own wheel handler may stop propagation; a
  bubble-phase React `onWheel` would never see the event. The capture-phase listener fires
  first and reliably detects the scroll-up-at-top intent.

## Future scope

- LiveKit transport: carry offset metadata on the LiveKit terminal frames so
  `GhosttyTerminalLiveKit` can use the same flow.
- Persisted scroll position across reconnects.
- A unified single-terminal surface if a future ghostty-web release adds a prepend/insert-at-top
  API that does not require a live-terminal reset.
