# 2026-10-04 — the Rust e2e leg: what is left after the shared LiveKit server and the parallel run

**Category:** Deferred performance work
**Source:** `#live-plan` 14/15, [#573](https://github.com/uppin/tddy-coder/pull/573), which measured the
first green `Rust e2e tests` run (`37188042911`): a **472 s** run phase at **1.0×** parallelism after a
~1,525 s compile. Taken up by the `#e2e-leg` stack: unique rooms (#578), a configurable peer-forward
deadline and split `rpc_scenarios` (#579), per-binary timing and a compile-share measurement (#580),
one pinned shared LiveKit server with `slow-timeout` (#581), and the LiveKit binaries run in parallel
(#582). This entry keeps only what that stack did not close. The history is in
`docs/dev/changesets/2026-10-0[45]-e2e-leg-*.md`.

## Where the leg stands

Measured on `8e214cd5` (run `37264729977`, three attempts): run phase **344–355 s**, 3.99× parallel,
0 retries in 1,140 test executions; the 168 LiveKit tests finish in a ~105 s window. **The job is not
shorter:** 26m49s–28m28s, because compile is now ~1,090 s of the `cargo nextest` step. Compile is
the leg, and the run is a quarter of it.

Reproduce: `gh run download <run> --name junit-rust-e2e`; the root `<testsuites time=…>` is the run,
each `<testcase time=…>` a test. Compile time is the `cargo nextest` step duration minus that root
`time`.

## Open

### 1. Split the compile by test target — the biggest remaining item

Measured: the e2e leg would skip **53.3%** of its compile, over the 25% gate. Its own entry:
[split the compile by test target](./2026-10-04-split-e2e-leg-compile-by-test-target.md).

### 2. Gates node 4 (#581) left for CI, not yet read

- **The kill-the-server drill** has not been dispatched (`workflow_dispatch` input
  `kill_livekit_after_seconds`, `.github/workflows/ci.yml`). Run it once with the LiveKit binaries in
  parallel. The leg must fail within minutes with tests named as terminated by `slow-timeout` (60 s ×
  3), not wait for the job limit.
- **`timeout-minutes` for the e2e job** is still the shared 150. Warm-cache jobs now take 27–28 min. No
  cold run has been timed since the shared server. Time one, then tighten the cap with a stated margin
  over the cold figure.

Node 1's "two consecutive green runs, and a run with the binaries shuffled" and node 4's "the pin twice
green against a shared server" were met by the parallel runs above: five runs on the pinned server,
the last three green with 0 retries, with binaries interleaved across processes — a stronger mix
than a shuffled serial order.

### 3. The `rust-analyzer` group names 10 of the 31 binaries that drive a live rust-analyzer

The other 21 run in parallel with each other (documented in `docs/dev/guides/ci.md`). Serialising all 31
would cost ~8–10 minutes of run time (their summed time is ~950 s). This entry deferred it until the
rest of the leg was fast enough for this to be the long pole, and with the LiveKit tests parallel it
now is. Decide on measured flakes of the 21 unlisted binaries rather than on the wish for symmetry. Two `tddy-index-daemon` files and one `tddy-tools` file also say they run a live
rust-analyzer and are not in the e2e set. With the LiveKit tests now parallel, the slowest single test
in the leg (~38–41 s) is one of these: `extract_method_signature_acceptance::names_a_parameter_whose_type_a_build_script_generates`.

### 4. Small items

- `common_room_set_metadata_handshake_repro` spends 12 s on purpose: a **negative** assertion that the
  room slot stays populated for 12 s after the initial fill (the SDK's signalling timeout is 5 s). It is
  the property under test. Leave it, or shorten the window with a stated margin over 5 s.
- The LiveKit override in `.config/nextest.toml` (`slow-timeout` only) still lists seven
  `package(tddy-daemon) and binary(…)` pairs for binaries that moved to `tddy-daemon-livekit`,
  `tddy-worktree-service` and `tddy-session-lifecycle`. The package-less `binary(…)` entries keep the
  match correct, but rewrite those pairs under their real owners.
- `session_agent_remote_acceptance::restores_a_clone_that_diverged_and_says_so` (`FIXME(flaky)`)
  reads the divergences as soon as it sees the restored file. The mirror sends the divergence report
  only after restoring it (`tddy-session-agents/src/session_agent_clone.rs`). Wait, within a bound,
  for the divergences to be non-empty. It did not flake in the measured runs.
- `multi_host_acceptance` still uses three fixed session ids (`sess-new-7f3a`, `sess-host-check`,
  `session-owned-by-a-only`). Check whether any becomes a room on the shared server. If one does, take
  it from `unique_room` (the rule in `docs/dev/guides/testing.md`).

## What would close it

The compile split landed and measured, the drill run and recorded, the cap tightened to a measured
cold time, and each small item either fixed or recorded with a reason it stays.

**Do not** put a branch in production code that only the tests take, and do not weaken a deadline test
into a bare `is_err()`: the test asserts `DeadlineExceeded` on purpose, so that a missing deadline cannot
pass as an up-front refusal.
