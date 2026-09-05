# 2026-03-08 — TUI with ratatui

**Type:** Feature

Full TUI replaces inquire. Layout: scrollable activity log (top), status bar (goal + state + elapsed, goal-specific colors), prompt bar (bottom). "Other (type your own)" option on Select/MultiSelect clarification prompts. Piped mode (non-TTY) uses plain.rs. Agent output always visible; on resume with --conversation-output, replayed output skipped. TDDY_QUIET suppresses debug eprintln during TUI. (tddy-coder)
