# 2026-11-08 — The app handler builds no clients or auth port

**Category:** Future enhancement (design narrowed at green)
**Source:** `#web-split` 20/20 (PR #853), `packages/tddy-web/src/app/ports.ts` (`TODO(wiring)`)

`appHandler` builds the React-free ports — navigation, presence, media, room RPC, native file drop.
The clients and auth ports are optional on `AppPorts` and, in the real app, are still built by the
stateful providers in the tree (`RpcTransportProvider`, `ConnectionProviders`, `AuthProvider`,
`SelectedDaemonProvider`): the HTTP/Tauri transport, the connection registry, the selected-daemon
directory and `useAuth` are hooks over React state. A handler that supplies both (the specs' doubles)
replaces those providers; supplying only one throws.

**Why not now:** building them in the handler means rewriting those sources as React-free services —
a change to the transport and auth layers, not to the composition this node delivers.

**What closing it takes:** turn the transport, connection registry, selected-daemon directory and
auth session into services the handler starts and tears down, make `clients` and `auth` required on
`AppPorts`, and delete the in-tree provider branch of `AppTree` (`app/composeApp.tsx`).
