# 2026-11-08 — Terminal components carry engine-neutral names

No change to what the terminal does. `ghostty-web` is no longer shipped: every web terminal renders
with xterm.js, and the desktop app's content security policy no longer allows WebAssembly
compilation. The terminal components, their test ids and their Storybook stories are named
`TerminalView` / `TerminalSession` / `terminal` rather than after the old emulator.
