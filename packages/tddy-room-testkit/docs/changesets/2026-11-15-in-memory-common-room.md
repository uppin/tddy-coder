# 2026-11-15 — New crate: the in-memory room hub

**Type:** Refactor

New test-support crate. `RoomHub` hosts named rooms in-process: participants join under an
identity with metadata, watchers follow joins, leaves and metadata changes, a
`CommonRoomPeerRegistry` attaches to a room and stays synced through
`sync_from_participant_fields`, services are served under a participant identity and called
through `RpcClientTransport` or through `HubPeerForwarder` (addressed at
`daemon-{instance_id}`, the way the kernel's `CommonRoom` addresses peers), and topics carry
broadcasts. Forwarded calls arrive stamped `RequestTransport::LiveKit` with the caller's
identity, exactly as a real room stamps them. A silenced-participant model supports the
attach suite's forward-deadline test. Its own `hub_room` suites (4 tests) run without Docker.

Depended on from `[dev-dependencies]` only; no production crate names this crate.

Cross-package entry: [docs/dev/changesets/2026-11-15-in-memory-common-room.md](../../../../docs/dev/changesets/2026-11-15-in-memory-common-room.md).
