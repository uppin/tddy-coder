# 2026-11-08 — The dormant agent tab reads recorded output once it is parsed (#xterm 3/4, PR #868)

**Type:** Fix

- `DormantAgentTerminal` reads the terminal's buffer text after the latest recorded chunk has been
  parsed (`onWriteParsed`), since xterm.js parses writes asynchronously. The pending read is
  disposed when the effect is torn down.
- `GhosttyTerminalShrink.cy.tsx` measures `.xterm-screen` instead of a canvas.
