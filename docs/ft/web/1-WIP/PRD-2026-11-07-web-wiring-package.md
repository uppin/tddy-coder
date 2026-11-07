# `tddy-web` as the wiring package - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 16/16 — branch `feature/web-split/wiring`

## Affected Features

- **Primary**: [app-shell.md](../app-shell.md), [local-web-dev.md](../local-web-dev.md), [url-state-routing.md](../url-state-routing.md).

## Summary

Finish the split: `tddy-web` holds only wiring — the app entry, the app container composing every package's handler with the framework's container, the port implementations (HTTP/Tauri transports, the LiveKit ports from `tddy-livekit-web`, URL-backed navigation, auth), the app shell, routes and `*AppPage` pages, and the e2e specs. A rule test fails if a feature component appears in `tddy-web` again.

## Background

After `session` lands, what remains in `tddy-web` is composition plus leftovers (root `components/`, `hooks/`, `lib/`, `utils/`, `test-utils/`) that never found a home. This node moves the leftovers to their owners, composes the app through the framework's container, and pins the rule.

## Proposed Changes

### What's Changing

- App composition through the container; leftover modules moved to owning packages; the wiring rule test; `tddy-web`'s own PR check covers its build, its remaining specs and e2e.

### What's Staying the Same

- App behaviour, routes, the desktop bundle.

## Impact Analysis

### Technical Impact

Files move with `git mv` (history follows); imports are rewritten; the package's Cypress specs, page
objects and drivers move with it and run from its own config and PR check. The package imports nothing
from `tddy-web` and nothing from `livekit-client` — it uses `tddy-client-web` ports, `tddy-ui-web` and
`tddy-react-commons`.

### User Impact

None — no screen or behaviour changes.

## Acceptance Criteria

- [ ] `tddy-web` contains no feature component (rule test).
- [ ] The app is composed from package handlers through the container.
- [ ] Build, e2e and remaining specs pass in its PR check.

## References

- Predecessors (real): every other node.
