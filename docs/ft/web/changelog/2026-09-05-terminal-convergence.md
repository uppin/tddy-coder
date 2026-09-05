# 2026-09-05 — Scrollback on every wire, from one terminal

A session's terminal scrolls back through earlier output whatever carries the session. Until now only
a host-served session could: a session carried over its own LiveKit room saw nothing older than what
was live, because the terminal rendering it had no history at all.

There is one terminal component now, not two. It is handed a feed by the session's connection and asks
that feed whether it can replay — so which transport a session is on stops deciding what its terminal
can do. A room-carried session's room serves its PTY and nothing else, so its scrollback is fetched
from the host daemon that holds the capture ring; the output itself still travels the room.

The scrollback control appears when the connection can serve history and stays hidden when it cannot,
where before its absence was a property of the wire rather than of what the host could actually offer.

Everything both terminals did survives unchanged: the status strip, reconnect handling, resize, zoom
and pinch bounds, file drop and upload, the mobile keyboard, and the shortcut drawer.

Two fixes come with it. The standalone room screen now shows a token-fetch failure instead of going
blank — the screen whose job was to report that error was the one screen that could not. And when
another screen holds the terminal's control lease, the "Claim terminal" prompt stays on top of the
terminal canvas rather than being painted over by it, which had left such a session looking
interactive while it swallowed every keystroke.

See [Terminal replay — lazy scroll-up history](../terminal-replay-lazy-scroll.md) and
[the session terminal](../../../packages/tddy-web/docs/terminal-session.md).
