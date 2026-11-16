# 2026-11-15 — A dedicated package for LiveKit connectivity and contract suites

**Type:** Refactor
**Packages:** tddy-livekit-compat-tests (new), tddy-livekit, tddy-daemon-livekit, tddy-e2e, tddy-coder-cli

New crate `tddy-livekit-compat-tests` holds the 24 suites whose job is to prove a LiveKit adapter
connects to a real server and honours the RPC contract: 9 from `tddy-livekit` (`rpc_scenarios`,
`rpc_three_participant_forward`, `room_connector`, `room_roster_livekit`,
`broadcast_and_room_metadata`, `participant_metadata_acceptance`, `rpc_client_factory`,
`acp_session_livekit`, `webrtc_runtime_bounded`), 9 from `tddy-daemon-livekit`
(`livekit_peer_daemons_acceptance`, `forward_to_peer_shared_registry`,
`forwarded_rpc_is_stamped_by_the_receiver`, `session_room_livekit_acceptance`,
`livekit_integration_status`, `common_room_integration_status` and the three `common_room_*`
repros), 4 from `tddy-e2e` (`livekit_terminal_rpc`, `terminal_service_livekit`,
`token_generation_livekit`, `token_service_livekit`) and 2 from `tddy-coder-cli`
(`coder_publishes_session_metadata_to_participant`,
`coder_serves_connection_service_from_participant`). Every suite moved with
`move_test_binary_to_crate`, test names unchanged.

No product logic lives in the new crate, and no logic suite moved into it. The logic suites still
running over a real room (11 in `tddy-daemon`, 2 in `tddy-session-lifecycle`, 1 each in
`tddy-daemon-rpc` and `tddy-worktree-service`) stay in the crates that own them until they run over
an in-memory or IPC transport; `tests/livekit_test_placement.rs` names them, pins that the moved
suites live only here, and pins the set of crates still holding a `tddy-livekit-testkit`
dev-dependency so the list can only shrink. The three origin crates that no longer need it
(`tddy-livekit`, `tddy-daemon-livekit`, `tddy-coder-cli`) dropped that dev-dependency.

CI gains a separate check, `Rust LiveKit e2e tests`, running only this package plus the testkit's
own suites (`.config/rust-livekit-e2e.filterset`) against one shared LiveKit server. The moved
binaries left `.config/rust-e2e.filterset`, and the unit leg now excludes both e2e sets, so no
suite runs twice and no logic suite pays for a container in a leg that has no server.

Two adaptations the moves needed: the four `tddy-e2e` suites lost their
`#[cfg(feature = "livekit")]` gate (in this crate they are unconditional), with `tddy-e2e`'s
`livekit` feature enabled on the dev-dependency instead — which exposed a stale
`Presenter::new` call in `tddy-e2e`'s LiveKit-gated helper, fixed to the four-argument shape its
sibling uses; and `tddy-coder-cli`'s `livekit_participant_host` module is now `pub mod` (its
`LiveKitParticipantHost` was already public through the crate root).

Cross-package entry: [docs/dev/changesets/2026-11-15-livekit-compat-tests.md](../../../../docs/dev/changesets/2026-11-15-livekit-compat-tests.md).
