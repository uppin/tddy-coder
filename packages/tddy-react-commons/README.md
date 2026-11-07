# tddy-react-commons

A small composable React framework the web packages use at their seams.

| Subpath | What it gives you |
|---|---|
| `tddy-react-commons/events` | Typed component events: an `eventObserver` prop, emitters, `withEvents`, observer and adapter hooks |
| `tddy-react-commons/stores` | rxjs-backed stores (`createStore`), bound slices, `useStore`, structural equality |
| `tddy-react-commons/container` | `ReactAppContainer.create(handler, tree)` — runs React-free app logic and hands its context to the tree; `useAppContext`, `createSliceHook` |
| `tddy-react-commons/ports` | `withPorts(handler, ports)` — an app handler declares what it needs from outside as a typed port map |
| `tddy-react-commons/async` | Request state, a poller (refresh, pause while hidden, equal results do not re-render), command errors, an app scope that disposes what it created |
| `tddy-react-commons/testkit` | Doubles for all of the above: `stubStore`, `eventRecorder`, `stubPorts`, `deferred`, `stubScope`, `AppContextStub` |

React 19 is a peer dependency. Tests run on `bun test` with a happy-dom preload
(`test-utils/dom-preload.ts`): `bun test` from this directory.

See [docs/](docs/) for each module.
