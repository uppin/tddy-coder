# Changeset: Terminal component split along its input, scroll and history seams

**Date**: 2026-11-08
**Status**: 🚧 In Progress
**Type**: Refactoring
**Complexity**: 3 — one package, Cypress component integration tests, behaviour-preserving hand split of two components

**Stack**: `#xterm` 1/4 — branch `feature/xterm/terminal-seams`, base `feature/web-split/wiring`
**PR**: [#866](https://github.com/uppin/tddy-coder/pull/866)

## Initial Discovery

[2026-11-08-terminal-component-seams-initial-discovery.md](./2026-11-08-terminal-component-seams-initial-discovery.md)

## Affected Packages

- **tddy-terminal-web**: primary.
- **tddy-session-web**, **tddy-storybook**, **tddy-web**: consumers, specs and e2e as listed in Scope.

## Related Feature Documentation

- PRD: [PRD-2026-11-08-terminal-component-seams.md](../../ft/web/1-WIP/PRD-2026-11-08-terminal-component-seams.md)

## Summary

Split `GhosttyTerminal.tsx` (1491 lines) and `GhosttyTerminalSession.tsx` (1004 lines) in `tddy-terminal-web` into hooks and modules along their natural seams, with no behaviour change. This closes the two oversized-file code issues and gives the engine swap in the next nodes small, separately testable units.

## Background

Both components have been over the 500-line budget since 2026-10-24, and the split was deferred to "its own behaviour-preserving PR". The `#xterm` stack replaces ghostty-web with xterm.js; the code it must rewrite (mouse/touch forwarding, wheel gate, scroll mapping, history double buffer, fit/zoom) is interleaved through these two files. Splitting first keeps the engine swap reviewable.

## Prerequisites

- [oversized-file-ghostty-terminal.md](../../../packages/tddy-terminal-web/docs/code-issues/oversized-file-ghostty-terminal.md) — ✅ RESOLVED HERE
- [oversized-file-ghostty-terminal-session.md](../../../packages/tddy-terminal-web/docs/code-issues/oversized-file-ghostty-terminal-session.md) — ✅ RESOLVED HERE
- [2026-10-24-ghostty-terminal-components-over-budget.md](../todo/2026-10-24-ghostty-terminal-components-over-budget.md) — ✅ RESOLVED HERE
- [2026-11-07-terminal-web-does-not-typecheck.md](../todo/2026-11-07-terminal-web-does-not-typecheck.md) — owned by the `#web-split` sweep node; not claimed here

## Responsibility

Owns the new hook/module files under `packages/tddy-terminal-web/src/` (`terminal/` and `session/` subfolders) and the reduced `GhosttyTerminal.tsx` / `GhosttyTerminalSession.tsx`.

## Boundaries

- No engine abstraction (n2 `feature/xterm/terminal-engine`).
- No scroll-coordinate change, no xterm.js, no dependency change.
- No rename.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#853` web-split wiring | `tddy-terminal-web` as its own package, consumed by `tddy-session-web` / `tddy-web` | splits that package's two components | change the package's exports or its consumers |

## Draft PR contract

First push (commit 2), not this PR's deliverable: A structural rule test (`src/componentSize.test.ts`) asserting every `src/**/*.tsx?` file in the package is ≤ 500 lines — failing on the two oversized components until the split lands. The existing Cypress specs are the behaviour baseline (green before and after).

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — needs only #853's package layout
**Concurrent with:** none — the stack is serial
**Blocks:** n2 `feature/xterm/terminal-engine` (its interface wraps the extracted seams)

Real dependency edges: `n1 → n2 → n3 → n4`. Pass 2 waits until `#web-split` is green (developer, 2026-11-08).

## Scope

- [x] Baseline: run the package's component specs and unit tests green before moving code
- [x] Extract hooks from `GhosttyTerminal.tsx`
- [x] Extract hooks from `GhosttyTerminalSession.tsx`
- [x] Size rule test passes
- [x] Same specs green after
- [x] Close both oversized-file code issues and the todo

## Technical Changes

### State A (Current)

Two components of 1491 and 1004 lines hold every listener, handle method and history-buffer step inline.

### State B (Target)

Each component is a composition of focused hooks/modules, each ≤ 500 lines, with unchanged behaviour.

## Testing Plan

Cypress component specs one at a time locally, `bun test src`, scoped to the touched packages. CI: `web.yml` affected-package shards.

## Acceptance Tests

- `src/componentSize.test.ts` — every source file ≤ 500 lines (bun test).
- Existing specs, unchanged: all `packages/tddy-terminal-web/cypress/component/*.cy.tsx`, the terminal specs in `tddy-session-web`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- Developer, 2026-11-08 (pass 2 review): the size rule covers **production files only** — `*.test.ts(x)` are excluded, so `src/rpc/terminalFeed.test.ts` (609 lines) is outside this node's scope. The code issues measure production files.
- Developer, 2026-11-08: WebGL renderer with DOM fallback; deps `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-unicode11`, `@xterm/addon-webgl`, `@xterm/addon-clipboard`; rename inside the last node; stack registered as an extension of `#web-split`.

## Refactoring Needed

## Validation Results

Final measurement (2026-11-08, `/green`), `wc -l` of the production files, closing the two code issues and the backlog entry:

| File | Before | After |
|---|---|---|
| `src/GhosttyTerminal.tsx` | 1491 | 199 |
| `src/GhosttyTerminalSession.tsx` | 1004 | 486 |
| largest new file (`src/terminal/useTerminalLifecycle.ts`) | — | 277 |

New modules: `src/terminal/` (types, loggers, geometry, refs, lifecycle, font size/fit/zoom, pinch zoom, touch scroll routes, mouse forwarding, touch focus and tap-as-click, key and IME forwarding, imperative handle) and `src/session/` (props, diagnostics, feed stream and session-ended, history double buffer, buffer-text and viewport mirrors, live wheel gate and test window hooks, probes, pane overlays and styles). Mechanical moves only; exports of `src/index.ts`, props, test ids and specs unchanged. `tsc --noEmit` stays at the 14 recorded errors.

## TODO

- [x] Record initial discovery
- [x] Cross-check code issues and TODOs
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Final validation (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
