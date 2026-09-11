# 2026-07-28 — Terminal lazy scroll-up — LiveKit transport & unified surface

**Category:** Future enhancement
**Source:** terminal-replay-viewport changeset, 2026-07-28

- ~~**LiveKit transport does not carry offset metadata.**~~ **Settled by the side channel.** A
  room-carried session's feed (`packages/tddy-web/src/rpc/connections/livekit/roomTerminalFeed.ts`)
  keeps its bytes on the bidi `terminal.TerminalService/StreamTerminalIO` and gets its history from
  the owning daemon's `terminal_session.TerminalSessionService/GetTerminalHistory`, so a
  LiveKit-backed terminal pages exactly like an HTTP-backed one without a single output byte leaving
  the room.
- **Paged forward-fill.** The page terminal is filled with the entire retained capture
  (`0 → anchor`), which transfers all bytes even though the terminal retains only the last
  `scrollback` lines. Page the forward-fill to fill the scrollback budget only (skipping bytes
  that would be discarded).
- **Unified single-terminal surface.** The viewport integration uses two interchangeable,
  overlaid ghostty-web terminals (live at `scrollback: 0`, page at `scrollback > 0`) to avoid
  resetting the live terminal (which would reintroduce the duplicate-pane bug). A unified
  single-terminal surface is infeasible today because ghostty-web has no "insert at top of
  scrollback" API and a live reset is unacceptable; revisit if a future ghostty-web release adds a
  prepend API that does not require a live-terminal reset.
- **Persisted scroll position across reconnects** is out of scope; the forward fill populates the
  page terminal from offset `0` toward the anchor.

## What is settled at the RPC layer, and what is not

The **server-side** terminal surface is one surface:
`terminal_session.TerminalSessionService`, served by
[`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md) for every
host. One terminal message set, one implementation of the replay and offset contract, no converters,
and a sandboxed session's PTY reached through the same `TerminalSessionStore` as every other — so
`GetTerminalHistory` and the anchored replay work for a jailed terminal too. That removes every
*protocol* reason a terminal could page differently depending on how it was reached.

**The remaining items in this entry are all browser-side and none of them is addressed:** paged
forward-fill, the unified single-terminal surface (which is blocked on ghostty-web having no
"insert at top of scrollback" API, not on the wire), and persisted scroll position across reconnects.
This entry stays open for those.
