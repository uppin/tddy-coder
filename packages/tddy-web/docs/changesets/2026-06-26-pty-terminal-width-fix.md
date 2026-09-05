# 2026-06-26 — **PTY terminal width fix

**Type:** Fix

GrpcSessionTerminal + GhosttyTerminalGrpc** — new `GrpcSessionTerminal` component: measures container via `getBoundingClientRect()`, sends `initial_cols`/`initial_rows` in `StreamTerminalOutput` request (8px/17px char-cell estimates); `GhosttyTerminalGrpc`: 200ms polling interval + hidden `data-testid="terminal-buffer-text"` div for Cypress visibility; 3 component tests (GrpcSessionTerminalResize.cy.tsx) + 4 e2e tests (terminal-rendering.cy.ts). Feature [web-terminal.md](../../../../docs/ft/web/web-terminal.md). (tddy-web)
