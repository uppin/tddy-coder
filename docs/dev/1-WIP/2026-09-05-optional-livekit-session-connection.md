# Changeset: optional-livekit-session-connection

**Stack:** `optional-livekit` — node 3 of 7 (parent: `connection-model`, base
`feature/optional-livekit/connection-model`)
PR: [#439](https://github.com/uppin/tddy-coder/pull/439)
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
  participant targeting — absorbing `useSessionLiveKitRoom`, `sessionParticipantRpcClient` and
  `sessionClientCache`. **`useLiveKitTerminalToken` was not absorbed**: `SessionLiveKitTerminal`
  still performs its own room join, and folding that in is node 5's. Deferred there, not delivered
  here.
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

**Sequencing:** none blocked.

Two consequences of extending the parent's interface, both deliberate and both recorded here rather
than left to be discovered:

1. **`openSession` is added to `HostConnection`,** which node 1 owns. Adding a member is this PR's to
   do (see `## Boundaries`); it makes two of node 1's *test fixtures* fail to typecheck, so this PR
   adds a throwing `openSession` to each — one line in `src/rpc/connections/registry.test.ts` and one
   in `cypress/component/HostConnectionAcceptance.cy.tsx`. No production file of node 1's is touched.
2. **This node's acceptance tests take their `HostConnection` straight from the fixture provider,
   not through `ConnectionProviderRegistry`.** The registry is node 1's and **is** implemented on
   this branch — it was still red when this was written, and the rebase onto node 1's green landed
   it. Routing a host to a connection remains incidental to what these specs assert, so the seam
   stays; the reason is attribution, not absence.

   Since `/pr-wrap` those acceptance specs no longer take a hand-written provider at all: they drive
   `LiveKitConnectionProvider.openSession` into the real `openHostServedSession` and mount the real
   `SessionRuntime`, because a spec that asserts the behaviour of its own fixture pins nothing.

3. **A third node-1 test file gains an additive block.** `src/rpc/connections/liveKit.test.ts` gets a
   `describe` covering `openSession`'s two refusals. `openSession` is this PR's symbol, so its tests
   are this PR's; the block is append-only, so it cannot conflict with node 1's own edits.

**Sibling note (not a dependency):** node 2 (`host-directory`, #438) also branches off node 1 and
edits `SessionsDrawerScreen.tsx`, in a different region (`:315-317`, `:856` — presence). This PR's
region is attachment and the session client — broadly `:12-17`, `:233-291`, `:394-421`, `:524-559`,
`:600-650`, `:847-878`. That reaches nearer node 2's `:856` than first planned; the two remain
disjoint in content (attachment/session client vs presence), but check the seam when rebasing.

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

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests — `cypress/component/SessionConnectionAcceptance.cy.tsx`
- [x] Run acceptance tests (verify they fail) — 5/5 on `attachmentHintFromReply`
- [x] USER REVIEW — acceptance tests — waived 2026-09-05 (run wave 2 straight through)
- [x] TDD Red — write failing unit/integration tests — `src/rpc/connections/sessionAttachment.test.ts`
- [x] Implement production code making tests pass (`/green`)
- [x] `/validate-changes` — three passes (changes, tests, prod-readiness/clean-code); seven
      production defects found and fixed, see **Review findings** below
- [ ] `/pr-wrap`

## Verification

### Green result

| Gate | Result |
|---|---|
| `bun run --filter tddy-web test:unit` | **1034 pass, 0 fail** (1018 before the review fixes) |
| `bun run --filter tddy-web build` | clean, exit 0 |
| Cypress, the specs this node touched plus the at-risk ones | **71/71**, over 8 specs |

Node 2's nine `host directory merge` failures, inherited while #438 was still red, cleared when its
green landed and this branch rebased onto it. Nothing on this branch is red.

### Baseline after rebasing onto the parent's contract commit

`bun run --filter tddy-web test:unit` — 971 pass, 6 fail. The 6 were the **parent's** red
(`src/rpc/connections/registry.test.ts` from #437), inherited and not this node's to fix. Both
that red and node 2's have since cleared.

The full Cypress component sweep ran once at node 1 (207 specs, 1213/1214; the one failure
pre-existing in `SelectedHostUrlStateAcceptance.cy.tsx`). Per-node full sweeps were dropped for
nodes 2–5 by agreement; one full sweep runs at the Step 8 completion gate.

### Red status at the contract commit

| Suite | Result |
|---|---|
| `src/rpc/connections/sessionAttachment.test.ts` | **5 tests, 5 failing** |
| `cypress/component/SessionConnectionAcceptance.cy.tsx` | **5 tests, 5 failing** |

Every failure is on this node's own `TODO(session-connection)` bodies — `attachmentHintFromReply`,
`capabilitiesForHint`. None is on a parent's surface.

### Review findings

Three review passes over the green phase found seven production defects. All are fixed on this
branch; recorded here because the tests were green throughout and green was hiding them.

| # | Defect | Resolution |
|---|---|---|
| 1 | The handshake overlay tracked the RPC connection's room while the terminal joined its own, and `status` was gated on the session process appearing in `remoteParticipants` — so the overlay could lift over a handshaking terminal, or never lift at all. It is `pointer-events-auto`, so the second case left an unclickable pane with no recovery. | `status` is reachability only; presence still routes calls. The terminal's own status is combined back in via `leastConnectedOf`, marked `TODO(optional-livekit node 5)`. |
| 2 | Session connections were never released when `SessionsDrawerScreen` unmounted — a joined room and a rescheduling token timer stranded per session, per navigation. A regression against `useCommonRoom`'s effect cleanup. | `SessionRuntimeRegistry.closeAll()`, called from the screen's cleanup; a connection opened after unmount no longer escapes registration. |
| 3 | `close()` during an in-flight join released the room twice. | The join owns the release once `close()` has latched. |
| 4 | A token TTL ≤ 60s collapsed the refresh delay to zero, spinning `refreshToken` at the timer floor. | Floored, via an exported injectable `TokenRefreshPolicy`. |
| 5 | One refused renewal ended refreshing permanently; the room dropped silently at expiry. | Bounded retry, warns once when spent. The room is still deliberately left up. |
| 6 | The terminal's browser identity was minted during render and reused across re-attaches, colliding with a participant still leaving. | Random suffix; ref writes moved out of render. |
| 7 | Disconnecting a session could leave the attachment reporting a closed connection as `connected`. | The attachment resets when the connection it names is released. |

Three test defects went with them: two tests that could not fail (one asserting object identity where
every call returns a fresh object, one whose TTL clamped the very delay it claimed to check), and an
acceptance spec that asserted the behaviour of its own fixture. `useConnectionStatus`, the registry's
connection lifetime and the terminal identity had no coverage at all and now do.

### Commands

```bash
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
```

Installs via `bun run local-registry-install` — the public npm registry is unavailable.

## Successor PRs

- [`2026-09-05-optional-livekit-capability-gating.md`](2026-09-05-optional-livekit-capability-gating.md)
  — branch `feature/optional-livekit/capability-gating`
