# 2026-03-29 — TUI status bar: worktree segment, idle heartbeat, plan tail, prompt caret

- **Core**: `PresenterState::active_worktree_display` carries a short worktree label from `WorkflowEvent::WorktreeSwitched` via `presenter::worktree_display::format_worktree_for_status_bar`.
- **TUI**: The status line prefixes activity, session segment, and optional worktree label before `Goal:`; idle waits use a multi-phase `·`/`•`/`●` heartbeat while agent-active mode keeps the fast spinner; goal elapsed stays frozen in clarification waits as documented in [tui-status-bar.md](../tui-status-bar.md).
- **Markdown plan viewer**: Approve and Reject appear as trailing wrapped lines after the user scrolls to the end of the plan body; scroll bounds use `Paragraph::line_count` with the same wrap as draw (ratatui `unstable-rendered-line-info`).
- **Local TUI**: The hardware cursor sits at the UTF-8-safe insert index for prompt editing; crossterm `Show` runs when a caret position applies.
- **Virtual TUI / streaming**: Cursor-position-only CSI updates respect a minimum send interval; full frames still diff on paint changes.
- **Docs**: [tui-status-bar.md](../tui-status-bar.md); `packages/tddy-tui/docs/architecture.md`; `packages/tddy-core/docs/architecture.md`.
