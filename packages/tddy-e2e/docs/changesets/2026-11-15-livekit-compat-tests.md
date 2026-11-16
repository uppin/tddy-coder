# 2026-11-15 — LiveKit suites move to `tddy-livekit-compat-tests`

**Type:** Refactor

Four suites moved to `tddy-livekit-compat-tests` with `move_test_binary_to_crate`:
`livekit_terminal_rpc`, `terminal_service_livekit`, `token_generation_livekit`,
`token_service_livekit`. Their `#[cfg(feature = "livekit")]` gates went with the move — in the new
crate the suites are unconditional, and the crate enables this crate's `livekit` feature on its
dev-dependency instead. That exposed a stale three-argument `Presenter::new` call in
`spawn_presenter_with_view_connection_factory` (only compiled under the feature), now the
four-argument shape its sibling helper uses. Everything else here — the gRPC and PTY suites that
run the presenter in memory — is unchanged.

Cross-package entry: [docs/dev/changesets/2026-11-15-livekit-compat-tests.md](../../../../docs/dev/changesets/2026-11-15-livekit-compat-tests.md).
