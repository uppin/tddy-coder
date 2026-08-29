# Terminal Replay — Lazy Scroll-up History (Viewport Integration)

> **Status:** Amendment — adds the viewport integration that consumes the
> `GetTerminalHistory` RPC shipped in the *Terminal Replay & PTY-over-RPC Unification*
> changeset. The backend, proto, and client primitive (`TerminalHistoryForwardLoader`) already
> exist; this doc defines the user-facing scroll-up behavior and the Ghostty shared
> component ownership model.
>
> **Live terminal scrollback policy:** the live terminal stays at `scrollback: 0` (the default).
> It is always pinned to the live tip — there is no native live scrollback to scroll through, so
> the first wheel-up (when the TUI is not mouse-tracking) loads the older-history page terminal
> immediately. `scrollback: 0` is the deliberate duplicate-pane mitigation: with no live
> scrollback, neither primary-screen nor alternate-screen (DEC 1049) repaints accumulate, so a
> TUI can never leave duplicate panes behind in the live terminal. The page terminal exposes a
> native `Scrollbar { total, offset, len }` as the single source of truth for viewport position
> (same coordinate space as `scrollToLine`). Mouse tracking (DEC 1006) gates the wheel to the TUI
> instead of triggering the forward-fill.
>
> **Reconnect resume by offset:** `StreamTerminalOutput` gains a `StreamReplayMode`
> (`TAIL` default / `FROM_OFFSET`). A reconnecting terminal resumes by offset instead of
> re-replaying the whole retained buffer (no duplicate content), and a transient transport blip
> (null client) no longer evicts the runtime — the terminal stays mounted and resumes with
> `FROM_OFFSET` when a non-null client returns.
>
> **Status history:** the earlier "terminal-native-scrolling" amendment raised the live terminal
> to `scrollback > 0`; that was reverted (it reintroduced duplicate-pane accumulation and
> surfaced a blank-page-after-reconnect bug). The live terminal stays at `scrollback: 0`; the
> native `Scrollbar` and mouse-tracking gating from that amendment are retained.
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
fills it forward** from offset `0` toward the **current live tip** via `GetTerminalHistory`,
appending one chunk at a time until the tip is reached. (The fill is bounded by the *current* tip,
not the stale anchor captured at open time, so a capture ring that has since evicted the original
anchor still returns its retained `[start_offset, tip]` range rather than an empty one.) While the
fill runs, a **loading indicator** is shown over the foreground terminal; once the fill completes,
the two terminals **switch places** — the older-history terminal becomes the foreground
(scrollable through the retained capture), and the live terminal stays mounted underneath, still
receiving the stream.

This is a **progressive, append-only** fill — no resets, no prepend. The overlay is split
across two ghostty-web instances only because ghostty-web has no "insert at top of scrollback"
API (a single terminal cannot lazily prepend older history above already-written content). The
**live terminal is the active screen**: it carries `scrollback: 0` (the default) and is always
pinned to the live tip — there is no native live scrollback, so the duplicate-pane bug (periodic
full-screen status-bar re-renders accumulating as duplicate panes) cannot occur in the live
terminal at all. The **page terminal is the older PageList history view**: it carries a large
`scrollback` and accumulates the forward-filled pre-connect capture, so the user scrolls through
the retained history with the terminal's own viewport. The page terminal exposes a native
`Scrollbar { total, offset, len }` (the same coordinate space as `scrollToLine`) as the single
source of truth for its viewport position. "Back to live" (or a scroll-down-at-bottom gesture on
the page terminal) swaps back to the live terminal, which has stayed current underneath.

Because the live terminal has `scrollback: 0`, its viewport can never be scrolled up away from
the live tip, so the native scroll-to-bottom policy (`scroll-to-bottom.keystroke` /
`scroll-to-bottom.output`) is a no-op on the live terminal — there is nowhere to scroll back
from. When the TUI has enabled **mouse tracking** (DEC 1006), the wheel is reported to the TUI
(SGR button 64/65) and does **not** trigger the forward-fill — matching native ghostty's
`isMouseReporting` gate.

The `GrpcSessionTerminal → onRegisterLoadOlderHistory` indirection is removed; the Ghostty shared
component owns the scroll-up flow end-to-end, including the double-buffer paging.

## Background

