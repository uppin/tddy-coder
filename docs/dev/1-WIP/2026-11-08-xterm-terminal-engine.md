# Changeset: xterm.js terminal engine

**Date**: 2026-11-08
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — new engine and 5 dependencies, input-routing semantics (mouse/wheel ownership), renderer choice with fallback, specs across 3 packages

**Stack**: `#xterm` 3/4 — branch `feature/xterm/xterm-engine`, base `feature/xterm/terminal-engine`
**PR**: [#868](https://github.com/uppin/tddy-coder/pull/868)

## Initial Discovery

[2026-11-08-xterm-terminal-engine-initial-discovery.md](./2026-11-08-xterm-terminal-engine-initial-discovery.md)

## Affected Packages

- **tddy-terminal-web**: primary.
- **tddy-session-web**, **tddy-storybook**, **tddy-web**: consumers, specs and e2e as listed in Scope.

## Related Feature Documentation

- PRD: [PRD-2026-11-08-xterm-terminal-engine.md](../../ft/web/1-WIP/PRD-2026-11-08-xterm-terminal-engine.md)

## Summary

Replace ghostty-web with xterm.js as the web terminal's engine: `xtermEngine` implements `TerminalEngine` with `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-unicode11`, `@xterm/addon-clipboard` and `@xterm/addon-webgl` (WebGL renderer, the DOM renderer when WebGL is unavailable). The ghostty-only workarounds are deleted, and the canvas-geometry specs are rewritten.

## Background

ghostty-web needs a per-terminal WASM instance, a DPR watcher on a private renderer field, hand-written SGR mouse reporting, a capture-phase wheel gate and an IME Escape workaround. xterm.js handles DPR, mouse reporting and alternate-screen wheel itself, has a public API for buffers and modes, and is the widely used browser terminal. Developer decisions (2026-11-08): WebGL with DOM fallback; the five dependencies above.

## Prerequisites

- [2026-08-29-mobile-terminal-touch-two-gaps-left-open.md](../todo/2026-08-29-mobile-terminal-touch-two-gaps-left-open.md) — during: the tap→SGR route is rewritten here; the stray-press behaviour change is not claimed
- [2026-07-28-terminal-native-scrolling-model.md](../todo/2026-07-28-terminal-native-scrolling-model.md) — answered: xterm.js cannot prepend either, the double buffer stays

## Responsibility

Owns `src/engine/xtermEngine.ts`, the renderer selection, the five `@xterm/*` dependencies (and `bun.lock`), the deletions listed above, and the rewritten canvas specs.

## Boundaries

- Does not remove `ghostty-web` or `ghosttyEngine` (n4).
- No rename (n4).
- Does not change the lazy history model (the double buffer stays) or the daemon.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n2 `feature/xterm/terminal-engine` | `TerminalEngine`, the contract suite, top-based scrolling | implements the interface and runs the suite | change the interface shape without raising it on n2 |

## Draft PR contract

First push (commit 2), not this PR's deliverable: `xtermEngine` factory with its signature, the contract suite wired to run against it, and new specs: single mouse report per click/wheel (`TerminalMouseReporting.cy.tsx`), wide-character cells (`TerminalUnicodeWidth.cy.tsx`), renderer selection (`TerminalRenderer.cy.tsx`) — failing until the engine lands.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** after n2 — implements its interface
**Concurrent with:** none — the stack is serial
**Blocks:** n4 `feature/xterm/drop-ghostty`

Real dependency edges: `n1 → n2 → n3 → n4`. Pass 2 waits until `#web-split` is green (developer, 2026-11-08).

## Scope

- [ ] Add the five `@xterm/*` deps (mirror lockfile)
- [ ] `xtermEngine` + renderer selection
- [ ] Components construct `xtermEngine`
- [ ] Delete the ghostty-only workarounds
- [ ] Port OSC 52
- [ ] Rewrite canvas specs
- [ ] Contract suite + new specs green

## Technical Changes

### State A (Current)

ghostty-web renders every terminal through one canvas, with workarounds for its DPR, mouse, wheel and IME behaviour.

### State B (Target)

xterm.js renders every terminal (WebGL, DOM fallback); the workarounds are gone.

## Testing Plan

Cypress component specs one at a time locally, `bun test src`, scoped to the touched packages. CI: `web.yml` affected-package shards.

## Acceptance Tests

- `TerminalEngineContract.cy.tsx` against `xtermEngine`.
- `TerminalMouseReporting.cy.tsx` — one SGR report per click and per wheel step to a mouse-tracking program.
- `TerminalUnicodeWidth.cy.tsx` — emoji/CJK take two cells.
- `TerminalRenderer.cy.tsx` — renderer attribute is `webgl` or `dom`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- Developer, 2026-11-08 (pass 2 review): `xtermEngine` names its renderer on its container (`data-terminal-renderer`), so `TerminalEngine` is unchanged; the unicode-width spec drives the engine directly, the mouse and renderer specs the component; the mouse spec also pins one wheel-up report per **pixel-mode** wheel notch (`deltaY: -100`), not only per line-mode step.
- Developer, 2026-11-08: WebGL renderer with DOM fallback; deps `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-unicode11`, `@xterm/addon-webgl`, `@xterm/addon-clipboard`; rename inside the last node; stack registered as an extension of `#web-split`.

## Refactoring Needed

## Validation Results

## TODO

- [x] Record initial discovery
- [x] Cross-check code issues and TODOs
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Final validation (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
