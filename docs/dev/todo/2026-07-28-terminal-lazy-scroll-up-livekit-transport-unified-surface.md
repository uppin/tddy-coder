# 2026-07-28 — Terminal lazy scroll-up — LiveKit transport & unified surface

**Category:** Future enhancement
**Source:** terminal-replay-viewport changeset, 2026-07-28

- **LiveKit transport does not carry offset metadata.** `GhosttyTerminalGrpc` now owns the
  overlay double-buffer scroll-up paging via `GetTerminalHistory`, but `GhosttyTerminalLiveKit`
  still receives raw bytes only. Carry `end_offset`/`at_oldest` on the LiveKit terminal frames (or
  a side channel) so the LiveKit-backed terminal can use the same overlay paging flow.
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
