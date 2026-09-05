# 2026-03-08 — TUI UX, Plan Resume, Ctrl+C

- **TUI scroll**: PageUp/PageDown for activity log; no mouse capture so terminal text selection works.
- **Ctrl+C**: Raw mode with ISIG preserved; ctrlc handler restores LeaveAlternateScreen, cursor Show, disable_raw_mode.
- **Plan resume**: When `--session-dir` has Init state and no PRD.md, runs plan() to complete the plan.
- **Debug area**: `--debug` enables TUI debug area and TDDY_QUIET bypass for debug output.
