# tddy-web

The wiring package of the tddy web app. It holds no feature: it composes the component packages
(`tddy-session-web`, `tddy-hosts-web`, `tddy-workspace-web`, `tddy-prstack-web`, `tddy-terminal-web`,
`tddy-chat-web`, `tddy-session-create-web`, `tddy-rpc-playground-web`, `tddy-livekit-web`, over
`tddy-client-web` and `tddy-ui-web`) into the page `tddy-daemon` serves and `tddy-desktop` embeds.

| Path | Holds |
|------|-------|
| `src/index.tsx` | the entry: `composeApp(appHandler({ window }))` into `#root` |
| `src/app/composeApp.tsx` | `appHandler(environment)` builds the ports (URL navigation, LiveKit presence/media/room RPC, the window's native file drop); `composeApp(handler)` is `ReactAppContainer.create(handler, tree)` |
| `src/app/ports.ts` | `AppPorts`, and the scope the stateful providers read them from |
| `src/app/App.tsx` | the routes: config load, sign-in gate, `*AppPage` per path |
| `src/app/rpc/`, `src/app/auth/` | the port implementations that are still React state: HTTP/Tauri transports, connection registry, selected-daemon directory, `useAuth` |
| `src/routing/` | the URL grammar of this app (`appRoutes`, remembered host selection) |
| `src/components/shell/` | the application shell and daemon menu |
| `src/**/*AppPage.tsx` | pages placing a package's screen in the shell |
| `cypress/` | app, shell, routing and cross-package specs; `e2e/` runs against `tddy-storybook`'s static build |

`src/wiringBoundary.test.ts` keeps it that way: any other module under `src/` fails it.

**Not yet through the container:** the `clients` and `auth` ports. They are built by the providers in the
tree (`RpcTransportProvider`, `ConnectionProviders`, `AuthProvider`, `SelectedDaemonProvider`) because the
transport, connection wires, selected-daemon directory and `useAuth` are hooks over React state. A handler
that supplies `clients` and `auth` (the specs' port doubles) takes those providers' place. Recorded in
`docs/dev/todo/2026-11-08-app-handler-builds-no-clients-or-auth-port.md`.

```bash
./dev bun test src                      # unit + wiring rule
./dev bun run build                     # vite bundle (tddy-daemon serves dist/)
./dev bun run cypress:component         # component specs
```

Documentation: [docs/README.md](docs/README.md).