`GhosttyTerminal` (the shared renderer) is constructed with `scrollback: 0` today, so there is
no native scrollback to scroll through. This is deliberate: a non-zero scrollback reintroduces a
duplicate-pane bug — periodic full-screen status-bar re-renders accumulate as duplicate panes in
the scrollback. The lazy-replay changeset added the `GetTerminalHistory` RPC and a client
primitive but left the viewport integration undone.

The live terminal therefore stays at `scrollback: 0`: it is always pinned to the live tip, and
the first wheel-up (when the TUI is not mouse-tracking) loads the older-history page terminal
immediately. There is no "scroll through post-connect live scrollback" step — the live terminal
retains nothing, so neither primary-screen nor alternate-screen (DEC 1049) repaints can
accumulate as duplicate panes. This is stricter than native ghostty (whose primary screen has
scrollback), but it is the chosen permanent mitigation for the duplicate-pane bug under
ghostty-web's no-prepend constraint.

ghostty-web exposes a native `scrollback` option plus viewport APIs (`getViewportY`,
`scrollLines`, `scrollToTop`, `scrollToBottom`, `scrollToLine`, `getScrollbackLength`,
`buffer.active.length`), but it has **no "insert at top of scrollback" / prepend API** —
`write()` only appends and `reset()` clears the whole buffer. A reset+replay of the *live*
terminal would also reintroduce the duplicate-pane bug if the replayed range includes primary-
screen TUI repaints.

The progressive forward-fill design sidesteps both constraints: older bytes are appended
**forward** to a **separate** older-history "page" terminal (which is read-only and never
receives the live TUI re-paints that cause duplicates), while the live terminal remains at
`scrollback: 0` and shows only the current screen. The two terminals **overlay one another**
in the same rect and **switch places** (foreground/background) when the fill completes, so the
user sees a single seamless terminal surface rather than a split pane.

## Scope

### In scope

- `GhosttyTerminal` gains a `scrollback` prop (default `0`) and imperative handle methods
  `scrollToTop()`, `scrollToBottom()`, and `isPinnedToBottom()` so the gRPC wrapper can detect the
  scroll-up-on-live / scroll-down-on-page gestures without reaching into ghostty-web internals.
  A `testId` prop lets the two overlaid instances be targeted separately in tests. The handle also
  exposes `scrollToLine(line)`, `scrollLines(amount)`, `getScrollbackLength()`, and a native
  `getScrollbar()` returning `{ total, offset, len }` (the single source of truth for viewport
  position, same coordinate space as `scrollToLine`): `total = getScrollbackLength() + rows`,
  `offset = max(0, getScrollbackLength() - getViewportY())`, `len = rows`.
- `GhosttyTerminalGrpc` (the shared component) **owns the scroll-up history flow**: it renders
  **two overlaid ghostty-web terminals** sharing one rect — a live terminal (`scrollback: 0`,
  the active screen, always pinned to the live tip) and an older-history "page" terminal
  (`scrollback > 0`, read-only). It captures the absolute `endOffset` anchor from the initial
  `StreamTerminalOutput` replay frame (its `atOldest` flag gates the affordance), detects the
  user's request for earlier output, and **progressively fills the page terminal forward in the
  background** from offset `0` toward the **current live tip** via a `historyFetcher` prop (one
  `GetTerminalHistory`-shaped call per chunk, advancing `fromOffset` to each chunk's `endOffset`,
  until a chunk arrives with `atEnd = true`). The upper bound is the *current* cumulative output
  offset at fill time, not the stale anchor — so after a reconnect (or any output that evicts the
  original anchor from the capture ring) the fill still returns the retained `[start_offset, tip]`
  range instead of an empty one. A **loading indicator** is shown while filling; once the fill
  completes (and only if it actually wrote history) the page terminal **swaps to the foreground**
  (landed at its bottom = the newest pre-tip line, seamless). A failed or empty fill stays on the
  live pane (no blank page). No resets; the live terminal keeps appending live bytes throughout
  and stays mounted underneath so swapping back is instant and current.
- **Live terminal scrollback policy:** the live terminal carries `scrollback: 0` (the default).
  It is always pinned to the live tip — there is no native live scrollback to scroll through.
  This is the deliberate duplicate-pane mitigation: with no live scrollback, neither primary-screen
  nor alternate-screen (DEC 1049) repaints accumulate, so a TUI can never leave duplicate panes
  behind in the live terminal.
