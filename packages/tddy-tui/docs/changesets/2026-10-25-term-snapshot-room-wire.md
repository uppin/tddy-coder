# 2026-10-25 — VirtualTui repaints on request, without a resize
**Type:** Bug Fix

`VIRTUAL_TUI_REPAINT_REQUEST` at the front of the input clears the buffers without touching the grid,
so the next render is a full repaint. Every full repaint — requested or after a resize — is followed
by `EnableMouseCapture`, since the terminal service sends it led by RIS (#763, `#term-snapshot` 4/4).
