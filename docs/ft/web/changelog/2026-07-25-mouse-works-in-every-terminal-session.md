# 2026-07-25 — Mouse works in every terminal session

- Fixed: only the **newest** session responded to the mouse — older sessions dropped every click, drag and scroll while keyboard input kept working. A terminal reports mouse events only after it has seen the TUI's mouse-tracking DECSET, which is emitted once at startup and eventually trimmed out of the server's 64 KiB replay buffer.
- The server now remembers the mouse modes still in effect and re-issues them as the **first frame on every attach**, so opening an older session, reconnecting, or reloading the page all restore mouse input. See [web-terminal.md § Touch/mouse mode](../web-terminal.md).
- The replay buffer also stops trimming mid-escape-sequence, so a reconnect can no longer print a stray fragment (e.g. a bare `1m`) at the top of the restored screen.