- **Scroll-up-on-live gesture:** because the live terminal has `scrollback: 0` (always pinned to
  the bottom), any wheel-up while the TUI is not mouse-tracking triggers the page forward-fill
  immediately — there is no "scroll through post-connect live scrollback" step. Once the page
  terminal is filled, the same gesture swaps to it instantly.
- **Mouse-tracking gating:** when the TUI has enabled mouse tracking (DEC 1006), the wheel is
  reported to the TUI (SGR button 64/65) and does **not** trigger the forward-fill — matching
  native ghostty's `isMouseReporting` gate.
- **Touch (mobile) is gated the same three ways.** A one-finger drag decides at `touchstart`, per
  line of finger travel: a **mouse-tracking** TUI is sent an SGR wheel report (button 64/65) at the
  touch point; the **alternate screen** (DEC 1049) without tracking is sent the arrow key
  ghostty-web emulates the wheel with; only the **normal screen** scrolls the pane's own
  scrollback. Neither TUI route reaches the forward-fill, so a full-screen TUI such as the Claude
  CLI scrolls its own content on mobile exactly as it does on desktop, rather than the drag being
  spent on the live pane's (empty) scrollback.
- A **"Load earlier output" affordance** (`data-testid="load-earlier-history"`) renders on the
  live pane while older history is available and not yet filled. Activating it starts the
  background fill. After the first fill, the live pane shows a **"View history"** affordance
  (`data-testid="view-history"`) instead, which swaps to the page pane instantly (no re-fetch).
  The page pane shows a **"Back to live"** affordance (`data-testid="back-to-live"`).
- A **scroll-down-at-bottom gesture** on the page pane swaps back to live. Both wheel listeners
  are attached in the **capture phase** so they fire before ghostty-web's own wheel handler
  (which may stop propagation).
- `GrpcSessionTerminal` builds the `historyFetcher` from its `Client<ConnectionService>` +
  session ids and passes it (plus the full `SessionTerminalOutput` frames carrying the offset
  metadata) down to `GhosttyTerminalGrpc`. The `onRegisterLoadOlderHistory` prop is **removed**.
- **Reconnect resume by offset (`StreamReplayMode`):** `StreamTerminalOutputRequest` (and the bidi
  `StreamSessionTerminalIO` open frame) gain a `mode` (`StreamReplayMode`) + `from_offset`.
  `TAIL` (default, first connect) sends the mode prologue + current last-frame tail chunk, resizes
  the PTY to the client's dimensions, drains the pre-resize broadcast, then bridges live output.
  `FROM_OFFSET` (reconnect) sends the mode prologue + chunked catch-up via `replay_from(from_offset,
  tip, …)` until `at_end`, then live output — no tail chunk, no PTY resize/drain — so a terminal
  that already holds state up to `from_offset` receives only the bytes it missed, with no duplicate
  replay. `GrpcSessionTerminal.client` widens to `ConnectionClient | null`: a **null client** (a
  transient transport blip) **pauses** the terminal — it stays mounted (its scrollback and the
  ghostty instance survive), input is queued — and resumes with `FROM_OFFSET` when a non-null client
  returns. Only a stream-end with a **valid** client (a real `pty_done`) evicts the runtime. The
  client tracks the cumulative output offset and sends `FROM_OFFSET` with the tracked offset on
  reconnect.

### Out of scope

- The LiveKit-backed `GhosttyTerminalLiveKit` path — the LiveKit transport does not yet carry
  the offset metadata. LiveKit lazy history is a separate follow-up. (The shared
  `GhosttyTerminal` `scrollback`/handle changes benefit both transports; only the gRPC arm wires
  the fetcher.)
- Persisting scroll position across session reconnects.
- A unified single-terminal surface (concatenating older + live into one scrollback) — would
  require either a prepend API or resetting the live terminal. The overlay double-buffer is the
  chosen permanent solution under ghostty-web's no-prepend constraint; a daemon-side PageList
  emulator is a future option if a prepend API never lands.
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

### Story 5 — A transport blip does not evict the terminal, and a reconnect does not re-replay

