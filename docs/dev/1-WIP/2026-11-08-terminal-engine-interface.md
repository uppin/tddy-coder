# Changeset: Terminal engine interface with top-based scrolling

**Date**: 2026-11-08
**Status**: 🚧 In Progress
**Type**: Refactoring
**Complexity**: 4 — scroll-coordinate contract change across tddy-terminal-web, tddy-session-web and tddy-web specs

**Stack**: `#xterm` 2/4 — branch `feature/xterm/terminal-engine`, base `feature/xterm/terminal-seams`
**PR**: [#867](https://github.com/uppin/tddy-coder/pull/867)

## Initial Discovery

[2026-11-08-terminal-engine-interface-initial-discovery.md](./2026-11-08-terminal-engine-interface-initial-discovery.md)

## Affected Packages

- **tddy-terminal-web**: primary.
- **tddy-session-web**, **tddy-storybook**, **tddy-web**: consumers, specs and e2e as listed in Scope.

## Related Feature Documentation

- PRD: [PRD-2026-11-08-terminal-engine-interface.md](../../ft/web/1-WIP/PRD-2026-11-08-terminal-engine-interface.md)

## Summary

Introduce a `TerminalEngine` interface in `tddy-terminal-web` that the terminal components use instead of ghostty-web's `Terminal` directly, implemented by `ghosttyEngine`. Scroll positions in the interface and everywhere above it count from the top, so the xterm.js engine (n3) drops in without touching the components.

## Background

ghostty-web counts its viewport from the bottom (0 = live tip) and is reached through `as unknown` casts for scrolling, alternate-screen and mouse-tracking queries. xterm.js counts from the top and exposes those as public API. Flipping the coordinates while ghostty is still the engine isolates that risk from the engine swap.

## Prerequisites

- [2026-07-28-terminal-native-scrolling-model.md](../todo/2026-07-28-terminal-native-scrolling-model.md) — reference: the double buffer stays

## Responsibility

Owns `src/engine/TerminalEngine.ts`, `src/engine/ghosttyEngine.ts`, the engine contract test suite (`src/engine/terminalEngineContract.ts` + its ghostty run), and the top-based scroll rewrite of `lib/terminalScrollbar.ts`.

## Boundaries

- No xterm.js and no new dependency (n3).
- No deletion of ghostty workarounds beyond moving them behind the engine (n3).
- No rename (n4).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `feature/xterm/terminal-seams` | the extracted hooks/modules | routes each hook through the engine | re-split or resize modules beyond what the engine needs |

## Draft PR contract

First push (commit 2), not this PR's deliverable: `TerminalEngine` type, `ghosttyEngine` factory signature, and a reusable contract suite `describeTerminalEngine(makeEngine)` exercised in Cypress component tests (needs a browser for WASM), plus the rewritten `terminalScrollbar.test.ts` asserting top-based offsets — failing until the engine and the flip land.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** after n1 — the engine wraps n1's extracted hooks
**Concurrent with:** none — the stack is serial
**Blocks:** n3 `feature/xterm/xterm-engine` (implements this interface)

Real dependency edges: `n1 → n2 → n3 → n4`. Pass 2 waits until `#web-split` is green (developer, 2026-11-08).

## Scope

- [x] `TerminalEngine` + `ghosttyEngine`
- [x] Components and hooks use the engine only
- [x] Top-based scroll in handle, scrollbar, history mirrors, paging driver
- [x] Canvas-reading specs/drivers read through the engine or buffer text
- [x] Engine contract suite green on ghostty

## Technical Changes

### State A (Current)

Components call ghostty-web directly, partly through casts, with bottom-based scroll positions.

### State B (Target)

Components depend on `TerminalEngine`; ghostty is one implementation; scroll positions are top-based.

## Testing Plan

Cypress component specs one at a time locally, `bun test src`, scoped to the touched packages. CI: `web.yml` affected-package shards.

## Acceptance Tests

- `cypress/component/TerminalEngineContract.cy.tsx` — the contract suite run against `ghosttyEngine` (write/read lines, resize, top-based scroll, alternate-screen and mouse-tracking modes, events).
- `src/lib/terminalScrollbar.test.ts` — top-based `{total, offset, len}`.
- `src/engine/engineBoundary.test.ts` — only the engine module imports `ghostty-web`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- Developer, 2026-11-08 (pass 2 review): the contract renames `computeScrollbar`'s input to `viewportTop` and updates its one caller in `GhosttyTerminal.tsx` (behaviour unchanged, body still bottom-based until green) — accepted with the trivial rebase conflict against n1's split. The engine contract suite lives in `src/engine/terminalEngineContract.ts` (counted by n1's size rule) and the boundary test scans `tddy-terminal-web/src` production files only.
- Developer, 2026-11-08: WebGL renderer with DOM fallback; deps `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-unicode11`, `@xterm/addon-webgl`, `@xterm/addon-clipboard`; rename inside the last node; stack registered as an extension of `#web-split`.
- `[W]`, 2026-11-08 — **`getBufferText` pads each trimmed engine line to `cols`** (kept). ghostty-web's `translateToString()` concatenated `getChars()` per cell, and a blank or wide-spacer cell (codepoint 0) yields `""`, so the old text was not a fixed-width grid either; the padded form keeps line boundaries from fusing (`W=80` followed by a line starting `1` no longer reads `W=801`). A line with a wide character is padded by UTF-16 units, so it carries one more space than its cell count. Evidence that no consumer observes it: every reader of the `terminal-buffer-text` / `terminal-older-buffer-text` mirrors and of `getBufferText` (`DormantAgentTerminal`, `useTerminalMirrors`, the session-web specs `DormantAgentTabAcceptance`, `GhosttyTerminalOsc52Acceptance`, `GrpcSessionTerminalLazyHistory`, `SessionMainPaneRoomTerminal`, `TerminalResyncOnGap`, `TerminalSnapshotOnReconnect`, the terminal-web specs and drivers, and the tddy-web e2e specs `ghostty-*`, `terminal-rendering`, `waitForBufferText`) uses `contain`/`include`/`indexOf`/`split` on in-line substrings, a regex within a line, or `compactNoWs`; none reads a per-line offset or length. All of those component specs pass locally.
- `[W]`, 2026-11-08 — **No FitAddon `fit()`/`observeResize()`** (kept). ghostty-web's `open(container)` sets `term.element = container`, and `observeResize()` observed `term.element` — the same element the lifecycle's own ResizeObserver already watched, and which already did `proposeDimensions` + `resize` (n1). So a container resize without a window resize still re-fits; the addon's path only added a timer debounce and the `_isResizing` latch behind the GhosttyTerminalShrink failure. Font changes call `fitTerminalToContainer` where they called `fit()`, which had the same "propose, resize if different" body minus the latch. Evidence: `GhosttyTerminalShrink` (container shrink), `TerminalZoomAcceptance` and `GhosttyTerminal` pass locally.
- `[W]`, 2026-11-08 — **Fixed: `ghosttyEngine.dispose()` removed the caller's container.** ghostty-web's `element` is the container passed to `open`, and `[G]` called `element.remove()` after `term.dispose()`; React owns that node (StrictMode re-runs the effect on the same node). ghostty-web's own dispose already removes its canvas and textarea, so the extra `remove()` is gone, and the contract's lifecycle case now asserts the container stays connected.

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
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Run the touched packages' tests — verify 100% pass
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [x] Validate tests (/validate-tests)
- [x] Validate production readiness (/validate-prod-ready)
- [x] Analyze code quality (/analyze-clean-code)
- [x] Final validation (/validate-changes)
- [x] USER REVIEW — scope developed, every gap approved: ready to wrap
