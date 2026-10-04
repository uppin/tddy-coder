# 2026-10-04 — `LiveKitTestkit::unique_room`

**Type:** Refactor

`#e2e-leg` 1/5, PR [#578](https://github.com/uppin/tddy-coder/pull/578). Cross-package entry:
[2026-10-04-e2e-leg-unique-rooms.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-unique-rooms.md).
State B: [livekit-testkit.md](../livekit-testkit.md).

`LiveKitTestkit::unique_room(prefix)` returns `<prefix>-<hex nanos>-<pid>-<counter>` with no new
dependency, and `tests/unique_room.rs` pins it (prefix kept, two calls differ, many threads never
repeat, valid LiveKit room name). The guard test `livekit_tests_use_unique_rooms` fails on any test
file that starts the testkit and names a room with a fixed literal.
