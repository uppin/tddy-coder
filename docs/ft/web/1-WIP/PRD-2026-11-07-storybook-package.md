# Storybook package (`tddy-storybook`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 15/16 — branch `feature/web-split/storybook`

## Affected Features

- **Primary**: component documentation, and the Storybook-based e2e terminal specs ([web-terminal.md](../web-terminal.md)).

## Summary

Create `packages/tddy-storybook`: the one Storybook for the web packages. It owns **every story** — the five that exist today move in — and adds at least one story per component package for its main exported screens/components, rendered against port doubles. It depends on the component packages, never the other way. `tddy-web`'s e2e terminal specs run against this package's static build.

## Background

Stories live inside `tddy-web` today and only five exist. With the components split into packages, one Storybook that depends on all of them is the developer's chosen home for every story.

## Proposed Changes

### What's Changing

- New package with Storybook 9 config, stories, and a story-render smoke test in its PR check; the root `storybook` script and `tddy-web`'s e2e scripts point at it.

### What's Staying the Same

- The terminal stories the e2e specs use keep their ids.

## Impact Analysis

### Technical Impact

Files move with `git mv` (history follows); imports are rewritten; the package's Cypress specs, page
objects and drivers move with it and run from its own config and PR check. The package imports nothing
from `tddy-web` and nothing from `livekit-client` — it uses `tddy-client-web` ports, `tddy-ui-web` and
`tddy-react-commons`.

### User Impact

None — no screen or behaviour changes.

## Acceptance Criteria

- [ ] Every story lives in `tddy-storybook`; no component package contains a story.
- [ ] Each component package has at least one story.
- [ ] The Storybook builds in its PR check and the e2e terminal specs pass against it.

## References

- Predecessors (real): every component package.
