# 2026-10-25 — Sandboxed terminals show the current screen on connect and reconnect

A sandboxed session's terminal now opens and reconnects with a snapshot of its current screen, like
every other terminal. The runner inside the jail reports its PTY size when the PTY opens and after
every resize it applied, in order with its output, so the daemon's screen model is always at the
jail's real grid and the snapshot renders what the jail's screen shows. The blanket
`FAILED_PRECONDITION` refusal for jailed terminals is gone; a jailed terminal that has not reported
a size yet still refuses rather than snapshot at a guessed size
([#762](https://github.com/uppin/tddy-coder/pull/762)). See
[Terminal sessions § Snapshot on (re)connect](../terminal-sessions.md#snapshot-on-reconnect).
