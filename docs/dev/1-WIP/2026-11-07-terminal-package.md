# Changeset: Terminal package (`tddy-terminal-web`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — two packages (new package; `tddy-web` sessions/chat/prstack consumers), ~35 specs + drivers moved, two inherited oversized-file records, `ghostty-web` dependency moves

**Stack**: `#web-split` 8/16 — branch `feature/web-split/terminal`, base `feature/web-split/create-session`

**PR**: [#845](https://github.com/uppin/tddy-coder/pull/845)

## Initial Discovery

[2026-11-07-terminal-package-initial-discovery.md](./2026-11-07-terminal-package-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) and `packages/*/docs/code-issues/` this change runs into.

### ⚠ DURING — oversized `GhosttyTerminal.tsx` / `GhosttyTerminalSession.tsx` — `packages/tddy-web/docs/code-issues/oversized-file-ghostty-terminal{,-session}.md`

Move with the files; recorded, not fixed here.

### ⚠ DURING — host terminal feed leftovers after snapshot — [`2026-10-25-web-host-terminal-feed-leftovers-after-snapshot.md`](../todo/2026-10-25-web-host-terminal-feed-leftovers-after-snapshot.md)

The feed code moves; behaviour unchanged; the entry's paths are updated.

## Affected Packages

- **tddy-terminal-web** (new): `packages/tddy-terminal-web/` — sources, specs, drivers (`ghosttyTerminalDriver`, `ghosttyTerminalSessionDriver`, `terminalHistoryPagingDriver`, `wheelTuiSessionDriver`), README, `docs/`, `docs/code-issues/oversized-file-ghostty-terminal{,-session}.md` (moved).
- **tddy-web**: loses `components/GhosttyTerminal{,Session}.tsx`, `components/connection/*` (terminal chrome; `StandaloneConnectionScreen` stays as wiring), `rpc/connections/{terminal,terminalFeed,inputDelivery}`, terminal-control / enqueued-input hooks, `lib/{terminal*,osc52,ghosttyInstance,toolShortcuts,keyboardLock,fileUploadChunks}` (final list by consumer count); `docs/terminal-*.md`, `web-terminal` docs at wrap.
- **root** `workspaces`, `bun.lock`; `.github/workflows/web-tddy-terminal-web.yml`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-terminal-package.md](../../ft/web/1-WIP/PRD-2026-11-07-terminal-package.md)

## Summary

Move the browser terminal and its chrome, feed and input delivery into its own package.

## Background

