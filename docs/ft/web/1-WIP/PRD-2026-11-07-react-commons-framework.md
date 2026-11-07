# Composable React framework (`tddy-react-commons`) - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 2/16 — branch `feature/web-split/react-commons`

## Affected Features

- **Primary**: Web app architecture — [app-shell.md](../app-shell.md) (the app becomes a composition of
  packages wired through this framework, in later nodes).
- **Related**: every component package the stack creates consumes it at its seams.

## Summary

Add `packages/tddy-react-commons`, a small composable React framework, as a workspace package:

- **events** — typed component events, event adapters and observer hooks, `withEvents`;
- **stores** — rxjs-backed stores with a `useStore` hook and structural selector comparison;
- **container** — an app container that composes handlers and slices, React-free handlers, context
  and slice hooks;
- **ports** — typed port declarations, so a component package states what it needs from the app and
  the app supplies the implementation;
- **async** — request helpers, a poller (refresh, visibility pause, equality), command errors,
  app-scope resource factories;
- **testkit** — a container testkit, async doubles, an app-scope double.

## Background

`tddy-web` is one 110k-line package where components reach transports, LiveKit and each other
directly. The stack splits it into component packages that know only their own components and the
RPC interfaces they consume. That needs a shared way to **declare** a dependency (a port), **compose**
packages into an app (a container), and **communicate** across a package boundary (typed events and
stores) — without each package inventing its own context plumbing. This node delivers that framework
on its own, with its tests, before any package uses it.

## Proposed Changes

### What's Changing

- New workspace package `tddy-react-commons` with subpath exports `.`, `./events`, `./stores`,
  `./container`, `./ports`, `./async`, `./testkit`.
- Targets this repo's stack: **React 19** (peer), TypeScript ESM, resolved from source like the other
  workspace packages.
- Its unit tests run on **`bun test`** (with the repo's DOM preload approach), not on a separate test
  runner.
- New dependencies, installed through the local registry: `rxjs`, `dequal`.
- Its own PR check, `web-tddy-react-commons.yml`, from the per-package workflow (`#web-split` 1/16).
- Package README and `docs/` describing each module.

### What's Staying the Same

- No existing package uses it yet — `tddy-web`'s behaviour is untouched. Adoption happens at each
  package's seam in later nodes; existing hooks and state are **not** rewritten onto its stores.

## Impact Analysis

### Technical Impact

- Two new runtime dependencies (`rxjs`, `dequal`) — approved by the developer at the interview.
- Tests that rely on DOM rendering need a DOM in `bun test`; the package reuses the workspace's
  preload pattern rather than adding a new runner.

### User Impact

None directly — no screen changes.

## Implementation Plan

1. Package skeleton, exports, tsconfig, bunfig, README/docs.
2. Each module with its tests, adapted to React 19 and `bun test`.
3. The package's workflow; register it in the workspace.

## Acceptance Criteria

- [ ] `tddy-react-commons` is a workspace package with the seven subpath exports.
- [ ] Each module's behaviour is covered by `bun test` suites that pass in its own PR check.
- [ ] It builds and runs against React 19.
- [ ] No other package's behaviour changes.

## References

- Predecessor: `feature/web-split/web-ci` (per-package workflow).
- Successors: `feature/web-split/clients` (declares `ClientsPort` with this package's ports).
