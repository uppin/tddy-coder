# 2026-10-25 — Terminals show the current screen on connect and reconnect

Opening or reconnecting to a session's terminal shows the screen as it is right now — a full-screen
agent TUI included — without resizing the browser. The daemon and the coder keep a screen model for
every terminal, and a stream opened with the `SNAPSHOT` mode starts with one repaint of it (screen
buffers, cursor, colours, scroll region and input modes such as mouse tracking and bracketed paste),
followed only by later output with contiguous offsets: nothing lost, nothing repeated, and no
SIGWINCH sent to the program. Any screen may watch a terminal, with or without terminal control;
a keystroke sent without control is refused on the same stream and typing resumes once control is
reclaimed. Coder-served sessions answer the same way.
Sandboxed terminals refuse the snapshot open until their size is known to the daemon
([#762](https://github.com/uppin/tddy-coder/pull/762)). See
[Terminal sessions § Snapshot on (re)connect](../terminal-sessions.md#snapshot-on-reconnect).
