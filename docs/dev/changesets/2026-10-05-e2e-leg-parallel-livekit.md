# 2026-10-05 — The LiveKit e2e binaries run in parallel

**Type:** Feature (CI infrastructure)

`#e2e-leg` 5/5 (top node), PR [#582](https://github.com/uppin/tddy-coder/pull/582),
`feature/e2e-leg/parallel-livekit`, base `master` (its parent, #581, merged during the work).
Consumes node 1's unique rooms (#578) and node 4's shared, pinned LiveKit server and `slow-timeout`
(#581).

## What shipped

- **The serial `docker` test-group is gone.** All 35 binaries that start the LiveKit testkit run with
  nextest's default parallelism against the one shared server. The `ci` override that listed them
  still exists and carries only node 4's `slow-timeout`; its filter is unchanged.
- **Drift check, derived from source.** `scripts/nextest-serial-groups.ts` finds every test binary
  whose source calls `LiveKitTestkit::start`, and reads every `max-threads = 1` group out of
  `.config/nextest.toml`. `default`-profile overrides are applied as nextest applies them, and the
  first override that matches decides. `scripts/nextest-serial-groups.test.ts` fails, naming each
  binary, if a testkit binary is serialised again, and fails if a serial group names a binary that
  exists nowhere. It covers workspace members (including `tddy-desktop/src-tauri`),
  `tests/<dir>/main.rs` targets and `mod`/`#[path]` modules. It throws on `[[test]]`/`autotests`
  manifests, a `ci` profile that `inherits`, and filter shapes it does not support. It is run locally,
  not by CI ([todo](../todo/2026-10-05-ci-script-tests-are-not-run-in-ci.md)).
- **One filterset parser.** `scripts/nextest-filterset.ts` is shared by the drift check and node 3's
  `scripts/ci-e2e-timing.ts`. It throws on any character or predicate it does not recognise; node 3's
  version had silently dropped `!` and glob syntax.
- **Two tests that had passed only because they ran alone, fixed in their own packages:**
  - `tddy-daemon` `session_agent_remote_acceptance`: one fixed session id meant concurrent fleets met
    in one `session-{id}` room under identical daemon identities. A later join evicted an earlier
    fleet's daemon, so a clone's READY report reached another process and the clone stayed
    PROVISIONING for 90 s. The session id now comes from `unique_room`. Locally at
    `--test-threads 8`, 13 of 22 tests failed before the fix and none after.
    [testing.md](../guides/testing.md#test-rooms-come-from-unique_room) now says names that become rooms
    count too.
  - `tddy-e2e` `stdio_remote_control_acceptance`: the gRPC port was probed on `127.0.0.1` while
    `tddy-coder --grpc` binds `0.0.0.0`. On Linux, a port held on another local address (a LiveKit
    client's WebRTC TCP candidate) passes the probe and fails the bind. It is now probed on `0.0.0.0`.
    In a Linux container, 7.2% of loopback-probed ports failed the bind; 0 failed with the wildcard
    probe.
- Node 4's `the_docker_override_kills_a_stuck_test` locates the override by its filter and is renamed
  `the_livekit_override_kills_a_stuck_test`. Its assertion is unchanged.
- `retries` stays at `count = 2`, with its reason rewritten. The port race it was added for is gone, but
  the `ci` profile also runs `Rust tests`, and known load-sensitive tests remain. Lower it once those
  are fixed.

## Measured (JUnit `junit-rust-e2e`; run phase = root `<testsuites time>`)

| Run | Head | Run phase | Parallelism | LiveKit window (168 tests) | Flaky reruns |
|---|---|---|---|---|---|
| baseline, master 37232820570 | `f014ee78` (group serial) | 477.8 s | 2.03× | 328.3 s (= their summed time) | 0 |
| 37259623964 | `82f184a3` (lifted) | 450.3 s | 2.92× | 228.4 s | 6 |
| 37261994899 | `c5131152` | 477.0 s | 2.58× | 265.6 s | 7 |
| 37264729977 #1 | `8e214cd5` (both fixes) | 344.1 s | 3.99× | 100.9 s | 0 |
| 37264729977 #2 | `8e214cd5` | 348.9 s | 3.99× | 104.5 s | 0 |
| 37264729977 #3 | `8e214cd5` | 354.7 s | 3.99× | 107.7 s | 0 |

- With the fixes, the run phase is 478 s → 344–355 s (−27%), and three consecutive runs had 0
  retries in 1,140 test executions. At 4.0× on four vCPUs, the runner's CPU is the limit.
- **The job did not get shorter.** It took 26m49s–28m28s, against 25m13s for the baseline, because
  the compile part of the `cargo nextest` step took ~1,090 s against ~780 s. The compile is variable
  between runs, and this change touches no dependency. The compile split
  ([todo](../todo/2026-10-04-split-e2e-leg-compile-by-test-target.md)) is what attacks it.
- The two flaky runs show why measuring mattered. Every retry of the fixed-session-id test waited out
  90 s, so a run with retries can be as slow as the serial baseline.

## Left open

- The override's filter still holds seven dead `package(tddy-daemon) and binary(…)` pairs, for
  binaries that moved to `tddy-daemon-livekit`, `tddy-worktree-service` and `tddy-session-lifecycle`.
  The package-less `binary(…)` entries keep the match correct. Recorded in the source note.
- `session_agent_remote_acceptance::restores_a_clone_that_diverged_and_says_so` (already
  `FIXME(flaky)`): the test reads divergences before the report arrives. It did not flake in the
  measured runs.
- Three fixed session ids remain in `multi_host_acceptance`. They did not flake, and it is unverified
  whether any of them names a room.
