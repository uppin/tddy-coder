# 2026-11-15 — Placement list shrinks as logic suites convert onto the hub

**Type:** Refactor

`REMAINING_LOGIC_SUITES` in `tests/livekit_test_placement.rs` dropped the four `tddy-daemon`
suites converted onto `tddy-room-testkit` in this node (15 → 11). No suite moved into this
package: conversions move logic tests onto in-memory transports in their owning crates, and
this package remains contract/connectivity only.

Cross-package entry: [docs/dev/changesets/2026-11-15-in-memory-common-room.md](../../../../docs/dev/changesets/2026-11-15-in-memory-common-room.md).
