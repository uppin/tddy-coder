# 2026-06-26 — PTY terminal width fix — correct cols/rows on gRPC reconnect

- `StreamTerminalOutputRequest` now accepts `initial_cols`/`initial_rows`; when non-zero the daemon resizes the PTY, drains stale broadcast output, and triggers a SIGWINCH redraw before forwarding live output — eliminates 220-column garbling on browser reconnect
- `PtyHandle::send_input` strips `\x1b]resize;{cols};{rows}\x07` OSC escape sequences from stdin data and calls `PtyHandle::resize` transparently
- `kill_all()` on daemon shutdown: sends SIGTERM to every registered PTY process, waits up to 5 s, then SIGKILL for survivors; clears the registry
- Capture replay buffer limit raised 64 KB → 512 KB (more history for reconnecting clients)
- New `tddy-demo-tui` binary: reads PTY dimensions via TIOCGWINSZ, draws `DEMO TUI W={cols} H={rows}`, redraws on SIGWINCH — used as fake claude CLI in e2e tests
