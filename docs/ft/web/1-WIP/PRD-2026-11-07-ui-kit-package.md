# UI kit package (`tddy-ui-web`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 3/16 — branch `feature/web-split/ui`

## Affected Features

- **Primary**: [app-shell.md](../app-shell.md) — the shared look (theme, primitives) moves to a package.
- **Related**: every screen, since all of them render these primitives.

## Summary

Move the shared presentational foundation out of `tddy-web` into `packages/tddy-ui-web`: the shadcn
primitives (`components/ui`), `Button`, the toast, the `cn` class helper (`lib/utils`), the theme CSS
(tailwind tokens, fonts), and the small pure helpers and hooks that more than one future package needs
and that import neither generated RPC code nor a transport (e.g. `useIsMobile`, `useVisualViewport`).

## Background

Every component package the stack creates renders these primitives. If they stay in `tddy-web`, every
package would depend on the app it is supposed to be split out of. They have no RPC and no LiveKit
reach, so they are the cleanest first cut and unblock almost every later node.

## Proposed Changes

### What's Changing

- New workspace package `tddy-ui-web`; `tddy-web` imports from it instead of `@/components/ui` and
  `@/lib/utils`.
- The primitives' Cypress component specs move with them and run from the package's own Cypress
  config; its own PR check `web-tddy-ui-web.yml`.
- A package-boundary rule: `tddy-ui-web` imports nothing from `tddy-web`, generated RPC code, or
  `livekit-client`.

### What's Staying the Same

- Every screen looks and behaves exactly as before. No component API changes beyond import paths.

## Impact Analysis

### Technical Impact

- Import rewrites across `tddy-web`; the `@/` alias stays for code still inside `tddy-web`.
- Tailwind must scan the package's sources so its classes are still generated.

### User Impact

None.

## Implementation Plan

1. Package skeleton + Cypress config mirroring `tddy-web`'s (React Compiler switch kept identical).
2. Move the modules and their specs; rewrite imports; tailwind source scan.
3. Workflow; boundary rule test.

## Acceptance Criteria

- [ ] The primitives, theme and shared pure helpers live in `tddy-ui-web`.
- [ ] Their specs pass from the package's own Cypress config and PR check.
- [ ] `tddy-ui-web` has no import of `tddy-web`, generated code or `livekit-client`.
- [ ] `tddy-web` builds and its remaining specs pass unchanged.

## References

- Real dependency: `feature/web-split/web-ci` (per-package workflow). Branch predecessor:
  `feature/web-split/react-commons` (line order only — nothing consumed from it).
- Successors: every component package, and `feature/web-split/livekit`.
