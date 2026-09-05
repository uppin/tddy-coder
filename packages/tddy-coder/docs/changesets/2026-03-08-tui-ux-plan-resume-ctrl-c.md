# 2026-03-08 — TUI UX, Plan Resume, Ctrl+C

**Type:** Feature

TUI scroll via PageUp/PageDown (no mouse capture; text selection works). Raw mode with ISIG preserved; ctrlc restores LeaveAlternateScreen, cursor Show, disable_raw_mode. Plan resume: when `--plan-dir` has Init state and no PRD.md, runs plan() to complete. Debug area in TUI when `--debug`; TDDY_QUIET bypass for debug output. (tddy-coder)
