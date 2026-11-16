# 2026-11-15 — LiveKit contract suites move to `tddy-livekit-compat-tests`

**Type:** Refactor

Nine suites moved to `tddy-livekit-compat-tests` with `move_test_binary_to_crate`:
`common_room_duplicate_identity_repro`, `common_room_integration_status`,
`common_room_rpc_participant_rejoin_repro`, `common_room_set_metadata_handshake_repro`,
`forward_to_peer_shared_registry`, `forwarded_rpc_is_stamped_by_the_receiver`,
`livekit_integration_status`, `livekit_peer_daemons_acceptance`,
`session_room_livekit_acceptance`. Their uses of crate-root re-exports were first rewritten to the
defining module (`tddy_daemon_livekit::session_reach::LIVEKIT_INTEGRATION_ID`,
`tddy_daemon_livekit::livekit_peer_forwarder::LiveKitPeerForwarder`, …). The crate's
`tddy-livekit-testkit` dev-dependency is gone; the suites that stay (`session_rooms_port`,
`split_rooms`, `stream_livekit_rooms_rpc`, …) need no server.

Cross-package entry: [docs/dev/changesets/2026-11-15-livekit-compat-tests.md](../../../../docs/dev/changesets/2026-11-15-livekit-compat-tests.md).
