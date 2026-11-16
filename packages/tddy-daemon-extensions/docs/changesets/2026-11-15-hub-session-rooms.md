# 2026-11-15 — The satellite boundary gains a test-double category

**Type:** Refactor

`tests/livekit_is_a_satellite.rs` flagged `tddy-room-testkit`: the in-memory hub names
`tddy-daemon-livekit` in `[dependencies]` because converted suites assert on the production
`CommonRoomPeerRegistry` it feeds. The rule's intent is that no production build gains a
LiveKit edge, and the hub can never be in one — `tddy-daemon` and `tddy-daemon-rpc` name it
only under `[dev-dependencies]`. The test now says so: `TEST_DOUBLES` may name a LiveKit
crate, and `a_test_double_is_only_ever_a_dev_dependency` proves no crate names a double
outside `[dev-dependencies]` (a double named from `[dependencies]` would launder its
LiveKit edge into a production build).

Cross-package entry: [docs/dev/changesets/2026-11-15-hub-session-rooms.md](../../../../docs/dev/changesets/2026-11-15-hub-session-rooms.md).
