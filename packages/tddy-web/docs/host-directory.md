# Host directory

Who this page can talk to, and how sure it is.

Knowing which hosts exist used to be the same thing as being joined to a LiveKit common room:
`SelectedDaemonProvider` connected a `Room` and read the host list off its participants. With no
LiveKit configuration the list was empty — including of the daemon serving the page, which
`/api/config` has always named as `daemon_instance_id`. A build that does not join a common room
could therefore reach no host at all.

The directory separates *which hosts exist* from *how you reach one*. Reaching one is
[host connections](host-connections.md); this document is the other half.

## The model

`src/rpc/hostDirectory/types.ts`:

| Type | What it is |
|---|---|
| `HostDescriptor` | a host some source described: `hostId`, `label`, `sourceId`, and the optional `reposBasePath` / `maxAttachmentBytes` a daemon advertises |
| `HostDirectorySource` | one contributor: `id`, `status`, `error`, `hosts` |
| `HostDirectory` | the merge every host-selection surface reads: `hosts`, `sources`, `status`, `error` |

A directory is the merge of several sources. A browser page has the LiveKit source and the
serving-host source; a desktop build adds one of its own. Nothing in the model knows what a room is.

## The merge rules

`mergeHostDirectory` (`useHostDirectory.tsx`) is pure and unit-tested without a rendered provider.

**Hosts** de-duplicate by `hostId`, **first source wins**. Order is therefore precedence, and it is
load-bearing: a desktop app puts its own source first so its description of its own machine beats the
common room's advertisement of it. Within a source, order is the source's own — the LiveKit source
already orders by the room's participant ordering, and re-sorting would make the selector jump under
the operator.

**Status is optimistic** — `connected` as soon as *any* source is connected, then `connecting`, then
`error`, then `idle`:

- One working source is a usable directory. A desktop app whose LiveKit peers are unreachable can
  still use its own host and must not be shown a connection error for a feature it never asked for.
- An unconfigured source reports `idle`, so it never drags the directory into `error`. **That is
  what makes an absent LiveKit configuration a choice rather than a fault.**
- `error` therefore means *every* source failed, and only then does the directory publish a reason
  (the first source with one). A failure on one source while another still names hosts belongs to
  that source and is read off `sources`.

A consequence worth knowing: because the serving source is `connected` from the first render, a
daemon-served page's directory status is `connected` while the common room is still joining. The
fleet starts at one host and grows. That is the intended trade — a real partial fleet beats asserting
there is none — and it is why a surface that is *about* the common room reads that source directly
rather than the merged status:

```ts
const commonRoom = useHostDirectorySource(LIVEKIT_SOURCE_ID);
commonRoom?.status === "error";   // not directoryStatus === "error"
```

`DaemonSelector`'s "Common room unreachable" placeholder and the LiveKit presence screen both do
this. Reading the merged status there would make the message unreachable on every production page.

## The sources

| Source | Id | Contributes |
|---|---|---|
| `useLiveKitHostDirectorySource` | `livekit` | common-room participants whose metadata parses as a daemon advertisement, with `reposBasePath` / `maxAttachmentBytes` carried through |
| `useServingHostDirectorySource` | `serving` | the daemon that served this page, from `servingInstanceId` |

With no `livekitUrl` / `commonRoom`, `useCommonRoom` short-circuits before constructing a `Room` or
calling `TokenService` — **nothing is built and no token is minted** — and the source reports `idle`.

The serving source is `connected` whenever there is an instance id: the page was served by that
daemon, so its existence is not in question. This source **names a host; it does not claim a wire to
it.** Whether anything can reach it is the connection registry's answer.

## Presence is asked for, not ambient

`SelectedDaemonContextValue` used to publish `room: Room | null` to the whole subtree, so any
component could reach LiveKit without declaring it needed presence. `room` is gone from that context.
A component that wants the participant roster asks by name and can be refused:

```ts
const room = useHostPresence(hostId);   // Room | null
```

It returns `null` unless that host's `HostConnection` advertises the `presence` capability. Two
things are needed and neither is sufficient alone: the **capability** says whether a roster applies
to this host at all, and the **room** is where the roster actually is. The room does not come from
the connection — `LiveKitHostConnection` keeps its own private, precisely so that holding a
connection does not let a component help itself to LiveKit — but from a context owned by this module
and populated by whoever owns the join.

The return type names `Room` deliberately. Presence is a LiveKit concept, and a neutral wrapper with
exactly one implementation would be a fiction. What changed is that reaching it requires asking.

## Wiring

`SelectedDaemonProvider` composes the sources and renders:

```
HostDirectorySources → HostPresenceRoom → LiveKitConnections → SelectedDaemonScope
```

`SelectedDaemonScope` reads the merged directory through `useHostDirectory()` — the same way every
other consumer does, rather than the provider quietly holding a merge nobody else can see — maps
`HostDescriptor` to the `DaemonHost` the screens speak (`daemonHost.ts`), and owns the selection.
`LiveKitConnections` stays **above** the `key={selectedInstanceId}` remount boundary: a host change
reloads the screens, it does not re-register the wire that reaches them.

Selection, persistence, URL sync and that remount are unchanged by the directory.

## Context surface

`useSelectedDaemon()` publishes `daemons`, `selectedInstanceId`, `servingInstanceId`, `selectDaemon`,
and:

| Field | Meaning |
|---|---|
| `directoryStatus` | whether this page can name hosts at all — the merged, optimistic status |
| `directoryError` | why, when `directoryStatus` is `error`; `null` otherwise |

These replace `roomStatus` / `roomError`. To ask about one source in particular, read
`useHostDirectorySource(id)` rather than the merged value.

## Testing

`HostDirectorySources` takes arbitrary sources, so a surface can be driven from an in-memory
directory with no `livekit-client` in the tree. `SelectedDaemonProvider` keeps its `room` / `daemons`
/ `roomFactory` test seams; `daemons` now supplies the *common room's* contribution, and the serving
host is still contributed alongside it exactly as in production.

Two traps worth repeating, both of which produced tests that could not fail:

- **Presence needs a registered wire.** With no `ConnectionProviders` in the tree, `useHostConnection`
  resolves against an empty registry and returns `null`, so `useHostPresence` returns `null` whether
  or not the capability gate exists. A spec about the gate must register a wire that really reaches
  the host, and supply a room, so the capability is the only variable.
- **A hand-built `HostDirectorySource` proves the merge, not the sources.** Assert the production
  path — `SelectedDaemonProvider` with `servingInstanceId` and no LiveKit configuration — for
  anything about what a source *produces*.

## Related

- [Host connections](host-connections.md) — the wire a host id is reached over
- [Cross-daemon fan-out](host-fan-out.md) — reading many hosts at once
- Product: [Daemon selector + host-connection RPC routing](../../../docs/ft/web/daemon-selector-livekit-rpc.md)
