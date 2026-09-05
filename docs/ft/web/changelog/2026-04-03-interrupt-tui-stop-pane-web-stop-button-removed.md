# 2026-04-03 — Interrupt: TUI Stop pane; web Stop button removed

- **tddy-web**: **`ConnectionTerminalChrome`** no longer renders a bottom-right **Stop** button or **`onStopInterrupt`**. Interrupt is the ratatui **Stop** pane (red **U+25A0**) beside the Enter strip; the browser forwards SGR mouse to the virtual TUI (same **0x03** path as **Ctrl+C**).
- **Feature docs**: [web-terminal.md](../web-terminal.md) (Connection chrome); [TUI Stop control](../../coder/tui-status-bar.md#mouse-mode-stop-control).
