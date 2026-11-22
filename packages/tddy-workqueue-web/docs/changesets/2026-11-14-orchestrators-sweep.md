# 2026-11-14 — The `#orchestrators` sweep: `PrStackScreen` split, and the "New stack" action

**Type:** Refactor

`#orchestrators` 12/12, PR [#901](https://github.com/uppin/tddy-coder/pull/901). Feature:
[PR-stack package § Behaviour](../../../docs/ft/web/prstack-package.md),
[PR stacking § Opening a project stack by id](../../../docs/ft/coder/pr-stacking.md#opening-a-project-stack-by-id).

## What changed

- **B11** — the Start-session flow moved to `usePrStackStartSession` and
  `PrStackStartSessionDialog`, and the stack-plan loading to `usePrStackPlan`.
  `PrStackScreen.tsx` is 395 lines.
- **B9** — `ProjectStackView` carries a "New stack" action: it calls `CreateProjectStack` and
  then adds planned PRs to the created stack by its address.

## Code issues

- `oversized-file-pr-stack-screen.md` deleted: 731 → 395 lines (final measurement above).
