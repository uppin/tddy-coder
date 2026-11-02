# 2026-11-02 — "+" sits after the last session tab

**Type:** Feature

`#session-tabs 5/7` ([#808](https://github.com/uppin/tddy-coder/pull/808)). `SessionTerminalTabs` renders `NewTerminalMenu` as the last item of the tab scroller, sticky at its right edge; the menu is portaled into `document.body`, placed under the button and clamped to the viewport. Reference: [terminal-session.md](../terminal-session.md).

Verified (scoped to `tddy-web`): `bun test src/components/sessions` 433 pass; Cypress `SessionTerminalTabsPlusPlacement` 7, `SessionTerminalTabsAgentKindsAcceptance` 7, `SessionTerminalTabsAcceptance` 5, `SessionTerminalTabsAgentTabs` 8, `SessionTerminalFullscreenAcceptance` 5, `DormantSessionTabsAcceptance` 11, all passing.

`SessionTerminalTabs.tsx` and `NewTerminalMenu.tsx` are also modified by #806 and #810, so neither is restructured here (both are under 500 lines).

## Validation results

- validate-changes: diff is this PR's only; Responsibility and acceptance criteria delivered. Gap fixed in the wrap: a viewport-edge `+` made the menu hang off the screen, so placement now clamps to the viewport (new Cypress case).
- Inline menu styles (`position`, `z-index`, coordinates): justified, since the coordinates are computed and the CSS-less harness painted a class-only portal under the page. Documented in `terminal-session.md`.
- validate-tests: Given-When-Then, no sleeps; the viewport-edge case fails without the clamp.
- validate-prod-ready: no mock code, TODO/FIXME or debug output.
- analyze-clean-code: `NewTerminalMenu.tsx` 271 lines, magic values are named constants, nesting at most 3.
