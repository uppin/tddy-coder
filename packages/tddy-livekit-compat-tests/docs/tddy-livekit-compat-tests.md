# tddy-livekit-compat-tests

The one package whose tests are allowed to need a real LiveKit server. Every suite here proves
connectivity or wire contract against `tddy-livekit` (and `tddy-livekit-testkit`'s container
harness). No logic use-cases live here: a test that asserts what a daemon or client *does* with a
room belongs in the owning package, over the in-memory `tddy-room-testkit` hub or another cheap
transport, so it runs in the default unit run without Docker.

## What lives here

- The LiveKit/Room participant, publish/subscribe, RPC, metadata, data-stream and
  server-protocol contract suites, moved from `tddy-livekit`, `tddy-daemon-livekit`,
  `tddy-coder-cli` and `tddy-e2e` by `tddy-tools restructure` (`move_test_binary_to_crate`).
- The suites stay named for what they prove and keep their original seeds and assertions; the
  move changed only which crate compiles them and which check runs them.
- `tddy-livekit-testkit`'s own suites run in this check too.

## The placement contract (executable)

Two tests state where suites belong; update them in the same commit as any move:

- `tests/livekit_test_placement.rs` here: `MOVED_HERE` pins that the 24 contract suites live in
  this package and nowhere else; `REMAINING_LOGIC_SUITES` names the logic suites still running
  over a real room in the crate that owns them (converting one deletes its line here); and the
  only crates allowed a `tddy-livekit-testkit` dev-dependency are this package and those
  remaining owners.
- `tests/test_placement.rs` in `tddy-daemon`: the owning package's own placement statement for
  its remaining and converted suites, kept independent so a move cannot quietly satisfy one
  copy only.

A placement failure names the offending file and the rule it broke. Fix by moving the suite,
not by adding it to an allow-list; the remainder list shrinks, never grows by convenience.

## How CI runs it

`.config/rust-livekit-e2e.filterset` selects only this package's test binaries and
`tddy-livekit-testkit`'s; the `Rust LiveKit e2e tests` PR check runs them as its own merge
gate, separate from `Rust tests`. The moved binaries also left `.config/rust-e2e.filterset`,
so contract suites run in neither the unit shards nor the e2e leg. The `ci` nextest profile's
LiveKit `slow-timeout` override names this package's binaries. See `docs/dev/guides/ci.md`.

Reference suites against the shared server (`LIVEKIT_TESTKIT_WS_URL`) are the local fast path;
self-containered runs are the CI shape.