**Given** I am viewing a live terminal that has already synced its state up to offset `N`,
**when** the transport blips (the daemon client goes `null`) and then returns,
**then** the terminal stays mounted throughout (its scrollback and the ghostty instance survive;
input is queued and flushed on reconnect),
**and** the reconnect opens the stream with `mode = FROM_OFFSET` and `from_offset = N`,
**and** the server sends only the catch-up bytes from `N` to the current tip (no tail chunk, no PTY
resize/drain) then live output — so the live terminal receives only the bytes it missed, with no
duplicate content,
**and** only a real stream-end with a valid client (a `pty_done`) evicts the runtime.

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

- New `scrollback` prop (default `0`). The live terminal passes the default `0` — always pinned
  to the live tip, no native scrollback, so neither primary-screen nor alternate-screen (DEC 1049)
  repaints accumulate as duplicate panes. The older-history page terminal passes a large value
  (e.g. `50000`) so the full retained capture fits.
- New `testId` prop (default `"ghostty-terminal"`); the page terminal passes
  `"ghostty-terminal-older"` so tests can target each instance.
- Imperative handle gains `scrollToTop()`, `scrollToBottom()`, `isPinnedToBottom()` (viewport
  at the bottom), `scrollToLine(line)`, `scrollLines(amount)`, `getScrollbackLength()`, and a
  native `getScrollbar()` returning `{ total, offset, len }` — the single source of truth for
  viewport position, in the same coordinate space as `scrollToLine` (`total = scrollbackLength +
  rows`, `offset = max(0, scrollbackLength - getViewportY())`, `len = rows`). The gRPC wrapper
  uses these to detect the scroll-up-on-live / scroll-down-on-page gestures, land the page
  terminal at its bottom after a swap, and expose the viewport position to tests/scrollbar UI.
- **Mouse-tracking gating:** the wheel handler checks `hasMouseTracking()`; when the TUI has
  tracking on, the wheel is reported to the TUI (SGR button 64/65) and does not trigger the
  forward-fill. (The native scroll-to-bottom policy is a no-op on the live terminal because
  `scrollback: 0` means the viewport can never be scrolled up away from the tip.)

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
  affordance re-renders immediately). The anchor's `atOldest` gates the affordance; its `endOffset`
  is NOT used as the fill upper bound (it is stale after a reconnect/eviction). The
  `TerminalHistoryForwardLoader` is constructed lazily on the first fill, bounded by the current
  cumulative output offset (the live tip) at that moment.
- Renders a second, **hidden** older-history page terminal (`scrollback > 0`, `testId =
  "ghostty-terminal-older"`, `preventFocusOnTap`) **overlaid behind** the live one in the same
  rect. Both terminals are always mounted (so the page terminal can be written to while hidden and
  the live terminal stays current while hidden). Foreground/background is driven by a `view` state
  (`"live" | "page"`) controlling `visibility`/`z-index`/`pointer-events`; each pane exposes
  `data-foreground` for test assertions.
- **Live terminal `scrollback: 0`** (the default). The live terminal is always pinned to the live
  tip — there is no native live scrollback to scroll through, so neither primary-screen nor
  alternate-screen (DEC 1049) repaints can accumulate as duplicate panes.
- **Scroll-up-on-live gesture:** because the live terminal has `scrollback: 0` (always pinned to
  the bottom), any wheel-up while the TUI is not mouse-tracking triggers the page forward-fill
  immediately — there is no "scroll through post-connect live scrollback" step. Once the page
  terminal is filled, the same gesture swaps to it instantly.
