# PRD: starting a session never waits on LiveKit

**Stack:** `optional-livekit` — node 9 of 9.
Changeset: [`2026-09-06-optional-livekit-lazy-session-room.md`](2026-09-06-optional-livekit-lazy-session-room.md)
Discovery: [`2026-09-06-optional-livekit-lazy-session-room-initial-discovery.md`](2026-09-06-optional-livekit-lazy-session-room-initial-discovery.md)

## Problem

An operator whose LiveKit server is unreachable cannot start a session at all. Not a degraded
session, not a session without media — no session. The daemon opens a per-session LiveKit room before
spawning the agent, the call has no timeout at any layer, and the client's RPC eventually times out
with `creating session room session-…`.

The deployment that never configured LiveKit is unaffected, because that path returns early. So the
failure lands on exactly the operator who *did* configure it and whose server is merely down — a
laptop off the VPN, a server restarting, an address that has gone stale. Their local session, which
needs nothing from LiveKit, waits on it anyway.

This is the last place the `optional-livekit` stack's premise is untrue. Nodes 1–7 made the client
work without LiveKit; node 8 made it switchable off; a session still cannot be *created* without it
answering.

## What this PR delivers

Session creation becomes a local operation. The session room is created the first time something
actually connects over LiveKit.

### Acceptance criteria

1. `StartSession` makes **zero** LiveKit calls — with LiveKit configured, and with it absent. Proven
   by asserting no call was made, not by a successful return.
2. A session starts normally while LiveKit is **configured and unreachable**. This is the reported
   failure.
3. A session starts normally while LiveKit is **absent**, as it does today.
4. For a session type that has a session room, the room exists by the time the first LiveKit consumer
   needs it — `ConnectSession`, split-placement start, `ensure_session_room_for_agents`, session-sync
   mirroring, seeded agent clones, participant admission, or the split agent's remote-identity route.
5. Two concurrent first-connections produce **one** room. A second connection reuses it.
6. A connection that needs the room and cannot create it **fails loudly**, with the reason. Session
   creation is what is unblocked, not LiveKit error reporting.
7. A session created while LiveKit was down becomes reachable over LiveKit once LiveKit is
   reachable, **without a restart** — both its room *and* its PTY bridge appear on the first LiveKit
   connection.

   **Corrected during implementation.** This first said "fully functional", which conflated two
   channels. On the desktop the local host is reached over **IPC**, and that is the only channel
   relevant to it: such a session is fully functional the moment it starts, whether LiveKit is up,
   down or absent, and it never needs a bridge. The PTY bridge exists so a **remote** client can
   drive the terminal over LiveKit (`PtyLiveKitService` in the session room). It is therefore lazy
   for exactly the reason the room is — it is LiveKit work, and LiveKit work happens when a LiveKit
   consumer arrives, not at spawn. A desktop-only deployment creates neither, ever.
8. LiveKit control-plane calls are bounded by a timeout, so an unreachable server fails fast rather
   than hanging until a client gives up.
9. The facilitating daemon is still the room's first participant when the room is created — the PRD
   invariant (FR2) holds at the new moment, or the design says explicitly why it cannot.
10. Nothing that has a session room today loses one.

### Non-goals

- Gating rooms on session type. `claude-cli` and `cursor-cli` run agents and their rooms are a
  documented feature.
- Making room failures non-fatal at the point of use.
- The `tool` session path's `"LiveKit not configured"` precondition
  (`connection_service.rs:10713`) — real, separate, narrower than it appears.
- Any change to the common room, peer discovery, or the terminal room.

## Why this shape

**Lazy, not conditional.** The alternative — deciding up front which sessions need a room — was tried
and abandoned: the only available signal is session type, and the types that look exempt are exactly
the ones that run agents and therefore *do* have rooms. Deferring asks no question at start and keeps
every consumer's guarantee, because the room appears when it is first wanted.

**The consumer list is the risk.** Six call sites need the room today. Deferring is only safe if each
triggers creation; a missed one turns a guarantee into an intermittent failure, which is worse than
the bug being fixed. The implementation must demonstrate the list is complete.

## Constraints

- `docs/ft/daemon/session-room.md` states the room opens before the agent is spawned. This node
  changes that contract and must amend it.
- No new dependency; no config flag (node 8 owns that).
- `tddy-desktop` and Cypress e2e are outside the CI gate.

## Successor PRs

None — this is the stack's top node.
