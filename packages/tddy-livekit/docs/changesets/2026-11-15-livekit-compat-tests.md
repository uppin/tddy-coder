# 2026-11-15 — LiveKit contract suites move to `tddy-livekit-compat-tests`

**Type:** Refactor

Nine suites moved to `tddy-livekit-compat-tests` with `move_test_binary_to_crate`:
`acp_session_livekit`, `broadcast_and_room_metadata`, `participant_metadata_acceptance`,
`room_connector`, `room_roster_livekit`, `rpc_client_factory`, `rpc_scenarios`,
`rpc_three_participant_forward`, `webrtc_runtime_bounded`. Their grouped crate-root imports were
first rewritten to one `use` per item, spelled by the defining module
(`tddy_livekit::participant::LiveKitParticipant`, `tddy_livekit::client::RpcClient`, …), the shape
the move can re-point. `participant_metadata_unit`, `room_roster_deadline` and `signal_tls` stay
here; they need no server. The crate's `tddy-livekit-testkit` dev-dependency is gone.

Cross-package entry: [docs/dev/changesets/2026-11-15-livekit-compat-tests.md](../../../../docs/dev/changesets/2026-11-15-livekit-compat-tests.md).
