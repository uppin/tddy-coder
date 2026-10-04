# 2026-10-04 — the Rust e2e leg: a shared LiveKit server, deadline waits it sits out, and a compile it pays twice

**Category:** Deferred performance work
**Source:** `#live-plan` 14/15, [#573](https://github.com/uppin/tddy-coder/pull/573) — measured on the first
green run of the `Rust e2e tests` check (run `37188042911`, head `ddd0c0cd`; JUnit artifact `junit-rust-e2e`),
before the 31 restructuring binaries joined the set. Numbers below are that run's unless said otherwise.
Everything under **Verified** was read in the code or measured; everything under **Not verified** is a
hypothesis the next agent must test before building on it.

## Where the time goes

| | `Rust e2e tests` |
|---|---|
| Job | 35m04s |
| `cargo nextest` step | 1,997s = **compile ~1,525s (76%)** + **run 472s (24%)** |
| Tests | 224 in 46 binaries; 159 of them (35 binaries) start a LiveKit server |
| Parallelism in the run | **1.0×** — 482s of summed test time in a 472s run |
| Per-test | median 1.1s, mean 2.2s, p90 4.7s, max 34.9s |

By test time: 66 tests of 2–5s hold 51% of it (245s); 4 tests of 10s+ hold 23% (110s); 82 tests under
0.5s hold 1%. So it is a **wide middle** (every LiveKit test is a handful of seconds) plus **four deadline
waits**, run **one at a time**.

Reproduce: `gh run download <run> --name junit-rust-e2e`; the root `<testsuites time=…>` is the run, each
`<testcase time=…>` a test. Compile time is the `cargo nextest` step duration minus that root `time`.

## The shape of the fix, in order of payoff

### 1. One LiveKit server for the whole job, shared across every test suite

**Verified.**

- `LiveKitTestkit::start()` (`packages/tddy-livekit-testkit/src/livekit_testkit.rs`) launches a **new
  `livekit/livekit-server:master` container per call** and waits for its Twirp API. The container lives
  in the struct and is dropped with it. **Every one of the 159 tests pays this**; no test file holds a
  static or lazy testkit. A start is ~0.5s (the testkit's own two tests take 0.99s together), so ~80s of
  the 472s — real, but not the biggest part.
- The reuse path already exists: with `LIVEKIT_TESTKIT_WS_URL` set, `start()` skips the container and
  only waits (≤ 15s, `API_READY_TIMEOUT`) for the API. `run-livekit-testkit-server` is the local
  front end (one named, persistent container). **CI sets nothing**, so none of it is used there.
- Why the tests are serial today: `.config/nextest.toml`'s `docker` test-group (`max-threads = 1`) exists
  because each container binds host ports found by `bind(:0)`-and-release, a TOCTOU race. A shared
  server has no per-test port race, so **the reason for the serialisation goes away**, and with it the
  1.0× parallelism. That is where the larger win is, not the 80s of container starts.
- **No test stops, restarts or pauses the server** (grepped `drop(`/`stop`/`kill`/`pause` over the
  LiveKit tests), so none depends on owning its server's lifecycle.

**Isolation — a hard precondition, do it before sharing anything.** On a shared server two tests that
use the same room name, or the same identity in the same room, interfere. What the code looks like today:

- ~22 fixed room-name constants, several **shared between binaries**: `"tddy-lobby"` in
  `cross_crate_session_token_acceptance`, `token_service_acceptance` and `session_tool_livekit_dispatch`;
  `"acceptance-common-room"` in `livekit_peer_daemons_acceptance` and `multi_host_acceptance`. Others are
  per-binary constants (`"agent-roster-common-room"`, `"attach-cross-host-room"`, …), which collide across
  **tests inside one binary** as soon as those run in parallel.
- Two binaries already do the right thing and are the pattern to copy: `forwarded_rpc_is_stamped_by_the_receiver`
  (`COMMON_ROOM_PREFIX`) and `session_room_acceptance` (`COMMON_ROOM_PREFIX`) build a name from a prefix.
- Add one testkit helper — e.g. `LiveKitTestkit::unique_room(prefix)` returning `prefix-<short random>` —
  and migrate every constant to it. Tokens are minted per room (`generate_token(room, identity)`), so the
  API key/secret (`devkey`/`secret`) can stay shared.
- **Tests that read server-global state need individual treatment** (they would see every other test's
  rooms): `stream_livekit_rooms_rpc`, `room_roster_livekit`, `room_roster_deadline`,
  `session_room_acceptance`, `local_token_uds`, and the roster code they exercise
  (`tddy-livekit/src/room_roster.rs`, `tddy-daemon-livekit/src/livekit_rooms_stream.rs`,
  `tddy-session-agents/src/session_room_participants.rs`). Each either filters by its own unique prefix or
  stays on a dedicated server. Decide per test; do not assume the filter exists.
- Leaked rooms (a test that aborts) are harmless once names are unique: LiveKit `--dev` closes an empty
  room after its empty timeout. Verify that figure for the pinned image rather than trusting this line.

**Lifecycle — it must not be able to hang the suite.** The server has to outlive every test binary, so it
cannot be owned by a test (`ContainerAsync` drop) — it is owned by whatever runs the suite.

- **CI:** two workflow steps around `cargo nextest`, in the e2e leg only: *start* (`docker run -d --rm`
  on dynamic ports, wait for the API, append `LIVEKIT_TESTKIT_WS_URL=…` to `$GITHUB_ENV`) and *stop*
  (`if: always()`, `docker rm -f`). The runner is ephemeral, so a leaked container dies with the job; the
  stop step is for the log and for a re-run on a reused runner.
- **Local:** keep `run-livekit-testkit-server` as the reuse path and add `--stop`, and make the testkit's
  *no URL set* path unchanged, so a bare `cargo test` still works with no setup.
- **nextest cannot do this itself** as far as I know: `[scripts.setup.*]` runs a command before the run and
  can export env through `$NEXTEST_ENV`, but there is **no teardown hook** — **not verified against
  0.9.132; read its docs before relying on this**. That is why the CI steps own it.
- **Hang protection, in layers** (none of these exists today; `nextest.toml` has no `slow-timeout`):
  1. *Start:* the readiness wait is already bounded (15s); give the start step a short `timeout-minutes`,
     and fail the job loudly if the API never answers.
  2. *Mid-run death:* if the server dies, tests hang on their own awaits. Add a nextest
     `slow-timeout = { period = "<>", terminate-after = <> }` for the e2e filter so a stuck test is
     **killed and reported**, not waited out. Size it above the longest legitimate test (the inner timeouts
     in `rpc_scenarios` are up to 20s, `common_room_set_metadata_handshake_repro` waits 60s) — measure,
     don't guess.
  3. *Fast diagnosis:* have `start()` (env-URL path) fail with a message naming the URL when the API does
     not answer, instead of the generic timeout.
  4. The job's `timeout-minutes` (150 today) is the last backstop; tighten it for this leg once the real
     runtime is known.
- **Pin the image.** `livekit/livekit-server:master` is a floating tag: a different server on every run,
  and a pull each time. Pin a digest (or a release tag), pre-pull in the start step, and note the version
  in the log.

**Then** remove the LiveKit binaries from the `docker` test-group, so they run in parallel (the runner has
4 vCPUs, and each test starts several in-process daemons, so expect CPU, not the server, to be the limit).
**Not verified:** the speed-up. It is bounded by CPU and could be anywhere from ~1.5× to ~3×; measure with
the same JUnit script before and after. Do it in two steps — shared server first with the group still
serial (proves isolation and lifecycle), then lift the group (proves the parallelism).

### 2. The deadline waits (~110s of 472s, 4 tests)

**Verified.**

- `fails_only_the_agents_of_a_daemon_that_goes_away` (34.9s) and
  `a_forwarded_rpc_to_a_peer_that_stopped_answering_fails_within_its_deadline` (31.2s) both end by waiting
  out `PEER_FORWARD_TIMEOUT` = **30s** (`tddy-daemon-kernel/src/peer_forwarding.rs:93`). That is real time over a
  real network, so there is no clock to pause.
- The seam is half there: `forward_to_peer_within(…, deadline)` takes the deadline, and `forward_to_peer`
  is a one-line wrapper over it with the constant. But ~10 call sites use the wrapper
  (`tddy-daemon-livekit/src/livekit_peer_discovery.rs`, `peer_routing.rs`, `tddy-host-service/src/service.rs`),
  and `forward_server_stream_to_peer` uses the constant directly, twice.
- Two ways to close it — pick one **deliberately**:
  - **(a) Make the deadline a real daemon setting** (`peer_forward_timeout_seconds`, default 30) threaded to
    those call sites; the tests set 2s. This is production configuration with a production default, not a
    test branch. Keep the guard that `PEER_FORWARD_STREAM_IDLE_TIMEOUT` and
    `PASS_LONG_ENOUGH_TO_BE_SERVICE` constrain each other (a test in `session_agent_roster_acceptance.rs`
    pins it), and update the comment in the deadline test that says its outer 60s is "longer than the 30s
    deadline".
  - **(b) Fail faster for a peer known to be gone.** The first test's peer has *left the room*; waiting 30s
    to learn that is arguably a product defect, and `svc_start_hosted_agent_clone.rs:124` already says
    "waiting out `PEER_FORWARD_TIMEOUT` reaches the same answer thirty seconds [later]". Check whether
    `peer_client` can see the participant is absent and answer `Unavailable` at once. This changes what
    the second test (a peer that is **present but silent**) proves, so it still needs (a).
- `rpc_scenarios` (30.8s) is **one test** that runs every scenario in sequence, with a `sleep(2s)` and
  10–20s inner timeouts. Each scenario already takes a `room_name` parameter. Split it into one test per
  scenario: the scenarios then run in parallel once (1) lands, and a failure names the scenario.
- `common_room_set_metadata_handshake_repro` (13.3s) spends 12s on purpose: a **negative** assertion that
  the room slot stays populated for 12s after the initial fill (the SDK's signalling timeout is 5s). It is
  the property under test. Leave it, or shorten the window with a stated margin over 5s — low priority.

### 3. The compile is paid twice (~1,525s, 76% of the leg)

**Verified:** both test legs reported `rust-cache: No cache found.`; they compile the same `--workspace`
test targets independently, from scratch. `master` holds caches for `rust-lint`, `rust-build`, `rust-vm`
and arm64 but **none for the test job**, and the repo's Actions cache is **9.4 of 10 GB**.
**Not verified:** *why* master never saves a test cache (size pressure and a failing save step are the two
candidates; the master log I read was inconclusive). Find that out first — it changes everything below.

- Restricting by *package* (`-p`, `--exclude`) is too coarse: the e2e binaries sit in packages that also
  hold unit tests. **Restricting by test *target* is possible — see 3a.** (An earlier version of this note
  said the unit leg cannot skip compiling the e2e binaries; that was wrong. Cargo has no "exclude this
  target" flag, but `required-features` gives the same effect.)
- `cargo nextest archive` once, then run both legs from the archive, turns two compiles into one. It does
  **not** shorten the critical path by itself (the compile still precedes the run); it halves the CPU spent
  and removes the second cold cache. Worth it only after the cache question is answered.

### 3a. Split the compile by test target: e2e builds only e2e targets, the rest never builds them

**The goal.** `Rust tests` must not compile the e2e test binaries, and `Rust e2e tests` must not compile
the other ~700. Today both run `nextest run --workspace` and nextest builds every test target of every
package before the filterset decides what to run, so each leg compiles and **links** the other's tests
too. Each file in `tests/` is its own crate and its own binary (758 of them), so this is likely a real
share of the 1,525s — **not measured; see the gate below.**

**Verified** on a throwaway two-crate workspace (cargo and nextest, via `./dev`; not on this repo):

| Command | What gets built |
|---|---|
| `cargo test --no-run --workspace` where `[[test]] name = "a_e2e"` has `required-features = ["e2e"]` | the lib unit tests and every ungated integration test; **the gated target is silently skipped** |
| `… --workspace --features a/e2e,b/e2e --test a_e2e --test b_e2e` | **only** those two test executables — no lib unit tests, no other integration test |
| `… --workspace --test a_e2e` without the feature | an error naming the missing feature (so the feature must always be passed) |
| `--test <name>` where the name exists in only one workspace member | fine; the other members are not an error |
| `cargo nextest list/run` with the same flags, and `-E <filterset>` on top | identical selection; the filterset still composes |

**Design.**

1. Give every package that owns an e2e test target an empty feature `e2e = []` and mark each e2e target
   `[[test]] name = "…" required-features = ["e2e"]`. Auto-discovery keeps working for every other file
   in `tests/`; the stanza only configures the one it names. An empty feature changes no dependency, so
   both legs still share one compiled dependency graph (and one cache).
2. **Unit leg:** the command is unchanged (`nextest run --workspace --profile ci`); the gated targets are
   skipped by cargo, so they are not compiled. The `not (…)` filterset becomes unnecessary.
3. **E2E leg:** `nextest run --workspace --profile ci --features <pkg>/e2e,… --test <name> --test <name> …`.
   Generate the argument list instead of writing it: `cargo metadata --format-version 1` lists each
   target's `required-features`, so a small script (`scripts/rust-e2e-targets.sh`) can print every target
   gated on `e2e` and the `pkg/e2e` features to enable. **The Cargo.toml stanzas then become the single
   source of what is e2e**, and `.config/rust-e2e.filterset` goes away (its `kind(test)` and
   `package(...)` terms are only approximations of it).
4. **Lib unit tests of the e2e packages** (e.g. the 176 in `tddy-supervisor`, the 761 in
   `tddy-code-restructuring`) stay in the unit leg, as now.

**Trade-offs to decide, not assume.**

- **Local runs change.** `cargo test` and `./test -p tddy-daemon` would skip the e2e targets unless the
  feature is on. Either `./test` enables every `e2e` feature by default (and CI's unit leg is the only
  place they are off), or it grows a flag. Pick one and document it in `AGENTS.md`'s Commands table;
  a developer who silently stops running the LiveKit tests locally is the failure to avoid.
- **A new e2e test that is not gated** runs in the unit leg without anyone noticing. Keep a cheap lint
  (`scripts/…`, run in `Rust lint`) that fails when a test file uses `LiveKitTestkit`, the restructuring
  `harness`, or `CARGO_BIN_EXE_tddy-supervisor`/`-index-daemon`/`-daemon`/`-coder` and is **not** gated.
  The criteria are derivable, which is what makes the lint possible.
- **Churn:** ~12 `Cargo.toml` files and ~77 `[[test]]` stanzas (46 binaries in the original set + the 31
  restructuring ones). Mechanical, but it touches shared manifests, so land it on its own.
- **What it does not save.** The dependency graph — the `webrtc` stack, the daemon and index-daemon
  libraries — compiles in both legs regardless. A leg saves its *test-target* compile and link, and
  nothing else. If most of the 1,525s is dependencies, this changes little.

**Gate, before any Cargo.toml churn.** Measure the share. Run `cargo test --no-run --workspace --timings`
(or `cargo build --timings` on the test profile) in CI once and keep the HTML report as an artifact: test
targets appear as `pkg (test "name")` with their own durations. Sum them for the e2e targets and for the
rest. **Proceed only if the target the leg would skip is a meaningful share** — a rule of thumb: ≥ 25% of
the leg's compile. Below that, prefer the cache fix and `nextest archive` above, which attack the
dependency compile.

**Alternatives considered.**

- *Dedicated e2e crates* (move the test files out of their packages): removes the need for features,
  but moves ~77 files and their fixtures, and breaks `CARGO_BIN_EXE_*` for the tests that exec their own
  package's binary. Too invasive for this.
- *`nextest archive`* — one compile, two legs run from it. The opposite trade (one build, no split);
  it saves CPU, not wall-clock, and it keeps compiling the other leg's tests. Compare it with 3a on
  measured numbers; the two are not combinable without losing the point of either.

### 4. Smaller items

- **The `rust-analyzer` nextest group names 10 of the 31 restructuring binaries that use the live harness.**
  The other 21 run in parallel with each other (documented in `docs/dev/guides/ci.md`). Serialising all 31
  would cost ~8–10 minutes of run time in this leg (their summed time is ~950s) — so do it **after** (1)
  and (2), when the rest of the leg is fast enough that it is the long pole. Two `tddy-index-daemon` files
  and one `tddy-tools` file also say they run a live rust-analyzer and are not in the e2e set.
- **Report the e2e per-binary timing in the check summary**, so the next regression shows up as a number
  rather than a 35-minute job.

## Why it was left

It was found while profiling a CI split that was asked for, not while changing the code it measures, and
(1) changes shared test infrastructure used by 35 binaries, which is its own PR.

## What would close it

The e2e leg's **run phase** well under 472s with **no loss of coverage** — compare `tests run` against the
`Rust e2e tests` count on the base run — and **no hang path**: kill the server mid-run on purpose and see
the leg fail in minutes with a named test, not time out at the job limit. Concretely, in this order:

1. Unique-room helper in the testkit; migrate the constants; settle the room-enumerating tests. Land with
   the `docker` group still serial. **Gate:** two consecutive runs green, and a run with the binaries
   shuffled (`--test-threads` / nextest `--partition`) green.
2. CI start/stop steps and the pinned image; `slow-timeout` for the leg. **Gate:** the kill-the-server drill.
3. Lift the `docker` group for the LiveKit binaries. **Gate:** measured run time and a flake count over
   several runs.
4. The deadline setting (and the split of `rpc_scenarios`).
5. The cache question, then the `--timings` measurement from 3a. If test targets are a meaningful share,
   do 3a (features, `required-features`, the generated target list, the lint); otherwise `nextest archive`.

**Do not** put a branch in production code that only the tests take, and do not weaken a deadline test
into a bare `is_err()`: the test asserts `DeadlineExceeded` on purpose, so that a missing deadline cannot
pass as an up-front refusal.
