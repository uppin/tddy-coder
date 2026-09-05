# 2026-03-08 — TUI with ratatui

- **TUI layout**: Scrollable activity log (top), status bar (middle), prompt bar (bottom). Uses ratatui + crossterm with alternate screen buffer.
- **Status bar**: Displays Goal, State, elapsed time. Goal-specific background colors (plan: yellow, acceptance-tests: orange, red: red, green: green, evaluate/validate: blue). Bold white text. Blank line before status bar.
- **Prompt bar**: Fixed at bottom with "> " prefix. Placeholder when empty: "> Type your feature description and press Enter..."
- **"Other" option**: Select and MultiSelect clarification prompts include "Other (type your own)" as last choice. Selecting it enables free-text input.
- **Piped mode**: When stdin or stderr is not a TTY, TUI is skipped; plain mode uses linear eprintln output.
- **Agent output**: Always visible. On resume (Claude/Cursor --resume) with `--conversation-output`, replayed output is skipped; only new output is echoed.
- **inquire removed**: Replaced entirely by custom ratatui widgets.
