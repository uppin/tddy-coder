# PRD: a transport-neutral host-connection model for tddy-web

**Stack:** `optional-livekit` — node 1 of 7 (`connection-model`)
**Target PRD on wrap:** [`docs/ft/web/daemon-selector-livekit-rpc.md`](../../ft/web/daemon-selector-livekit-rpc.md)
**Date:** 2026-09-05

## Problem

`tddy-desktop` does not connect to LiveKit by default, and today that means it cannot talk to any
host at all beyond the daemon that serves its own page. Everything above the serving-daemon
transport in `tddy-web` is spelled in LiveKit's vocabulary:

- `src/rpc/transportProvider.tsx` exposes the peer seam as
  `liveKitFactory: (room: Room, targetIdentity: string, options?) => Transport`. A caller cannot
  express "a connection to host H" without holding a `livekit-client` `Room`.
- `src/rpc/selectedDaemon.tsx` exposes `room: Room | null` on its context and builds every
  daemon-level client as `useLiveKitClient(service, room, daemonRpcIdentity(instanceId))`.

There is no name in the codebase for *a connection to a host*, so there is nowhere to plug a second
implementation of one.

## What this PR delivers

A first-class **host connection** model, and every daemon-level RPC call site in `tddy-web`
routed through it — with LiveKit as one registered provider, behaving exactly as it does today.

### The model

```ts
/** What a connection can do beyond plain unary/streaming RPC. */
type ConnectionCapability = "rpc" | "media" | "presence";

/** A live connection to one daemon host. */
interface HostConnection {
  readonly hostId: string;
  readonly status: ConnectionStatus;          // "idle" | "connecting" | "connected" | "error"
  readonly error: string | null;
  readonly capabilities: ReadonlySet<ConnectionCapability>;
  clientFor<S extends DescService>(service: S): Client<S>;
  transport(): Transport;
}

/** Knows how to reach hosts over one wire. */
interface ConnectionProvider {
  readonly id: string;                        // "livekit", later "ipc"
  connectHost(hostId: string): HostConnection | null;
}
```

Providers are held in a registry supplied through React context, so a host build (the desktop app)
can register its own without `tddy-web` importing it.

### Acceptance criteria

1. `useHostConnection(hostId)` returns a `HostConnection` for a host reachable by any registered
   provider, and `null` when no provider can reach it.
2. `useHostClient(service, hostId)` returns a memoised `Client<S>` whose identity is stable while
   the host and its underlying transport are unchanged, and fresh when either changes.
3. The LiveKit provider produces a connection whose `clientFor` is byte-for-byte the client
   `useLiveKitClient(service, room, daemonRpcIdentity(hostId))` produces today — same transport,
   same auth gate, same traffic meter.
4. The LiveKit provider advertises `{"rpc", "media", "presence"}`.
5. `useDaemonClient` / `useDaemonClientFor` keep their present signatures and behaviour, and are
   implemented on top of `useHostConnection`.
6. Every daemon-level RPC call site that reads `useSelectedDaemon().room` today reads a
   `HostConnection` instead: `useHostFanOut`, `useModelRegistryFanOut`, `ModelChatDialog`,
   `ProjectsAppPage`, and `SessionsDrawerScreen`'s cross-host daemon client.
7. A registry with **no** provider registered yields `null` connections and no thrown error — every
   call site already guards on a null client, and that guard is what makes LiveKit optional later.
8. A test can register an in-memory provider and drive every daemon-level screen with no LiveKit
   object anywhere in the tree.

### Non-goals

Stated in full in the changeset's `## Boundaries`. In short: the host *directory* (who exists),
session-bound connections, media-surface gating, the IPC transport, and the desktop wiring all
belong to later nodes.

## Why this shape

- **A provider registry, not a flag.** A `useLiveKit: boolean` would put a branch in every call
  site and leave the LiveKit import in the bundle. A registry means the desktop build registers a
  provider and `tddy-web` never learns which wire it got.
- **Capabilities on the connection, not on the host.** Whether VNC can render is a property of *how*
  you are connected, not of the machine — the same host is media-capable over LiveKit and not over
  IPC.
- **No behaviour change in this node.** The stack has ~120 Cypress specs that assume the LiveKit
  path. Keeping node 1 observably identical is what lets every later node be reviewed against a
  known-good baseline.

## Constraints

- **Zero new npm dependencies.** The public npm registry is not available; dependencies resolve
  against a local registry (`bun run local-registry-install`, `LOCAL_REGISTRY_URL`). Everything here
  lives in `packages/tddy-web`.
- No new proto, no daemon change: this node is `tddy-web`-only.

## Successor PRs

- `feature/optional-livekit/host-directory` — the host directory, and `room` leaving the context.
- `feature/optional-livekit/session-connection` — session-bound connections on a `HostConnection`.
