# The local host over IPC (`src/rpc/connections/localHost.ts`)

Inside the Tauri desktop app the daemon runs in the *same process* as the webview host. Until this
existed, `tddy-web` could not talk to it as a **host**: the only registered
[connection provider](host-connections.md) was LiveKit, so reaching a daemon meant being in a common
room with it, and `tddy-desktop` does not join one by default. The daemon serving the page was
already *named* in the [host directory](host-directory.md) — `hostDirectory/servingSource.ts` has
contributed it from `daemonInstanceId` since node 2 — but selecting it resolved no connection and
every screen rendered its "no connection to this host" state.

`createIpcConnectionProvider` is the wire that closes that gap. It reaches exactly one host, over the
[addressed IPC connections](../../tddy-desktop/docs/webview-ipc-connections.md) the host application
already holds, and it advertises `{"rpc"}` and nothing else.

Feature docs: [Tddy desktop app](../../../docs/ft/desktop/tddy-desktop-tauri.md),
[Daemon selector + host-connection routing](../../../docs/ft/web/daemon-selector-livekit-rpc.md).

## It contributes a wire, not an entry

The provider is the whole of this module's contribution to the directory: **nothing**. A directory
source naming the local host was built and removed, because it produced a descriptor byte-identical
to the serving source's — same `hostId`, same `` `<id> (this daemon)` `` label, same `connected`
status, from the same `appConfig.daemonInstanceId` — and the only field that differed, `sourceId`, is
dropped by `daemonHostOf` before any screen sees it. Two sources for one machine, distinguishable by
nothing a consumer reads, is a second place for one fact to drift.

What was missing was never the entry. It was a way to reach it.

## Why the local host stays off the media server

Precedence is registration order, first match wins ([host connections](host-connections.md)), and
this is the case it exists for. `LocalHostConnections` mounts **above** `SelectedDaemonProvider`, and
a parent renders before its children, so `ipc` registers ahead of `livekit` and wins for the one host
it claims — even where a common room is configured and could also reach that machine.

That is not a preference, it is a fact about the topology: the daemon is in this binary, and reaching
it through a media server is a round trip out of the machine and back to a roster already in the
process. Expressing it as order is what keeps a `preferIpc` setting from existing.

The same machine is therefore **media-capable from a browser and not from its own desktop app**,
which is exactly why capabilities live on the connection rather than on the host descriptor. The
daemon *could* publish media into a LiveKit room to fill the gap, but that would make the desktop's
own host quietly require the thing this stack made optional. The surfaces are absent instead, which
[capability gating](capability-gating.md) already handles with no further work here.

## The browser is protected behaviourally, not structurally

`packages/tddy-desktop` is a Tauri shell over `packages/tddy-web/dist`: `tauri.conf.json` sets
`frontendDist` to the browser's own bundle. **One build, one entry** (`src/index.tsx`), served to
browsers by the daemon and loaded by the shell alike. There is no desktop-only module and there never
was — `daemonTransport.ts` has always pulled `tddy-tauri-web` into the browser bundle.

So this module *is* in every build there is, and no import graph can keep it out. What keeps a
browser off the IPC path is that `localHostRegistrationFor(win, daemonInstanceId)` answers `null` for
it, so nothing is ever registered:

```ts
if (daemonTransportFlavour(win) !== "webview-ipc") return null;
```

That is not a new question. `daemonTransportFlavour` already decides how *this page reaches its own
daemon* — same-origin `/rpc`, or the IPC bridge — and a host reached over IPC is available on exactly
the same terms. Asking a second, differently-worded question ("is this the desktop") would have
invented a way for the two answers to disagree. There is no `isDesktop` anywhere.

The alternative considered and rejected was a second bundle with its own Vite entry, which would have
made the guarantee structural at the cost of `tauri.conf.json`, `./install` and `./publish.sh` — a
build-architecture change for a property a null registration already provides.

## The daemon connection is not this module's to open

**This is the subtlety worth reading the file for.** A bridge owns one connection and one epoch, and
`openConnection(DAEMON_TARGET)` returns the bridge the page already holds.

`RpcTransportProvider` builds the daemon transport eagerly on first render, which on the desktop opens
the `Daemon`-targeted bridge and **connects it** — `webviewFramePipe` calls `bridge.connect(...)` as
it is constructed, invoking `tddy_rpc_connect` under that bridge's epoch. All of that happens before
any screen renders.

A host connection that then called `openConnection(DAEMON_TARGET)` would be handed **that same
bridge** — the page's registry memoises per target — and building a transport over it would connect
it a second time, under the same epoch. `MultiConnectionHost` answers `EpochInUse`, deliberately
leaving the incumbent serving, so:

- every call through the local host connection fails immediately; and
- worse, the bridge assigns `registration` whatever the last `connect` returned, so the *shared
  daemon bridge* ends up holding the rejected promise — and its `close()`, which skips
  `tddy_rpc_disconnect` when the registration never resolved, silently stops releasing the page's own
  connection.