See the PRD. Package rules shared by every `#web-split` package node: the package imports nothing from
`tddy-web` and nothing from `livekit-client` (only `tddy-livekit-web` / `tddy-livekit-rpc-web` may);
it consumes the outside world only through `tddy-client-web` (generated clients and ports),
`tddy-ui-web` and `tddy-react-commons`; its Cypress component specs, page objects and drivers live in
the package and run from its own `cypress.config.ts` (React Compiler switch identical to `tddy-web`'s);
it has its own `web-<package>.yml` PR check; files move with `git mv` so history follows; code-issue
records move with their files.

## Responsibility

- `tddy-terminal-web` exports `GhosttyTerminal`, `GhosttyTerminalSession`, `ConnectionTerminalChrome`, `TerminalConnectionStatusBar`, `ShortcutDrawer`, `MobileTerminalKeyboard`, `TerminalFileDropZone`, `TerminalUploadButton`, `EnqueuedInputOverlay`, the terminal feed/input API (`openTerminalFeed`, `deliverInput`, `useTerminalControl`, `useEnqueuedInput`), and the terminal helpers other packages use.
- The moved specs (`Ghostty*`, `Terminal*`, `BrowserTerminalShortcuts`, `ConnectionTerminalChrome`, `EnqueuedInputOverlayAcceptance`, `MobileTerminalUploadButton`, `ShortcutDrawer` — ~35) and drivers.
- Boundary rule, workflow, the moved code-issue records.

## Boundaries

- Does not move `GrpcSessionTerminal`, the session terminal tabs or the session runtime registry (`feature/web-split/session`) — they consume this package.
- Does not move the LiveKit room terminal feed (`tddy-livekit-web`, #843); the feed API here is transport-agnostic.
- The e2e terminal specs stay in `tddy-web` (they run against the Storybook build).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `web-package.yml` | adds its workflow | change it |
| `n3` ui (#840) | primitives, `useIsMobile`, `useVisualViewport`, `clipboard` | renders with them | move them |
| `n5` clients (#842) | generated `terminal_session_pb`, `ClientsPort` hooks, `mountWithClients` | the feed and input delivery call the terminal RPC through them | add a transport |
| `n6`, `n7` (#843, create-session) | — (branch predecessors only) | nothing | — |

## Draft PR contract

**As pushed (commit 2):** package skeleton (`ghostty-web` dependency), workflow, boundary rule, and **21**
specs `git mv`'d (the `Ghostty*`/`Terminal*`/chrome specs that mount only terminal code). Nine terminal-named
specs that mount session, chat or host components (session terminal tabs, room terminal, file drop into a
session) stay in `tddy-web` and travel with `feature/web-split/session`. The terminal **types**
(`TerminalFeed`, `TerminalOptions`, `TerminalFrame`, …) already moved to `tddy-client-web/connections/terminal`
in #842's contract; this node owns the feed and input-delivery **implementations**.

Originally planned:

First push (commit 2): the package skeleton (manifest exporting the surface above, Cypress/Vite config with `ghostty-web`'s WASM handling as `tddy-web` has it, workflow), the ~35 specs and drivers `git mv`'d with imports pointing at the package, and the boundary rule — failing until the modules move.

## Green wave

**Wave:** 4 of 9
**Greenable independently:** yes — terminal specs mount the terminal against in-memory terminal RPC; nothing from n6/n7
**Concurrent with:** `livekit` (#843), `create-session`, `playground`
**Blocks:** `session` (terminal tabs, dormant agent tabs), `storybook` (terminal stories)

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] Package skeleton + workflow (`ghostty-web` dependency)
- [ ] Modules moved; consumers' imports rewritten
- [ ] Specs and drivers moved and green from the package config
- [ ] Boundary rule
- [ ] Code-issue records moved

## Technical Changes

### State A (Current)

`GhosttyTerminal.tsx` (1491 lines) and `GhosttyTerminalSession.tsx` (1005), both recorded oversized; terminal chrome in `components/connection/` (8 components + presentation helpers); feed and input delivery in `rpc/connections/{terminal,terminalFeed,inputDelivery}.ts` (transport-agnostic, terminal RPC); ~25 `lib/terminal*` helpers. `GhosttyTerminalSession` imports `sessions/sessionRuntimeRegistry` once and `lib/liveKitStatusPresentation` once — both seams to cut (a prop / a status type) in pass 2.

### State B (Target)

`tddy-terminal-web` owns the terminal; sessions/chat/prstack import it.

### Delta

- `git mv` of modules, specs, drivers; import rewrites; the two upward reaches replaced by props.

## Implementation Milestones

- [ ] Package specs green
- [ ] `tddy-web` builds; session terminal tab specs still green

## Testing Plan

**Level:** Cypress component from the package config (existing assertions), `bun test` for the pure helpers' suites, boundary rule.

## Acceptance Tests

### tddy-terminal-web

- The ~35 moved specs (`GhosttyTerminal*.cy.tsx`, `Terminal*.cy.tsx`, `BrowserTerminalShortcuts`, `ConnectionTerminalChrome`, `EnqueuedInputOverlayAcceptance`, `MobileTerminalUploadButton`, `ShortcutDrawer`) — unchanged assertions.
- Moved `bun test` suites for `lib/terminal*`, `osc52`, `connection/*Presentation`.
- `src/packageBoundary.test.ts` — `tddy-terminal-web imports nothing from tddy-web or livekit-client`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **`GrpcSessionTerminal` stays with sessions**: it binds the terminal to the session runtime registry, which is session state.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-terminal-package-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`tddy-terminal-web` Cypress + `bun test` + boundary rule; `tddy-web` build + session terminal tab specs) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
