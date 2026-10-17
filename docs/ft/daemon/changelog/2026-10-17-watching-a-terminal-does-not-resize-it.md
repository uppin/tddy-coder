# 2026-10-17 — Watching a session's terminal no longer resizes it

- **A screen that has not claimed control cannot reflow the terminal.** Opening a session on a
  phone while the desktop drives it used to resize the PTY to the phone's width the moment the
  output stream opened, before any claim. `StreamTerminalOutput` now honours its
  `initial_cols`/`initial_rows` only for the lease holder (or when nobody holds the lease); other
  screens watch at the driver's size. The request gains a `control_token` field for this.
- **Claiming control resizes the terminal to the claiming screen.** The web terminal re-sends its
  grid each time it gains the lease, including a claim back after being displaced, so the TUI is
  laid out for the grid the screen shows and taps land on the cell under the finger.
- **The `tddy-coder` terminal relay** sends no control token, so while a browser drives a session
  the relay opens at the existing size instead of resizing it.

See [terminal sessions](../terminal-sessions.md#reconnect-resume-by-offset-streamreplaymode).
