# 2026-11-15 — Session-room ports over the hub

**Type:** Refactor

New `session` module: `HubSessionReach` and `HubSessionRooms` implement the
`SessionReach`/`SessionRooms` ports over per-session hub rooms, bridging the terminal the
way `LiveKitSessionReach` does and reporting the room URL as `hub://local`.
`HubSessionRooms` holds each hosted session's `HubParticipant` for the hosted lifetime —
dropping it leaves the room, which had emptied rooms the moment they opened.

The peer-discovery types also moved from `lib.rs` into a `peer` module
(`HubPeerForwarder`, `HubEligibleDaemonSource`) via `tddy-tools restructure`, bringing
`lib.rs` from 551 to 424 production lines, under the 500-line budget.

Cross-package entry: [docs/dev/changesets/2026-11-15-hub-session-rooms.md](../../../../docs/dev/changesets/2026-11-15-hub-session-rooms.md).
