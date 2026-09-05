# 2026-03-18 — Terminal Resize Support

- **Local event loop**: Handles `Event::Resize` with `terminal.clear()` for a clean redraw with no visual artifacts.
- **Virtual TUI**: Accepts `\x1b]resize;cols;rows\x07`; after `terminal.resize()` calls `terminal.clear()` and resets the frame buffer so the next render sends a full frame to the remote client.
- **Scroll offset**: Clamped after resize so content does not jump past the end.
- **Packages**: tddy-tui (event_loop.rs, virtual_tui.rs).
