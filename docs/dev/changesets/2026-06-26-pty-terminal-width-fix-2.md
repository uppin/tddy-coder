# 2026-06-26 — **PTY terminal width fix

**Type:** Fix

correct initial cols/rows on gRPC reconnect** — `connection.proto`: `initial_cols`/`initial_rows` (uint32 4/5) on `StreamTerminalOutputRequest`; `tddy-daemon`: `stream_terminal_output` resizes PTY before replay, drains stale broadcast, triggers SIGWINCH redraw, skips capture replay when dims provided; `PtyHandle::send_input` strips `\x1b]resize;{cols};{rows}\x07` escape sequences; `kill_all()` on daemon shutdown (SIGTERM → SIGKILL); CAPTURE_LIMIT_BYTES 64KB→512KB; `tddy-web`: new `GrpcSessionTerminal` component measures container and sends `initial_cols`/`initial_rows`; `GhosttyTerminalGrpc` gains `terminal-buffer-text` polling div; new `tddy-demo-tui` binary (TIOCGWINSZ + SIGWINCH redraw, fake claude CLI for e2e); 4 e2e + 3 component tests. Feature [terminal-sessions.md](../ft/daemon/terminal-sessions.md). (tddy-service, tddy-daemon, tddy-web, tddy-demo-tui)
