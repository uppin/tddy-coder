# Remove ghostty-web and rename the terminal components - PRD

**Date**: 2026-11-08
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#xterm` 4/4 — branch `feature/xterm/drop-ghostty`

## Affected Features

- Every web terminal — names only.

## Summary

Remove `ghostty-web` and everything that existed only for it (`ghosttyEngine`, `lib/ghosttyInstance.ts`, the prebundle entry, Tauri's `wasm-unsafe-eval`, the storybook render exclusion), and rename `Ghostty*` to `Terminal*` across exports, testIds, story ids, Cypress commands, file names and docs.

## Background

After n3 no terminal uses ghostty-web. Its name remains on the public components and test hooks, which would mislead readers.

## Proposed Changes

### What's Changing

- Delete `ghostty-web` from `tddy-terminal-web`'s dependencies, `ghosttyEngine`, `lib/ghosttyInstance.ts`, the `ghostty-web` entry in `scripts/vite-prebundle.ts`, `'wasm-unsafe-eval'` from `tauri.conf.json` CSP (if nothing else needs WASM), and the ghostty exclusion in `tddy-storybook`'s `storiesRender.test.tsx`.
- Rename: `GhosttyTerminal` → `TerminalView` (not `Terminal`, which is xterm's class), `GhosttyTerminalSession` → `TerminalSession`, their `Props`/`Handle` types; testIds `ghostty-terminal` → `terminal`, `ghostty-terminal-dormant` → `terminal-dormant`, `TEST_IDS.ghosttyTerminal` → `TEST_IDS.terminal`; story title `components/GhosttyTerminal` → `components/Terminal` and `cy.visitGhosttyStory` → `cy.visitTerminalStory`; spec, driver and e2e file names; docs.

### What's Staying the Same

- Behaviour. Rust e2e test names that mention ghostty (`*_ghostty_virtual_terminal_e2e`) — recorded as a todo.

## Acceptance Criteria

1. `ghostty-web` is in no `package.json` and not in `bun.lock`.
2. No source, spec, testId or story id in `packages/tddy-*-web`, `tddy-storybook` or `tddy-web` contains `ghostty` (case-insensitive), except changelog/changeset history.
3. Tauri CSP has no `wasm-unsafe-eval` unless another WASM user is named.
4. The storybook render test covers the terminal story.
5. All terminal specs and the storybook e2e specs pass under the new names.

## References

- Stack: `#xterm` (ghostty-web → xterm.js), stacked on `feature/web-split/wiring` (#853).
- Predecessor: `feature/xterm/xterm-engine`.
