# 2026-10-25 — `PtyTerminalService` opens with a snapshot
**Type:** Bug Fix

The room wire's PTY service opens with a snapshot frame taken under the capture lock, then live
frames with contiguous offsets; a later `request_snapshot` or a lagged receiver re-takes it at a new
sequence point. The raw replay and the `trigger_redraw()` on open are removed (#763,
`#term-snapshot` 4/4).
