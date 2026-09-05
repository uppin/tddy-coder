# 2026-09-05 — media and presence surfaces are gated on connection capability

**Type:** Feature

Nodes 1–3 of `optional-livekit` made daemon and session RPC transport-neutral and put
`capabilities: ReadonlySet<ConnectionCapability>` on both connection kinds. Nothing consulted it.
Every media and presence surface rendered unconditionally, so a host reached without LiveKit would
have shown a VNC tab that never paints, a screen-sharing overlay with nothing to subscribe to, a
participant list permanently empty, and a sessions drawer that silently lost every cross-host row —
because that reconciliation is itself derived from presence.

`useHasCapability(connection, capability)` (`src/rpc/connections/useHasCapability.ts`) is now the
one predicate. Nothing re-derives capability from a transport, a status string or the presence of a
`Room`; a `null` connection answers `false`, and a caller that needs "no host" versus "host without
video" is asking a routing question and reads `status`.

**No capability can be gated on the predicate alone**, which is the finding this node is really
about. A common room still joining has not yet produced a connection carrying anything — the LiveKit
provider is bound to a `null` room until `Room.connect()` resolves — and a failed join produces no
connection at all, so a capability-first gate announces "not available on this connection" for the
second or two every LiveKit page spends connecting and then contradicts itself. Both halves had a
symptom: on presence it would have replaced the ICE-failure reason
`CommonRoomConnectionVisibilityAcceptance` exists to pin (the 2026-08-13 `udoo` incident) with a
verdict about the wire; on media the inspector's tab strip rendered seven tabs on load and nine a
second later, reflowing under the operator's cursor. `src/hooks/capabilityAvailability.ts` states the
order once — `error` → `connecting` → `unavailable` → `available` — so status answers for a
capability that exists and the predicate answers whether it exists at all, and a nav entry cannot
disagree with the screen it points at.

`src/hooks/useCapabilityAvailability.ts` is how a surface asks. Centralising the rule alone left six
components re-assembling the same three lines around it — resolve the connection, read the common
room out of the host directory, apply the predicate, default a missing source to `idle` — which is
the same drift one level up. It takes a connection rather than a host id because
`SessionInspectorDrawer` is handed one as a prop and has no id to resolve from. `ParticipantList` is
the single caller of the pure rule, because it is presentational and is *told* which room's status
it is reporting on.

**The host connection gates the media surfaces, not the session's.** `capabilitiesForHint` derives a
session's capabilities from whether the host handed it a room, so the host is the upstream fact; and
a dormant session has no `SessionConnection` at all, where reading one answers "no" to an
unanswerable question and strips the tabs from every dormant session. `SessionRuntime`'s
`carriesMedia` remains the one genuinely session-scoped media read.

Gated surfaces are **removed from navigation** rather than disabled: the inspector's VNC and Screen
Sharing tabs (with panel dispatch and the `?inspector=` fallback agreeing, so a media deep link
degrades to Details), the LiveKit nav entry, the rooms panel, the playground's participant picker.
Where an entry point must remain it names the reason — the `#/livekit` route, the participant roster,
and the sessions drawer's footnote that sessions on other hosts are not visible from this connection.
`LiveKitRoomsPanel` splits the two questions it was conflating: the panel keeps its place through a
join in flight so the page does not reflow, while `StreamLiveKitRooms` is subscribed from a child
mounted only on `available`, so no stream is opened for a panel nobody can read.

`ParticipantList`'s `data-room-status` now reports the room's real status. Collapsing `idle` and
`connecting` into one "Connecting…" branch used to make that attribute true by construction;
`available` is a verdict about the capability, not about the join, and a host that never joins a room
reaches it from `idle`.

Six props are **required** rather than defaulted — `ParticipantList.connection`,
`RpcPlaygroundScreen.presenceAvailable`, `SessionDrawer.crossHostSessionsVisible`,
`SessionInspectorDrawer.hostConnection`, `SessionMainPane.host` and `InspectorTabs.mediaAvailable`. A
default means "unknown ⇒ show it", which is the silence the gating removes; defaulting the other way
(`= null` ⇒ no media) is worse, because a forgotten prop then loses the media tabs silently and looks
exactly like a host that cannot carry a track.

**Known limitation, deliberately recorded rather than fixed:** the status half of the rule is
fleet-wide (one LiveKit host directory source per page) while the capability half is host-scoped, so
a common room `connecting` or in `error` answers "not unavailable" for every host. It cannot be
scoped per host today — during the join there is no `HostConnection` to read a status or a capability
off, and a descriptor's source id names which source advertised the host first, not which wire would
carry the capability. Harmless while LiveKit is the only registered provider; the first mixed fleet
([#443](https://github.com/uppin/tddy-coder/pull/443), node 7) must revisit it, and
`useCapabilityAvailability` is where the fix belongs.

Gating is rendering only: `sessionPaneIsWorkflowView` and terminal-claim decisions are unchanged, and
no media surface was deleted. No proto change, no daemon change, no new npm dependency. Tests: 1046
unit passing, and the new `PresenceCapabilityGatingAcceptance` (16),
`SessionInspectorMediaCapabilityAcceptance` (12), `ParticipantVideoCapabilityAcceptance` (4) and
`CapabilityGatingAcceptance` (5) beside ~380 existing Cypress tests re-run with no assertion changed.
Technical [capability-gating.md](../capability-gating.md), feature
[daemon-selector-livekit-rpc.md](../../../../docs/ft/web/daemon-selector-livekit-rpc.md). PR
[#440](https://github.com/uppin/tddy-coder/pull/440).
