# 2026-10-04 — Every LiveKit e2e test names a room of its own

**Type:** Refactor

`#e2e-leg` 1/5 (root node), PR [#578](https://github.com/uppin/tddy-coder/pull/578),
`feature/e2e-leg/unique-rooms`, base `master`. Package entry:
[2026-10-04-unique-rooms.md](../../packages/tddy-livekit-testkit/docs/changesets/2026-10-04-unique-rooms.md).

Test infrastructure only. A precondition for a LiveKit server shared by the CI job (node 4) and for
lifting the serial `docker` test-group (node 5): with either, two tests on one room name interfere,
and `#[serial]` does not help under nextest. Nothing about scheduling changed.

## What shipped

- `LiveKitTestkit::unique_room(prefix)` returns `<prefix>-<hex nanos>-<pid>-<counter>`. No `uuid`
  dependency was added.
- **42** fixed room names migrated across `tddy-livekit`, `tddy-daemon-livekit`, `tddy-daemon`,
  `tddy-daemon-rpc`, `tddy-e2e`, `tddy-session-lifecycle` and `tddy-coder` test files, including the one
  cross-binary collision (`acceptance-common-room`, shared by `livekit_peer_daemons_acceptance` and
  `multi_host_acceptance`). The ad-hoc nonce helpers were replaced by the helper.
- Guard test `livekit_tests_use_unique_rooms` (text scan of every package's `tests/`) fails on a fixed
  room and names the file, line and constant.
- Eleven heavy test files build their room once per process with a `OnceLock` `room()` helper (unique
  per binary run, not per test), because their tests share one room on purpose; per-test isolation
  inside a binary is node 5's concern. The helper is repeated in those files, deferred rather than a
  testkit API node 5 may replace.
- `rpc_scenarios.rs` was edited, though node 2 (#579) owns it: the guard flags its fixed per-block room
  names, so only room-name expressions changed (literals only; no deadline, no scenario structure).
  Node 2 rebases over those lines.
- Two identity consts in `first_login_enrolment_acceptance.rs` were renamed
  (`THE_DESKTOPS_ROOM_IDENTITY` to `THE_DESKTOP_LOBBY_IDENTITY`, `A_ROOM_PEERS_IDENTITY` to
  `A_LOBBY_PEERS_IDENTITY`) because the guard matches any `ROOM` const holding a string literal; values
  unchanged.
- Verified against a real server: `ListParticipants` for a nonexistent room returns `Ok([])`, so
  `LiveKitRoomRoster::read_roster` needed no change and no production code besides `unique_room`
  changed. The planned roster-race test was not written.
- The rule is documented in [testing.md](../guides/testing.md#test-rooms-come-from-unique_room) and
  [ci.md](../guides/ci.md).

## Verification

Scoped locally: the helper tests and the guard test pass; `cargo clippy -p tddy-livekit-testkit
--all-targets -- -D warnings` and `cargo fmt --check` are clean; `cargo check --all-targets` over the
seven touched packages is clean.

**Deferred to CI — verified by the PR checks, not a local gate:** the Docker-based LiveKit suites were
not run locally, and the milestone "two consecutive green e2e runs plus one shuffled run" and the TODO
"read the e2e leg on CI" were not completed here. The developer decided that wrapping happens when the
PR is ready for review, not after CI, so these are not claimed as passed.

## Backlog

[2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md)
is **narrowed, not resolved**: step 1 (isolation) is done and two of its claims are corrected there;
the remaining steps belong to later nodes, and the last node of the stack removes the note. No code
issue record in the touched packages is claimed by this PR; none reconciled.