So `IpcHostConnection` **uses the transport the page already has** (`LocalHostWiring.hostTransport`,
supplied from `useHttpTransport()` at the registration site) and never names `DAEMON_TARGET` at all.

This was a live bug, not a hypothetical. It was latent while each transport minted its own epoch — a
second transport over one bridge simply got a different epoch and worked — and became a hard failure
the moment `tddy-tauri-web` made the bridge own its epoch. Two designs that were each locally
reasonable met in the middle. `WebviewIpcBridge.connect`'s own doc now states the invariant outright:
*a bridge is connected once, by the one transport built over it.*

`LocalHostWiring.transportFor` is therefore for **session** bridges only, which
`openConnection(sessionTarget(id))` genuinely mints fresh.

### Neither half of the wiring has a default

`hostTransport` and `transportFor` are both supplied by `LocalHostConnections` and both refuse, by
name, when absent. `transportFor` builds `createDefaultWebviewIpcTransport(bridge, meters,
authTokenGate)` from React context, and the auth gate is why there is no fallback: a webview stays
open longer than an access token lives, so a transport built without it would send stale credentials
over a connection it had opened, rather than failing. Refusing names what is missing; a quiet default
would not.

## One session, one wire, counted attachments

`openSession` opens a `Session`-targeted IPC connection — separate, concurrent, released on
`close()`. The hint's LiveKit fields (`room`, `url`, `serverIdentity`) are read by nobody on this
path: a session on this host is served by this host, and a session reached this way advertises
`{"rpc"}` whatever the daemon advertised about it.

Sharing is not a choice this module makes. The host application keys bridges by target, so two
attachments of the same session **are** one connection whatever anything above believes. What is
chosen is what that means for `close()`: `IpcSessionWire` counts attachments and releases the
host-side peer when the **last** one lets go.

- Releasing on the first would take the wire out from under a screen still rendering the session —
  `useSessionAttachment` is per screen, so two screens on one session is ordinary.
- Never releasing would leak a host-side peer per session for the life of the page.
- Building a second transport for the second attachment would be the `EpochInUse` failure above, one
  level down.

Each attachment still gets its own handle with its own `close()`, its own terminal resume points, and
`clientFor`/`transport` that throw once detached — a call issued on a detached session has no answer
coming, and saying so beats leaving it unsettled. Releasing drops the registry entry, so re-attaching
the same session afterwards opens a genuinely new connection rather than a second transport over a
departed one.

`status` is `connected` while attached and `idle` once detached, and `error` never appears. A bridge's
`closed` promise resolves for exactly one reason — the page releasing it — so the only thing this
wire could learn from the host is what it already did itself, at the moment it has stopped caring. A
call that cannot be delivered fails as a call.

## Testing: the double has to model `registration`, not just `released`

The lesson from the bug above is a testing lesson. An optimistic `WebviewIpcBridge` double —
`connect` always succeeding, `close` always marking the bridge released — hid it completely, and hid
a leaked host-side peer besides.

The real `close()` skips `tddy_rpc_disconnect` **when the registration never resolved**. So a bridge
whose second `connect` was refused is released *page-side* while its peer survives, and a double that
reports page-side intent stays green in exactly the world where a peer leaks. `localHost.test.ts`'s
double therefore keeps both pieces of state the real bridge does, and exposes
`hostWasToldToDisconnect()` separately from `wasReleased()`; the lifecycle assertions read the first.
It also refuses a second `connect` by replacing `registration` with the rejected promise, deletes its
registry entry on release, resolves `closed`, and keeps its factory separate from its inspectors so
asking about a connection cannot mint one.

Both failures are pinned by mutation: reintroducing either the daemon-bridge reuse or the
second-transport-per-session regression turns tests red.

## Known limitation — capability availability on a mixed fleet

Node 4 predicted that "the first mixed fleet must revisit" the split in `useCapabilityAvailability`,
where the *status* half is fleet-wide (one LiveKit directory source per page) while the *capability*
half is host-scoped. This is that fleet, and it is **not fixed here** — capability gating is node 4's
surface.

It is narrower than node 4 feared. The unconfigured desktop, which is the common case, is already
correct: a host that never joins a room reports `idle` and never `connecting`, which
`capabilityAvailability` already handles, so nothing appears and then vanishes.

What remains: **with LiveKit configured and a common-room join in flight**, the local IPC host briefly
reads `connecting` for media rather than `unavailable`, because the fleet-wide room status wins for
the seconds the join takes. Transient and cosmetic — the surface settles to `unavailable` as soon as
the join resolves. `useCapabilityAvailability` is where a per-host fix belongs.

## Related

- [Host connections](host-connections.md) — the provider registry and precedence this registers into
- [Host directory](host-directory.md) — who the hosts are; the serving source names this one
- [Session connections](session-connections.md) — what `openSession` returns
- [Capability gating](capability-gating.md) — why `{"rpc"}` makes the media surfaces absent
- [Addressed webview IPC connections](../../tddy-desktop/docs/webview-ipc-connections.md) — the
  bridges, epochs and targets underneath all of this
