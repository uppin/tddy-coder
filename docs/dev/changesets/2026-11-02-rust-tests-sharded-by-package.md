# 2026-11-02 — The `Rust tests` leg is sharded by package across three runners

**Type:** Fix (CI)

Guide: [Continuous integration](../guides/ci.md#rust-tests-and-rust-e2e-tests).

`Rust tests` ran the whole workspace on one runner in two windows, and even half of the ~1,250 test
binaries no longer fit it (run 41440040890: the runner itself died with `No space left on device`).
`scripts/nextest-windowed.ts` gains `--shard <i>/<n>`, which cuts the packages with test binaries into
`n` contiguous groups balanced by binaries (3 groups: 428 / 444 / 374 binaries) before windowing each
in `--batches 2`. A malformed, out-of-range or empty shard is an error. `--plan` prints the batches
without building.

`ci.yml`: `rust-test` becomes the matrix `Rust tests shard i/3` (`rust-test-shard`, `fail-fast: false`,
JUnit as `junit-rust-shard-i`), and a new `Rust tests` aggregator (modelled on `Rust e2e tests`, with
`!cancelled()` and the gate) keeps the check name branch protection requires. rust-cache: each shard
is the sole writer of `test-shard-i`; the e2e archive restores `test-shard-1` read-only.
