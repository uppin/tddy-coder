# Composable React framework (`tddy-react-commons`)

**Package:** `packages/tddy-react-commons`
**Applies to:** every web component package

## Overview

The web is split into component packages that know only their own components and the RPC interfaces
they consume. `tddy-react-commons` supplies the shared way to **declare** a dependency (a port),
**compose** packages into an app (a container) and **communicate** across a package boundary (typed
events and stores), so no package invents its own context plumbing.

## Modules

Imported by subpath; the package root exports nothing.

| Subpath | Provides |
|---|---|
| `./events` | typed component events: an `eventObserver` prop, emitters, `withEvents`, observer and adapter hooks |
| `./stores` | rxjs-backed stores (`createStore`), bound slices, `store.useStore(selector?, equality?)`, `structurallyEqual` |
| `./container` | `ReactAppContainer`, React-free `AppHandler`s, `useAppContext`, `createSliceHook` |
| `./ports` | `withPorts(handler, ports)`, `PortsOf`, `ContextOf` — a handler states what it needs from the app |
| `./async` | `createRequest`, `createPoller` (refresh, pause while hidden, equal results do not re-render), command errors, `createAppScope` |
| `./testkit` | doubles for all of the above: `stubStore`, `eventRecorder`, `stubPorts`, `deferred`, `stubScope`, `AppContextStub` |

## Constraints

- React 19 is a peer dependency; runtime dependencies are `rxjs` and `dequal`.
- The package is resolved from source, like the other workspace packages.
- Unit tests run on `bun test` with a happy-dom preload.
- The testkit does not depend on Storybook: `actionObserver` takes an action factory as a parameter.
- Adoption happens at each package's seam; existing `tddy-web` hooks and state are not rewritten
  onto its stores.

## Acceptance

- The package has the six module subpaths, each covered by `bun test` suites that run in the `Web tests`
  shards whenever a change reaches the package.
- It runs against React 19.
- No other package's behaviour changes.
