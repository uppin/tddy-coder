# 2026-10-03 — Navigable `CodeBlock`: definition, hover and references

**Type:** Feature

`#live-plan` 8/15, PR [#574](https://github.com/uppin/tddy-coder/pull/574). Cross-package entry:
[2026-10-03-code-navigation.md](../../../../docs/dev/changesets/2026-10-03-code-navigation.md).

`CodeBlock` carries one-based line and UTF-8 byte-column positions on every line and identifier and
takes `onNavigate` / `onHover` / `focusLine`. `useCodeNavigation` owns the selected file and the
navigation state and handlers; `CodeNavigationOverlays` renders the hover card, the location list and
the notice; `codeNavigationApi` adapts `CodeNavigationService`. `SessionsDrawerScreen` resolves the
client for the owning host and passes it through `SessionMainPane` to `WorktreeCodePane`. Navigation is
offered for `.rs` files only. See [code-navigation.md](../code-navigation.md).

Tests: `cypress/component/WorktreeCodePaneNavigation.cy.tsx` (3 specs) alongside the existing pane spec
(11), both passing.

Code issues: `oversized-file-use-model-registry-fan-out` names code this change did not touch.
`SessionMainPane.tsx` (657 → 666) and `SessionsDrawerScreen.tsx` (966 → 969) grew past the file-length
budget; their decomposition is deferred in the backlog.
