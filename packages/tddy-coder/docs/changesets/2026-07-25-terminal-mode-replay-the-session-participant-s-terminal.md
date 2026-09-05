# 2026-07-25 — terminal-mode-replay: the session participant's terminal attach replays `cap.replay()` (mouse-mode prologue ++ retained output) instead of a raw capture clone, and `PtyHandle.capture` in `session_participant/{mod.rs,terminal_manager.rs}` becomes `Arc<Mutex<TerminalCapture>>`

**Type:** Fix

so a client attaching to a coder-hosted bash terminal after the startup DECSETs left the ring still gets a mouse-capable VT. Mechanics in [tddy-task terminal-capture.md](../../../tddy-task/docs/terminal-capture.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-coder)
