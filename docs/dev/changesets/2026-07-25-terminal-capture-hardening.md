# 2026-07-25 — terminal-capture-hardening

**Type:** Fix

closes the three gaps carried forward from terminal-mode-replay: RIS (`ESC c`) clears the sticky mouse-mode set, eviction's escape-sequence chase is bounded at 1 KiB so an unterminated OSC/DCS cannot empty the replay ring, and the daemon's sandbox sessions swap their raw unbounded capture for a `TerminalCapture` so the fourth attach path also sends the mode prologue. Docs [terminal-capture.md](../../../packages/tddy-task/docs/terminal-capture.md), [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#terminal-mode-replay-mouse-tracking). (tddy-task, tddy-daemon)
