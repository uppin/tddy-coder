# 2026-09-10 — `supervisor_spawn_delegation.rs` is two suites in one file, keeping `tddy-supervisor` in `tddy-daemon`

**Category:** Deferred refactor
**Source:** `#unbundle` node 3, [#472](https://github.com/uppin/tddy-coder/pull/472), milestone M2

M2 moved the spawn/supervisor subsystem into `tddy-spawn` and took `tddy-supervisor` out of
`tddy-daemon`'s `[dependencies]`. It is still in `[dev-dependencies]`, so the milestone's
*"`tddy-supervisor` gone from `tddy-daemon`"* is met **structurally but not literally**.

The reason is one test file. `packages/tddy-daemon/tests/supervisor_spawn_delegation.rs` is really
two suites:

- **Nine tests** exercise `spawner` / `supervisor_spawn` / `supervisor_client` only. These belong in
  `tddy-spawn` and would move unchanged.
- **Two tests** — `refuses_to_start_a_session_when_the_declared_supervisor_is_unreachable:408` and
  `refuses_to_clone_a_project_when_the_declared_supervisor_is_unreachable:452` — mount
  `ConnectionServiceImpl`, `multi_host`, `livekit_peer_discovery` and `claude_cli_session`. They are
  daemon integration tests that happen to assert a spawn-side guarantee.

Moving the file whole would make `tddy-daemon` a **dev-dependency of `tddy-spawn`**, so
`cargo test -p tddy-spawn` would build the entire daemon — defeating the point of the extraction.
Splitting it during M2 would have meant duplicating five helpers and restructuring a suite, which
node 3's constraints forbid (tests move unrewritten, and are never weakened).

## The fix

Split the file: the nine spawn-side tests move to `packages/tddy-spawn/tests/`, and the two
fail-closed integration tests stay in `tddy-daemon` under a name that says what they are (they are
about the daemon refusing to proceed without its declared supervisor, not about delegation
mechanics). The five shared helpers go wherever the majority land, with the daemon-side file
keeping only what its two tests need.

`tddy-supervisor` then leaves `tddy-daemon`'s `[dev-dependencies]` as well, and M2's stated outcome
becomes literally true.

Worth doing once the `#unbundle` stack has landed — splitting a test file while nodes 4–8 are still
rebasing on this branch conflicts every one of them.
