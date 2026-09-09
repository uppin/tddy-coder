# LiveKit rooms panel on every connection

## Problem

The `#/livekit` screen was gated wholesale on the `presence` capability. On the desktop build the
web UI reaches its embedded daemon over Tauri IPC, and that wire declares `rpc` and nothing else
(`IPC_CAPABILITIES`, `packages/tddy-web/src/rpc/connections/localHost.ts`) — so the entry vanished
from the navigation menu, and a deep link to the route landed on "LiveKit is not available on this
connection".

Only half of that was true. The **roster** is presence and genuinely has no data there: it renders
the room *this browser* joined, and an IPC wire joins none. The **rooms panel** is not — its feed is
`ConnectionService.StreamLiveKitRooms`, plain daemon RPC, and the daemon answers it from the LiveKit
server API. `LiveKitRoomsPanel`'s own comment conceded the point ("the feed itself is plain daemon
RPC and would survive without LiveKit") and gated it anyway, for consistency with the roster.

The cost fell on the one build most likely to need it: on the desktop there is no other way to see
who is joined to the common room or what metadata they published — which is exactly what an operator
diagnosing a daemon that joined but never advertised itself goes there to read.

## Change

The gate now asks what the panel actually depends on: a **daemon client**, not presence.

- `LiveKitRoomsPanel` renders its feed whenever the host is reachable. `connecting` and `error` keep
  the "joining" placeholder — those are the states where a LiveKit wire has no client yet, since the
  client arrives with the join — which preserves the no-layout-shift and no-stray-subscription
  properties the panel was split into a child component for.
- `LiveKitAppPage` no longer short-circuits. Both panels render and each answers for itself; the
  roster's own gate in `ParticipantList` is untouched and still names the connection as the reason
  it is empty.
- `DaemonNavMenu` offers the LiveKit entry unconditionally — the screen now leads somewhere on every
  connection, which was the entire premise of removing it.

## Documentation delta

`packages/tddy-web/docs/capability-gating.md` is the one context doc this contradicts, and it is not
edited here (per AGENTS.md, `packages/*/docs/` changes go through this workflow). Its worked
**presence** example needs to become a two-part rule when it is next wrapped:

- A capability verdict gates a **surface**, not a screen. Two panels on one screen may answer to
  different wires, and `#/livekit` is the case that proves it.
- Before gating on a capability, ask what the surface actually reads. A panel fed by daemon RPC is
  gated on the client; only a surface fed by the room itself is gated on presence. Gating the first
  on the second hides real data behind a verdict about the wire.

Also worth recording there: the rule's failure mode is silent. Nothing logged, nothing disabled —
the entry was simply absent, which is what made this take a debugging session to notice.

## Not in scope

- The RPC Playground's participant picker (`RpcPlaygroundScreen`) is genuinely presence-only — it
  addresses participants — and keeps its gate.
- `IPC_CAPABILITIES` is unchanged. Declaring `presence` on a wire that carries none would be a lie
  about the wire and would wrongly enable the media-gated surfaces that read the same set.