- Forward-fill algorithm (triggered by the "Load earlier output" affordance or a scroll-up-at-top
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
- Scroll-down-at-bottom wheel listener on the page pane (capture phase): swap back to the live pane.
- **Page terminal Scrollbar mirror:** the page terminal's `getScrollbar()` is mirrored to a hidden
  `data-testid="terminal-page-scrollbar"` element (`{total,offset,len}`) so component tests can
  assert the native viewport position as the single source of truth.

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
   `GetTerminalHistory` from offset 0 bounded by the current live tip.** The loading indicator is
   visible and the first `historyFetcher` call receives `fromOffset === 0n` and
   `untilOffset === <current cumulative output offset at fill time>` (≥ the initial `endOffset`,
   never the stale anchor after a reconnect/eviction).
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
10. **No blank page on a stale/empty/failed fill.** After a reconnect that evicts the original
    anchor (or any fill whose fetch returns no bytes or errors), the page terminal does NOT swap
    to a blank foreground; the live pane stays foreground, the loading indicator clears, and the
    affordance remains available for a later retry.
11. **Reconnect resumes by offset, not by re-replay.** After the initial sync, a reconnect opens
    `StreamTerminalOutput` with `mode = FROM_OFFSET` and `from_offset` = the tracked cumulative
    output offset; the server sends only the catch-up bytes from `from_offset` to the current tip
    (no tail chunk, no PTY resize/drain) then live output. The live terminal receives no duplicate
    content.
12. **A transient transport blip does not evict the runtime.** While the daemon client is `null`
    (transport blip), the terminal stays mounted, input is queued, and a stream-end does NOT evict.
    When a non-null client returns, the stream resumes with `FROM_OFFSET`. Only a stream-end with a
    valid client (a real `pty_done`) evicts the runtime.

## Decisions & trade-offs

- **Live `scrollback: 0` over native per-screen scrollback** — the live terminal carries
  `scrollback: 0` (always pinned to the live tip). This is the deliberate duplicate-pane
  mitigation: with no live scrollback, neither primary-screen nor alternate-screen (DEC 1049)
  repaints can accumulate, so a TUI can never leave duplicate panes behind in the live terminal.
  This is stricter than native ghostty (whose primary screen has scrollback), but it is the
  chosen permanent mitigation under ghostty-web's no-prepend constraint. The cost is that the
  live terminal retains no post-connect history of its own — but the older-history page terminal
  (forward-filled from offset `0` toward the anchor) covers all pre-connect history, and the
  first wheel-up loads it immediately.
- **Two overlaid, interchangeable terminals over a split pane or a single rebuilt terminal** —
  chosen because ghostty-web has no prepend API and resetting the *live* terminal would reintroduce
  the duplicate-pane bug. The two terminals share one rect and switch foreground/background, so the
  user sees a single seamless surface (no split-pane seam). The overlay is split across two
  instances (live = active screen at `scrollback: 0`, page = older PageList history view at
  `scrollback > 0`) — a single-terminal surface is infeasible under ghostty-web's no-prepend
  constraint. Cost: a brief loading indicator while the background page terminal is populated
  before the swap. Benefit: the live terminal stays duplicate-free, older bytes are appended
  forward (no reset flash), the two streams are fully independent, and swapping back to live is
  instant and current.
- **Progressive forward fill over one-shot reset+replay** — append-only, no resets. The user
  sees older output stream in as chunks resolve, and live output never pauses. The fill
  terminates naturally at the anchor (`atEnd`).
- **Native `Scrollbar { total, offset, len }` as the single source of truth** — the page
  terminal's viewport position is exposed in the same coordinate space as `scrollToLine`, matching
  ghostty's `PageList.Scrollbar`. This gives full viewport control (absolute row positioning) and
  a single source of truth for tests and any future scrollbar UI.
- **Mouse-tracking gating** — when the TUI has mouse tracking on, the wheel goes to the TUI,
  not the viewport, matching ghostty's `isMouseReporting` gate. (The native scroll-to-bottom
  policy is a no-op on the live terminal because `scrollback: 0` means the viewport can never be
  scrolled up away from the tip.)
- **`historyFetcher` callback over `Client` injection** — keeps `GhosttyTerminalGrpc` decoupled
  from `ConnectionService` and unit-testable with a plain function double.
- **Capture-phase wheel listeners** — ghostty-web's own wheel handler may stop propagation; a
  bubble-phase React `onWheel` would never see the event. The capture-phase listeners fire first
  and reliably detect the scroll-up-on-live / scroll-down-on-page intent on each pane.
- **Live terminal always mounted & streaming** — the live terminal is never unmounted or reset, so
  returning to the live tip is always instant and current, even after browsing history for a while.

## Future scope

- LiveKit transport: carry offset metadata on the LiveKit terminal frames so
  `GhosttyTerminalLiveKit` can use the same flow.
- Persisted scroll position across reconnects.
- A unified single-terminal surface if a future ghostty-web release adds a prepend/insert-at-top
  API that does not require a live-terminal reset, or a daemon-side PageList emulator that holds
  the terminal state and renders the visible window over RPC.
- Paged forward-fill: today the page terminal is filled with the entire retained capture
  (`0 → anchor`), which transfers all bytes even though the terminal retains only the last
  `scrollback` lines. A future optimization can page the forward-fill to fill the scrollback
  budget only (skipping bytes that would be discarded).
