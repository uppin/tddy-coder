# 2026-09-06 — the desktop app reaches its own host over an in-process bridge

**Type:** Feature

Node 7 of `optional-livekit`, and the one the stack existed for. Nodes 1–4 gave `tddy-web` a
provider registry, a source-merged host directory, session connections carrying capabilities, and
surfaces gated on them; node 6 gave the desktop app concurrent addressed IPC connections. Nothing was
wired: the only registered `ConnectionProvider` was LiveKit, so reaching a daemon meant being in a
common room with it, and `tddy-desktop` does not join one by default. The daemon serving the page was
already *named* in the directory by `hostDirectory/servingSource.ts` — selecting it simply resolved
no connection, and every screen rendered its "no connection to this host" state.

`createIpcConnectionProvider` (`src/rpc/connections/localHost.ts`) is that wire. It claims exactly
one host — `registration.daemonInstanceId`, `null` for every other — over node 6's addressed IPC, and
advertises `{"rpc"}` and nothing else. `LocalHostConnections`
(`src/rpc/connections/localHostRegistration.tsx`) registers it, following `LiveKitConnections`
exactly: `useConnectionProviders()`, the provider in a `useRef`, `register()` unconditionally every
render. It mounts above `SelectedDaemonProvider`, so `ipc` precedes `livekit` and wins for the host it
claims even where a common room could also reach that machine — the daemon is in this binary, and
a round trip through a media server to reach a roster already in the process is pure latency.
Precedence is how that is expressed; there is no `preferIpc` setting.

**The registration point is gated on the transport flavour, because there is no desktop entry
module.** `tauri.conf.json` sets `frontendDist: ../../tddy-web/dist`, so the Tauri shell loads the
browser's own bundle: one build, one entry. The plan assumed a desktop-only module and a structural
guarantee — "`tddy-web` imports neither" — that this repository's build layout cannot provide, and
`daemonTransport.ts` had already put a Tauri dependency in the browser bundle anyway. So
`localHostRegistrationFor(win, daemonInstanceId)` delegates to `daemonTransportFlavour(win)` and
returns `null` for a browser page. **The browser guarantee is behavioural, not structural**: the
module is in every build there is, and what keeps a browser off the IPC path is that nothing is ever
registered there. That is not a second question — `daemonTransportFlavour` already decides how this
page reaches its own daemon, and a host over IPC is available on the same terms; asking again in
different words would invent a way for the two answers to disagree. A second bundle with its own Vite
entry was the rejected alternative, at the cost of `tauri.conf.json`, `./install` and `./publish.sh`.

**The local host connection does not open a connection.** `RpcTransportProvider` builds the daemon
transport eagerly on first render, which on the desktop opens the `Daemon`-targeted bridge and
connects it — `webviewFramePipe` invokes `tddy_rpc_connect` as it is constructed — before any screen
renders. A host connection calling `openConnection(DAEMON_TARGET)` is handed *that same bridge*, the
page's registry memoising per target, and building a transport over it connects it again under the
same epoch. `MultiConnectionHost` answers `EpochInUse` and leaves the incumbent serving, so every
call through the local host failed; and because the bridge assigns `registration` whatever the last
`connect` returned, the shared daemon bridge came to hold the rejected promise, and its `close()` —
which skips `tddy_rpc_disconnect` when the registration never resolved — silently stopped releasing
the page's own connection. `LocalHostWiring.hostTransport` is the fix: the host connection **uses**
`useHttpTransport()` and never names `DAEMON_TARGET`. The bug was latent while each transport minted
its own epoch and became fatal the moment `tddy-tauri-web` made a bridge own one — two locally
reasonable designs meeting in the middle. `transportFor` now applies to **session** bridges only,
which `openConnection(sessionTarget(id))` genuinely mints fresh; neither half of the wiring has a
default, because a transport built without the page's auth gate would send stale credentials over a
connection it had opened rather than failing.

