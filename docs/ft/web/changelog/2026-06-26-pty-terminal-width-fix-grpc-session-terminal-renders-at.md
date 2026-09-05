# 2026-06-26 — PTY terminal width fix — gRPC session terminal renders at correct width

- New `GrpcSessionTerminal` component: measures its container's pixel width/height, computes `initial_cols`/`initial_rows` (8px × 17px character-cell estimates), and passes them to the `StreamTerminalOutput` gRPC request so the daemon resizes the PTY before forwarding output
- `GhosttyTerminalGrpc` gains a hidden `data-testid="terminal-buffer-text"` div (200 ms polling) enabling Cypress to assert visible terminal text without OCR
- New `GrpcSessionTerminalResize.cy.tsx` component tests (3) verify `initial_cols > 0`, `initial_rows > 0`, and that cols match container width
- New `terminal-rendering.cy.ts` e2e tests (4) against a live daemon with `tddy-demo-tui`: AC1 width ≠ 220, AC2 no horizontal overflow, AC3 resize updates cols, AC4 reconnect shows correct width immediately
