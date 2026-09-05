# 2026-03-29 — Status bar worktree segment, idle heartbeat phases, plan markdown tail, prompt caret, Virtual TUI cursor throttle

**Type:** Feature

`PresenterState::active_worktree_display` woven via `inject_worktree_into_status_line`; multi-phase idle glyph (`·`/`•`/`●`); `MarkdownViewer` Approve/Reject as trailing wrapped lines at end-of-scroll with `Paragraph::line_count` + `unstable-rendered-line-info`; `editing_prompt_cursor_position` + event-loop `Show`; Virtual Tui CSI strip and minimum interval for cursor-only diffs. Feature doc `docs/ft/coder/tui-status-bar.md`; `packages/tddy-tui/docs/architecture.md`. (tddy-tui)
