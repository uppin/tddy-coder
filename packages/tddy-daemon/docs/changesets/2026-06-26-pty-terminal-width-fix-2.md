# 2026-06-26 — PTY terminal width fix

**Type:** Fix

`stream_terminal_output`: reads `initial_cols`/`initial_rows` from request, resizes PTY via `PtyHandle::resize`, drains stale broadcast messages (`try_recv` loop), calls `trigger_redraw()`, skips capture replay when dims provided; `PtyHandle::send_input`: strips `\x1b]resize;{cols};{rows}\x07` escape sequences before forwarding stdin; `kill_all()`: SIGTERM + 5s wait + SIGKILL for all registered PIYs, clears registry; CAPTURE_LIMIT_BYTES raised 64KB→512KB; 4 kill_all unit tests, 3 component acceptance test updates.
