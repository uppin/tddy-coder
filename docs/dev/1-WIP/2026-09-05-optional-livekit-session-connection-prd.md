# PRD: one session connection abstraction over any transport

**Stack:** `optional-livekit` — node 3 of 7 (`session-connection`)
**Target PRD on wrap:** [`docs/ft/web/session-drawer.md`](../../ft/web/session-drawer.md)
**Date:** 2026-09-05

## Problem

Attaching to a session is where LiveKit is deepest in `tddy-web`, and where the codebase already
admits a second path without abstracting it.

`ConnectSession` / `ResumeSession` reply with `{livekitRoom, livekitUrl, livekitServerIdentity}`.
`useSessionAttachment` branches on whether `livekitRoom` is empty and produces one of **two**
statuses:

- `connected-livekit` — the rich path. A **second** LiveKit room is joined per session
  (`useSessionLiveKitRoom`, minting a `web-traffic-*` observer identity); session RPC targets the
  session process's own participant `daemon-<instance>-<session>`
  (`sessionParticipantRpcClient`); a browser token is minted and refreshed on a TTL timer
  (`useLiveKitTerminalToken`); clients are cached with the `Room` object as the cache key
  (`sessionClientCache`).
- `connected-grpc` — a degraded path. Session RPC falls back to the **daemon** client
  (`SessionRuntime.tsx:176`), the terminal is `GhosttyTerminalGrpc`, and the connection handshake
  overlay never appears because it is LiveKit-only (`SessionRuntime.tsx:130`, `:545`). A `TODO` at
  `:264` records that gRPC sessions do not plumb everything.

Both statuses are threaded by hand through `SessionRuntime`, `sessionRuntimeRegistry`,
`SessionMainPane`, `SessionDetailPane` and `SessionsDrawerScreen` — so every consumer knows which
wire it is on, and a third wire would mean a third branch in every one of them.

## What this PR delivers

One **session connection** abstraction, opened from a `HostConnection`, and the two statuses
collapsed into it.

### The model

```ts
interface SessionConnection {
  readonly hostId: string;
  readonly sessionId: string;
  readonly status: ConnectionStatus;
  readonly error: string | null;
  readonly capabilities: ReadonlySet<ConnectionCapability>;
  clientFor<S extends DescService>(service: S): Client<S>;
  transport(): Transport;
  close(): void;
}

interface HostConnection {
  // ...as node 1 defined it, plus:
  openSession(sessionId: string, hint: SessionAttachmentHint): SessionConnection;
}
```

`SessionAttachmentHint` is what the attach reply carries, in transport-neutral terms — the provider
that opens the connection is the only thing that reads the LiveKit fields out of it.

`SessionAttachmentState` collapses to a single connected status carrying the connection and its
capabilities, so no consumer branches on the wire again.

### Acceptance criteria

1. `hostConnection.openSession(sessionId, hint)` returns a `SessionConnection` whose `clientFor`
   routes exactly where the current code routes: over LiveKit, to
   `daemon-<instance>-<session>`; where the attach reply names no room, to the daemon connection.
2. The LiveKit implementation owns room join, observer-identity minting, token generation and TTL
   refresh. None of those words appear outside `src/rpc/connections/livekit/`.
3. `SessionAttachmentState` has **one** connected status. `connected-livekit` and `connected-grpc`
   are gone from `useSessionAttachment`, `sessionRuntimeRegistry`, `SessionRuntime`,
   `SessionMainPane`, `SessionDetailPane` and `SessionsDrawerScreen`.
4. A connection over LiveKit advertises `{"rpc", "media", "presence"}`; one that resolves to the
   daemon advertises `{"rpc"}`.
5. Client identity is as stable as its routing — the guarantee `SessionClientCache` gives today,
   preserved with the connection (not a `Room`) as the cache key.
6. `close()` releases the underlying resources; switching sessions does not leak a joined room, and
   the existing attach-claim rules (`attachClaim.ts`) are untouched.
7. Which terminal component renders is derived from the connection's capabilities rather than from a
   status string. **Both components still exist** — merging them is node 5.
8. The connection handshake overlay is driven by `SessionConnection.status`, so a non-LiveKit
   connection gets a real status instead of no overlay at all.
9. Every existing session Cypress acceptance test passes unchanged in behaviour.

### Non-goals

Merging the terminal components, gating media tabs, the IPC transport, desktop registration. See the
changeset's `## Boundaries`.

## Why this shape

- **A hint, not a leak.** The attach reply is a proto message with LiveKit fields on it. Passing it
  whole into the provider keeps the parsing in one place instead of spreading `livekitRoom !== ""`
  across the app.
- **One status, capabilities on the side.** Two statuses meant every consumer re-derived "can I do
  X" from "which wire am I on". Capabilities answer that question directly and survive a third wire.
- **`close()` is explicit.** Today the session room's lifetime is a `useEffect` cleanup inside
  `useCommonRoom`. A connection that can be closed is what lets node 6 hold several IPC connections
  open at once without inventing a second lifecycle.

## Constraints

- **Zero new npm dependencies** (no public npm registry; `bun run local-registry-install`).
- No proto change: `SessionAttachmentHint` is built from the existing reply fields.
- `tddy-web` only.

## Successor PRs

- `feature/optional-livekit/capability-gating` — media and presence surfaces gated on capability.
