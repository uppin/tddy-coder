# Changeset: Composable React framework (`tddy-react-commons`)

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 3 — one new package, ~45 public symbols over six modules, DOM-rendering unit tests on `bun test` (new happy-dom preload), two runtime deps

**Stack**: `#web-split` 2/16 — branch `feature/web-split/react-commons`, base `feature/web-split/web-ci` (#838)

**PR**: [#839](https://github.com/uppin/tddy-coder/pull/839)

## Initial Discovery

[2026-11-07-react-commons-framework-initial-discovery.md](./2026-11-07-react-commons-framework-initial-discovery.md)

## Affected Packages

- **tddy-react-commons** (new): `packages/tddy-react-commons/` — README, `docs/`, sources, tests.
- **root** `package.json` (`workspaces`), `bun.lock`.
- **CI**: `.github/workflows/web-tddy-react-commons.yml` (calls `web-package.yml` from #838).

## Related Feature Documentation

- PRD: [PRD-2026-11-07-react-commons-framework.md](../../ft/web/1-WIP/PRD-2026-11-07-react-commons-framework.md)

## Summary

Add a composable React framework package — events, stores, container, ports, async, testkit — that
later nodes use to declare and wire package seams.

## Background

See the PRD. Imported from a framework the developer supplied, adapted to this repo (React 19,
`bun test`, local registry). **The origin is not named anywhere** — not in code, docs, commit or PR.

## Responsibility

Every export of `tddy-react-commons`, by subpath:

| Subpath | Exports |
|---|---|
| `./events` | `EventMap`, `EventMapKeys`, `Envelope`, `EventedProps`, `EventEmitters`, `ChildEvents`, `RC`, `useEventObserver`, `createChildEvents`, `withEvents`, `useAdaptedEventObserver` |
| `./stores` | `structurallyEqual`, `SetState`, `Store`, `createStore`, `StoreAction`, `BoundActions`, `SliceHandle`, `bindSlice`, `useStore` |
| `./container` | `AppHandle`, `AppHandler`, `useAppContext`, `ReactAppContainer`, `SliceHookConfig`, `createSliceHook` |
| `./ports` | `PortMap`, `PortedAppHandler`, `PortedAppHandle`, `PortsOf`, `ContextOf`, `withPorts`, `AppScope` (re-export) |
| `./async` | `RequestState`, `isReady`, `isPending`, `Request`, `RequestOptions`, `createRequest`, `CommandError`, `CommandErrorChannel`, `createCommandErrorChannel`, `AppScope`, `ScopedAppResources`, `createAppScope`, poller API |
| `./testkit` | `AppContextStub`, `StubStore`, `stubStore`, `EventRecorder`, `eventRecorder`, `emittersFor`, `eventDriver`, `ActionFactory`, `actionObserver`, `stubPorts`, `Deferred`, `deferred`, `ScopeRegistrationKind`, `StubScope`, `stubScope` |

Plus: the package's `bunfig.toml` + happy-dom preload, its workflow, README and `docs/`.

## Boundaries

- No consumer: nothing in `tddy-web` or any other package imports it in this PR.
- No example app, no stories (stories belong to `feature/web-split/storybook`).
- No Storybook coupling in the testkit: `actionObserver` takes an `ActionFactory`; it does not import
  a Storybook addon.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` web-ci (#838) | `.github/workflows/web-package.yml` (reusable, input `package`) and the workspace ↔ workflow rule test | adds `web-tddy-react-commons.yml` calling it; the rule test then requires that file | change the shared workflow, the detection script or the rule |

## Draft PR contract

First push (commit 2): the package skeleton (`package.json` with the seven subpath exports, tsconfig,
`bunfig.toml`, happy-dom preload, workflow file), every exported type in full, every exported function
with its signature and a `// TODO(react-commons): implement` body, and the ported test suites —
failing because the bodies are missing.

## Green wave

**Wave:** 2 of 9
**Greenable independently:** yes — the suites exercise only this package; the workflow file needs only
#838's published shared workflow (present on the base).
**Concurrent with:** `feature/web-split/ui`, `feature/web-split/livekit-rpc`
**Blocks:** `feature/web-split/clients` (declares `ClientsPort` with `./ports`), and through it every
component package.

    n1 → n2      n2 → n5 → n6, n7, n8, n9 → …

## Scope

- [ ] Package skeleton, exports, tsconfig, happy-dom preload
- [ ] events, stores, container, ports, async, testkit — implemented, React 19
- [ ] Test suites ported to `bun test`
- [ ] `web-tddy-react-commons.yml`
- [ ] README + `docs/` per module

## Technical Changes

### State A (Current)

No such package. The workspace has no DOM test environment for `bun test` (`tddy-web`'s preload
polyfills `KeyboardEvent` only). `@testing-library/dom` is in `bun.lock` transitively (Storybook);
`@testing-library/react`, `happy-dom`, `rxjs`, `dequal` are not direct deps of anything.

### State B (Target)

`packages/tddy-react-commons` with the exports above, ESM, `react`/`react-dom` `^19` as peers, deps
`rxjs`, `dequal`; devDeps `@happy-dom/global-registrator`, `@testing-library/react`, `react`,
`react-dom`, `@types/react`. `bun test` with a preload registering happy-dom and setting
testing-library's async budget. `test`/`test:ci` scripts (JUnit to `reports/junit/react-commons.xml`).
Consumers resolve it from source (like the other workspace packages), so `exports` map to `src/`.

### Delta

- New package (sources + tests + docs); root `workspaces` entry; `bun.lock` (via
  `bun run local-registry-install`); one workflow file.

## Implementation Milestones

- [ ] Skeleton resolves from another workspace package by subpath
- [ ] Each module's suite green
- [ ] Workflow present; rule test green

## Testing Plan

**Level:** unit, in-memory, with a DOM for hook/component suites. The framework has no I/O.
Suites keep their behaviour-level names (subscription timing, poller refresh/visibility/equality,
app-scope disposal, slice hooks, typed emitters, testkit doubles).

## Acceptance Tests

### tddy-react-commons (`cd packages/tddy-react-commons && ../../dev bun test`)

- `src/events/*.test.tsx` — typed emitters, component events, `withEvents`, observer hooks, event
  adapter, subscription timing (an event emitted during the first commit is observed).
- `src/stores/*.test.ts[x]` — `createStore` set/get/subscribe, structural selector comparison, `useStore`.
- `src/container/*.test.ts[x]` — app container composes handlers, `useAppContext` outside a container
  throws, slice hooks, React-free handlers.
- `src/ports/ports.test.ts` — `withPorts` hands declared ports to the handler; missing port is a type error.
- `src/async/*.test.ts[x]` — request states, poller start/stop/refresh, pause while hidden, equal
  results do not re-render, command error channel, app scope disposes everything it created.
- `src/testkit/*.test.ts[x]` — `stubStore`, `eventRecorder`, `stubPorts`, `deferred`, `stubScope`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **happy-dom over jsdom / Cypress** (developer's choice): keeps every suite on `bun test`.
- **React 19 peer**: the source targeted React 18; the APIs used (`useSyncExternalStore`, context,
  refs) are unchanged in 19. Any suite relying on React 18-only behaviour is adapted, not skipped.
- **Testkit decoupled from Storybook** — keeps the package free of Storybook deps.
- **Test port, recorded at the contract:** `vitest` imports → `bun:test` (its `vi.fn`, `vi.spyOn`,
  `vi.restoreAllMocks`, `vi.useFakeTimers` work as-is); `vi.advanceTimersByTimeAsync(n)` → `vi.advanceTimersByTime(n)`
  followed by a microtask flush (bun 1.3 has no async variant); the global `JSX.Element` → React 19's
  `JSX` from `react`. The contract run: 149 failing on `TODO(react-commons)`, 5 type-level tests passing;
  the same suites pass 176/176 against the intended implementation.
- **Lockfile without a public registry:** `bun install --lockfile-only --registry <mirror>`, then the
  mirror's tarball URLs stripped back to `""` (how `bun.lock` records the default registry) — integrity
  hashes are the registry's own, so `--frozen-lockfile` on CI resolves the same packages.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-react-commons-framework-initial-discovery.md`)
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
- [ ] Run the package's tests (`bun test` in `packages/tddy-react-commons`) — verify 100% pass
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
