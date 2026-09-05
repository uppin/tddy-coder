# Changeset: optional-livekit-session-connection

**Stack:** `optional-livekit` — node 3 of 7 (parent: `connection-model`, base
`feature/optional-livekit/connection-model`)
PRD: [`2026-09-05-optional-livekit-session-connection-prd.md`](2026-09-05-optional-livekit-session-connection-prd.md)
Discovery: [`2026-09-05-optional-livekit-session-connection-initial-discovery.md`](2026-09-05-optional-livekit-session-connection-initial-discovery.md)

## State A

Inherited from node 1: `HostConnection`, `ConnectionProvider`, the registry, `useHostConnection` /
`useHostClient`, and daemon-level RPC routed through them. Session attachment is untouched and still
LiveKit-shaped:

- `useSessionAttachment.ts` — `attachmentStateFromResponse` branches on `resp.livekitRoom !== ""`
  and yields `connected-livekit` (carrying `livekitRoom`, `livekitUrl`, `livekitServerIdentity`,
  `identity`) or `connected-grpc`.
- `useSessionLiveKitRoom.ts` — a **second** `useCommonRoom` join per attached session, with a
  `web-traffic-*` identity regenerated when the room name changes.
- `sessionParticipantRpcClient.ts` — `sessionParticipantIdentity(instance, session)` ->
  `daemon-<instance>-<session>`, client built through `liveKitFactory`.
- `useLiveKitTerminalToken.ts` — `TokenService.generateToken` / `refreshToken`, TTL-driven.
- `sessionClientCache.ts` — clients keyed by `targetIdentity` + the `Room` object as `transportKey`.
- The two statuses are branched on in `SessionRuntime.tsx` (`:130`, `:176`, `:264`, `:428`, `:545`,
  `:639-643`), `sessionRuntimeRegistry.ts:21`, `SessionMainPane.tsx:145`, `SessionDetailPane.tsx:22,46`,
  `SessionsDrawerScreen.tsx:230,233,388,399,530,533`.

## State B

- `HostConnection.openSession(sessionId, hint)` returns a `SessionConnection` (status, error,
  capabilities, `clientFor`, `transport`, `close`).
- `SessionAttachmentHint` carries the attach reply's routing fields; only the provider reads them.
- The LiveKit implementation, under `src/rpc/connections/livekit/`, owns room join, observer
  identity, token mint and TTL refresh. A hint naming no room resolves to the host connection and
  advertises `{"rpc"}` only.
- `SessionAttachmentState` has **one** connected status. `connected-livekit` / `connected-grpc` are
  gone from every consumer listed above.
- Terminal component choice and the handshake overlay derive from capabilities and
  `SessionConnection.status`, not from a status string. Both terminal components still exist.
- Client identity stability is preserved with the connection as the cache key.

## Responsibility

- `SessionConnection` and `SessionAttachmentHint`; `HostConnection.openSession`.
- The LiveKit session-connection implementation: room join, observer identity, token mint + refresh,
  participant targeting — absorbing `useSessionLiveKitRoom`, `sessionParticipantRpcClient`,
  `useLiveKitTerminalToken` and `sessionClientCache`.
- Collapsing `SessionAttachmentState` to one connected status, and migrating all six consumer files.
- Driving the handshake overlay and terminal selection off capabilities/status.
- Unit tests for routing, capabilities, cache-key stability and `close()`; Cypress acceptance tests
  for a session driven over a capability-limited connection.

## Boundaries

- Does **not** define or change `HostConnection`'s existing members, `ConnectionCapability`, the
  provider registry or `useHostClient` — node 1's, consumed as they stand. `openSession` is added to
  the interface **by this PR** and is this PR's to own.
- Does **not** touch the host directory, `SelectedDaemonProvider`, or `room` on its context. Node 2
  owns those, and this PR must not remove `room` even where it looks incidental.
- Does **not** hide or gate VNC, screen-sharing, participant video or the participant list. Node 4
  owns that; this PR leaves them rendering as today.
- Does **not** merge `GhosttyTerminalLiveKit` (736 lines) with `GhosttyTerminalGrpc` (631 lines).
  Node 5 owns that. This PR only changes *which* of the two is selected, and on what basis.
- Does **not** change `attachClaim.ts`'s rules, `ConnectSession`/`ResumeSession` protos, or any
  daemon behaviour.
- Does **not** add an IPC session connection. Node 6 provides the transport, node 7 the provider.
- Adds **no npm dependency**.

## Dependencies

What the parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `connection-model` (#437) | `HostConnection`, `ConnectionStatus`, `ConnectionCapability`, `ConnectionProvider` + registry, `useHostConnection` / `useHostClient`, `LiveKitConnectionProvider` | `openSession` hangs off `HostConnection`; a hint with no room falls back to the host connection's own client; capabilities reuse node 1's vocabulary | rename or widen `ConnectionCapability`; change `useHostClient`; re-implement the LiveKit *host* provider; register a second provider |

**Sequencing:** none blocked. Everything this PR needs exists as of node 1's contract commit.

**Sibling note (not a dependency):** node 2 (`host-directory`, #438) also branches off node 1 and
edits `SessionsDrawerScreen.tsx`, in a different region (`:315-317`, `:856` — presence). This PR's
region is `:230,233,257-284,388,399,530,533` (attachment and session client). Keep them disjoint.

## Draft PR contract

Lands first, so node 4 can branch off a real ref and compile against real signatures:

1. `src/rpc/connections/session.ts` — `SessionConnection`, `SessionAttachmentHint`, and
   `HostConnection.openSession`'s signature.
2. The collapsed `SessionAttachmentState` type.
3. Failing unit tests: a hint naming a room routes to `daemon-<instance>-<session>`; a hint naming
   none routes to the host connection and advertises `{"rpc"}` only; client identity is stable across
   renders and fresh when routing changes; `close()` releases the room.
4. Failing Cypress acceptance test: a session attached over a `{"rpc"}`-only connection reaches a
   connected status and shows a real handshake overlay — the case that renders nothing today.

Implementation and the six consumer migrations land in the same PR under `/green`. **Not a merge
candidate on the contract alone.**

## TODO

- [ ] Record initial discovery
- [ ] Create/update PRD documentation
- [ ] Create changeset
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap`

## Verification

```bash
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
```

Installs via `bun run local-registry-install` — the public npm registry is unavailable.

## Successor PRs

- [`2026-09-05-optional-livekit-capability-gating.md`](2026-09-05-optional-livekit-capability-gating.md)
  — branch `feature/optional-livekit/capability-gating`
