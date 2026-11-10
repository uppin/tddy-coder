# 2026-11-08 — Terminal e2e specs and Cypress command use the engine-neutral names (#xterm 4/4, PR #869)

**Type:** Refactor

`cy.visitGhosttyStory` → `cy.visitTerminalStory`; the `ghostty-terminal-*.cy.ts` e2e specs are
`terminal-*.cy.ts` and target the `components-terminal--*` stories and the `terminal` test id. No
behaviour change.
