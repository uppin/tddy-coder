# Changeset: Full-screen remote desktop viewer with keyboard lock

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**PR**: (opened in wave 1; number recorded in wave 2)
**Type**: Feature
**Stack**: `#host-desktops` 5/8. Base: `feature/host-desktops/hosts-mount`.

## Initial Discovery

[2026-10-18-host-desktops-viewer-fullscreen-initial-discovery.md](./2026-10-18-host-desktops-viewer-fullscreen-initial-discovery.md)

## Responsibility

- `src/lib/browserFullscreen.ts` generalised: `requestElementFullscreen(el)` (renaming
  `requestFullscreenForConnectedTerminal`, terminal callers updated), `exitDocumentFullscreen`,
  `isTargetInActiveFullscreen`, plus a `useElementFullscreen(ref)` hook reporting state from
  `fullscreenchange`.
- `src/lib/keyboardLock.ts` (new): `isKeyboardLockSupported()`, `lockKeyboard()`, `unlockKeyboard()`.
- `ScreenSharingOverlay.tsx`: the full-screen toggle (`data-testid="screen-sharing-fullscreen"`),
  its label by state, the "Esc leaves full screen" hint when not locked, lock on enter, unlock and
  exit on leave and on unmount.

## Boundaries

- Does not change input forwarding or scaling (`useScreenSharingInput`), the closing chord, or the host flow.
- Does not add picture-in-picture or scaling modes.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1–n4 | — | nothing (line predecessors only) | touch Hosts components or Rust |

## Draft PR contract

```ts
// src/lib/browserFullscreen.ts
export function requestElementFullscreen(target: HTMLElement): Promise<void>;
export function useElementFullscreen(ref: RefObject<HTMLElement>): { isFullscreen: boolean; enter(): Promise<void>; exit(): Promise<void> };
// src/lib/keyboardLock.ts
export function isKeyboardLockSupported(): boolean;
export function lockKeyboard(): Promise<void>;
export function unlockKeyboard(): void;
```
`ScreenSharingOverlay` renders the toggle; behaviour pending. Failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — the overlay is mounted on its own with a fake room; the Fullscreen and Keyboard Lock APIs are stubbed on the test window.
**Concurrent with:** nodes 1 (after #626), 2, 3, 4
**Blocks:** nothing.

    #626 → n1      n1 → n6, n7, n8      n2 ⇢ n3 (signature only)      n2, n3, n6 → n8

## Affected Packages

- `tddy-web` — [README](../../../packages/tddy-web/README.md), [docs](../../../packages/tddy-web/docs/)

## Related Feature Documentation

- PRD: `docs/ft/web/1-WIP/PRD-2026-10-18-host-desktops-viewer-fullscreen.md`

## Summary

The desktop can take the whole screen, and keys the browser would otherwise keep reach it.

## Scope

- [ ] Generalised fullscreen helper; terminal callers renamed
- [ ] Keyboard Lock helper
- [ ] Overlay toggle, hint, cleanup

## Technical Changes

### State A
- `ScreenSharingOverlay.tsx:84` `fixed inset-0 z-50 …`; no `requestFullscreen`.
- `src/lib/browserFullscreen.ts` (110): `requestFullscreenForConnectedTerminal`, `exitDocumentFullscreen`, `isTargetInActiveFullscreen`; used by `ConnectionTerminalChrome.tsx:202`, `SessionRuntime.tsx:295,330`.
- Fullscreen precedent spec: `SessionTerminalFullscreenAcceptance.cy.tsx`.

### State B
The overlay's root element goes full screen on request; Keyboard Lock follows full screen state.

### Delta
| Package | Change |
|---|---|
| `tddy-web` | `src/lib/browserFullscreen.ts`, `src/lib/keyboardLock.ts` (new), `ScreenSharingOverlay.tsx`, terminal callers, `screenSharingOverlayPage.ts` |

## Implementation Milestones
- [ ] M1 Helpers + unit tests
- [ ] M2 Overlay toggle + Cypress acceptance

## Testing Plan

Cypress component tests mount `ScreenSharingOverlay` with a fake room (as the input-forwarding specs
do) and stub `Element.prototype.requestFullscreen`, `document.exitFullscreen` and
`navigator.keyboard` on the AUT window, dispatching `fullscreenchange` to simulate the browser.
Browser-reserved behaviour itself is not testable; the specs pin what tddy asks the browser to do.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `hands the desktop viewer to the fullscreen api and not the page` | `packages/tddy-web/cypress/component/ScreenSharingFullscreenAcceptance.cy.tsx` (new) | request target |
| `offers to exit full screen while full screen` | same | label + exit call |
| `locks the keyboard on entering full screen where the browser can` | same | `navigator.keyboard.lock()` |
| `unlocks the keyboard when the browser leaves full screen` | same | unlock on `fullscreenchange` |
| `says escape leaves full screen where the keyboard cannot be locked` | same | hint |
| `leaves full screen when the desktop is closed` | same | exit on close |

Unit: `src/lib/keyboardLock.test.ts` — `reports keyboard lock as unsupported where the browser has none`, `locks every key`; `src/lib/browserFullscreen.test.ts` — existing test renamed to `requestElementFullscreen`.

## Decisions & Trade-offs
- **Lock all keys** (`lock()` with no list) — the desktop expects every key; the closing chord is still caught by tddy before forwarding.

## Technical Debt & Production Readiness

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-host-desktops-viewer-fullscreen-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-host-desktops-viewer-fullscreen-initial-discovery.md`
- [ ] USER REVIEW — work complete
