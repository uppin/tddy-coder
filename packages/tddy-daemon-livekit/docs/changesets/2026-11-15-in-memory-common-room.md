# 2026-11-15 — A transport-neutral snapshot entry for the peer registry

**Type:** Refactor

`CommonRoomPeerRegistry` learned `sync_from_participant_fields`, which reuses the
`peer_daemon_from_participant_fields` classifier that `sync_from_room` already delegates to.
The LiveKit discovery loop is unchanged; the in-memory hub drives the same registry from its
own participant snapshots, so converted logic suites assert against the real registry code.
(+23 lines in `livekit_peer_discovery.rs`; the file's oversized-file record carries the new
measurement.)

Cross-package entry: [docs/dev/changesets/2026-11-15-in-memory-common-room.md](../../../../docs/dev/changesets/2026-11-15-in-memory-common-room.md).
