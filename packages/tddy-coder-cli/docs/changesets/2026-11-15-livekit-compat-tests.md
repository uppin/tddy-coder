# 2026-11-15 — Participant contract suites move to `tddy-livekit-compat-tests`

**Type:** Refactor

Two suites moved to `tddy-livekit-compat-tests` with `move_test_binary_to_crate`:
`coder_publishes_session_metadata_to_participant` and
`coder_serves_connection_service_from_participant`. They now name the host by the module that
defines it, so `src/lib.rs` declares `pub mod livekit_participant_host;` — `LiveKitParticipantHost`
was already public through the crate root, and its visibility is unchanged. The crate's
`tddy-livekit-testkit` dev-dependency is gone.

Cross-package entry: [docs/dev/changesets/2026-11-15-livekit-compat-tests.md](../../../../docs/dev/changesets/2026-11-15-livekit-compat-tests.md).