**`IpcSessionWire` counts attachments.** The host application keys bridges by target, so two
attachments of one session *are* one connection whatever anything above believes; what is chosen here
is that the host-side peer is released on the **last** detach. Releasing on the first takes the wire
out from under a screen still rendering the session (`useSessionAttachment` is per screen), never
releasing leaks a peer per session for the life of the page, and a second transport for the second
attachment is the `EpochInUse` failure one level down. Each attachment keeps its own `close()`, its
own terminal resume points, and `clientFor`/`transport` that throw once detached. `status` is
`connected` while attached and `idle` after; `error` is unreachable and therefore not modelled, since
a bridge's `closed` resolves for exactly one reason — the page releasing it.

The hint's `room`, `url` and `serverIdentity` are read by nobody on this path, and a session reached
this way advertises `{"rpc"}` whatever the daemon advertised about it. No room, participant, token or
LiveKit identity exists anywhere on the IPC path.

**Two pieces of the planned surface were built, measured, and removed rather than shipped.**
`createLocalHostDirectorySource` produced a descriptor byte-identical to
`useServingHostDirectorySource`'s — same `hostId`, same `` `<id> (this daemon)` `` label, same
`connected`, from the same `appConfig.daemonInstanceId` — and nothing in `packages/tddy-web/src` reads
`HostDescriptor.sourceId`, which `daemonHostOf` drops before any screen sees it. Removing it took this
PR **out of `selectedDaemon.tsx` entirely**, since that file was modified only to accept the
`hostSources` prop the source needed; that file belongs to node 2
([#438](https://github.com/uppin/tddy-coder/pull/438)), which is open, so this also closed live
conflict surface. `liveKitIsConfigured` went for the same reason: it never gained a production caller,
and the rule it stated is enforced by `useCommonRoom`'s own guard — an exported predicate nothing
calls is not a statement of a rule, it is a second place for one to drift.

The test double is part of the change. An optimistic `WebviewIpcBridge` double hid the `EpochInUse`
bug completely and hid a leaked host-side peer besides: the real `close()` skips
`tddy_rpc_disconnect` when the registration never resolved, so a bridge whose second `connect` was
refused is released *page-side* while its peer survives, and a double reporting page-side intent stays
green in exactly that world. It now keeps `registration` as well as `released`, exposes
`hostWasToldToDisconnect()` distinct from `wasReleased()`, refuses a second `connect` by replacing the
registration with the rejected promise, deletes its registry entry on release, resolves `closed`, and
keeps its factory separate from its inspectors. Both regressions are pinned by mutation.

**Known limitation, recorded rather than fixed:** node 4 predicted that the first mixed fleet must
revisit the fleet-wide/host-scoped split in `useCapabilityAvailability`. This is that fleet, and
capability gating is node 4's surface. It is narrower than feared — the unconfigured desktop is
already correct, since a host that never joins a room reports `idle` and never `connecting` — but with
LiveKit configured and a join in flight, the local IPC host briefly reads `connecting` for media
rather than `unavailable`. Transient and cosmetic; it settles when the join resolves.

No proto change, no daemon change, no change to `tddy-tauri-web` or `tddy-tauri-rpc`, no new npm or
Rust dependency, and zero Rust files touched. Tests: 1096 unit passing;
`DesktopIpcHostAcceptance.cy.tsx` 12 passing, every one resolving through the real
`ConnectionProviderRegistry` rather than a local restatement of its precedence rule;
`localHost.test.ts` 19. The `tddy-desktop` e2e fails 1 of 2 — and fails **identically on this node's
base** at `b1524872` with none of this code present, measured rather than assumed, so it is inherited
and not a regression here; it is outside the CI gate, which is what let it go unreported. The manual
`./desktop-dev` run in both LiveKit configurations needs an operator at a GUI and is deferred to
review.

Technical [local-host-ipc.md](../local-host-ipc.md), feature
[tddy-desktop-tauri.md](../../../../docs/ft/desktop/tddy-desktop-tauri.md),
[daemon-selector-livekit-rpc.md](../../../../docs/ft/web/daemon-selector-livekit-rpc.md). PR
[#443](https://github.com/uppin/tddy-coder/pull/443).
