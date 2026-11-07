# tddy-react-commons — technical notes

- **Events** (`src/events`): a component receives `eventObserver` and emits envelopes
  `{ type, payload }`; parents observe with `useEventObserver` or adapt a child's events into their
  own with `useAdaptedEventObserver`. Subscription happens before the first commit, so an event
  emitted during mount is observed.
- **Stores** (`src/stores`): `createStore(initializer)` is an rxjs `Observable` + `Observer` with
  `getState`/`setState`; `useStore(store, selector)` re-renders only when the selected value changes
  structurally.
- **Container** (`src/container`): an `AppHandler` has one member, `start()`, returning the context
  and a teardown. `ReactAppContainer.create` starts it on mount and tears it down on unmount.
- **Ports** (`src/ports`): `withPorts(handler, ports)` binds a port map to a handler; `PortsOf<H>`
  reads a handler's required ports. The web packages declare their app-facing dependencies this way.
- **Async** (`src/async`): `createRequest`, `createPoller` and `createCommandErrorChannel`, all
  registered on an `AppScope` that disposes them together.
- **Testkit** (`src/testkit`): doubles with the same shapes as the real objects; `actionObserver`
  takes an action factory (e.g. Storybook's `action`) as a parameter, so the package never depends on
  Storybook.
