# tddy-room-testkit

The in-memory common room logic suites run against instead of a real LiveKit server.
`RoomHub` hosts named rooms in-process; participants join under an identity, serve
`tddy-rpc` services, and call each other through the same ports production wires to
LiveKit. Nothing here opens a socket, needs Docker, or knows a LiveKit URL.

## Shape

- `RoomHub` / `HubParticipant` (`src/lib.rs`) — rooms, join/leave, the participant
  registry, forwarded calls stamped as `RequestTransport::LiveKit` with the sender's
  identity (production services make trust decisions from that stamp, so the hub must
  reproduce it, not a neutral stand-in), broadcasts, and server-stream relay.
- `HubPeerForwarder` / `HubEligibleDaemonSource` (`src/peer.rs`) — the peer-discovery
  surface: a caller for `peer_id` reaches `daemon-{peer_id}`; the eligible-daemon
  source wraps the kernel's `CommonRoom` over a `CommonRoomPeerRegistry` fed from the
  same advertised participant fields the LiveKit registry reads
  (`sync_from_participant_fields` in `tddy-daemon-livekit`, driven from `src/lib.rs`).
- `HubSessionReach` / `HubSessionRooms` (`src/session.rs`) — the `SessionReach` and
  `SessionRooms` ports over per-session hub rooms. The reach reports the room URL as
  `hub://local`, a scheme no network dialer will mistake for a server.
  `HubSessionRooms` holds each hosted session's `HubParticipant` for exactly as long
  as the room is hosted — dropping it leaves the room (its `Drop` impl), which empties
  the room the moment it opens.

## Rules for suites that use it

- Convert a logic suite by rewiring its ports to the hub, never by copying the suite:
  the same assertions run, only the transport changes, and the suite keeps its name
  (minus any `livekit` in it) in the owning package.
- When a suite converts, delete its line from `REMAINING_LOGIC_SUITES` in
  `tddy-livekit-compat-tests`' placement test in the same commit, and drop the owning
  crate's `tddy-livekit-testkit` dev-dependency when no remaining suite needs it.
- Contract/connectivity assertions do not belong here. If a converted suite only
  passes because the hub grew a LiveKit-shaped quirk, the quirk belongs in the compat
  package's contract suites, not in the hub.

## The satellite boundary

This crate names `tddy-daemon-livekit` in `[dependencies]` — the registry it feeds is the
production one — which the LiveKit-satellite rule (`livekit_is_a_satellite` in
`tddy-daemon-extensions`) normally forbids outside the plugins and binary wiring. The hub
is admitted there as a named test double, on one condition the same test enforces: every
crate that names this crate does so under `[dev-dependencies]`, so its LiveKit edge can
never enter a production build. Adding a `[dependencies]` edge to this crate anywhere
fails that test, by design.

## Why not a fake LiveKit server

A fake would have to reimplement the server's join, metadata and RPC semantics, and a
suite passing against it would prove the fake, not the product. The hub instead
implements the *ports* (`PeerForwarder`, `SessionReach`, …) the product already
depends on, so the production code under test is the real code; only the transport
below the port is in-memory. The compat package remains the proof that the real
LiveKit adapters honour the same contracts.
