# 2026-07-28 — Terminal native scrolling model

**Category:** Future enhancement
**Source:** terminal-native-scrolling changeset, 2026-07-28

Adopts the native ghostty desktop scrolling model in the web (live `scrollback > 0`, native
`Scrollbar {total, offset, len}` on the page terminal, native scroll-to-bottom policy,
mouse-tracking gating). Future enhancements beyond that changeset:

- **Persisted scroll position across reconnects** — the live terminal lands at the live tip on
  reconnect and the page terminal fills from offset `0`; a future option can persist and restore
  the user's viewport position across sessions. (Also tracked above; kept here as the
  native-scrolling-scoped reference.)
- **Daemon-side PageList emulator** — the overlay double-buffer exists only because ghostty-web
  has no "insert at top of scrollback" API (a single terminal cannot lazily prepend older
  history). If a future ghostty-web release does not add a prepend API, a daemon-side PageList
  emulator that holds the terminal state and renders the visible window over RPC would give a
  true single-terminal surface (no overlay, no second instance).
- **Paged forward-fill** — the page terminal is filled with the entire retained capture
  (`0 → anchor`), which transfers all bytes even though the terminal retains only the last
  `scrollback` lines. Page the forward-fill to fill the scrollback budget only (skipping bytes
  that would be discarded). (Already noted above; kept as the native-scrolling-scoped reference.)

## Not addressed by the terminal-service unification

The wire side is settled — one coordinate,
`terminal_session.TerminalSessionService`, one replay and offset implementation for every host and
for sandboxed sessions too
([terminal-session-service.md](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md)).
Every item above is a **browser** concern and none of them changed: the overlay double-buffer is
still two ghostty-web instances, the page terminal is still filled from offset `0`, and there is
still no daemon-side PageList emulator. This entry stays open in full.
