# 2026-10-04 — Split the Rust compile by test target, so each CI leg builds only its own tests

**Category:** Future enhancement (CI throughput)
**Source:** the `#e2e-leg` stack, node 3 — the measurement gate in § 3a of
[the e2e leg note](./2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md) came back **proceed**.

## What and why

Both legs run `nextest run --workspace`, and nextest builds every test target of every package before the
filterset decides what to run. So `Rust tests` compiles and links the e2e binaries it will never run, and
`Rust e2e tests` compiles and links the ~700 other test binaries it will never run.

**Measured** (the manual `Rust compile timings` job: a cold `cargo test --no-run --workspace --locked
--timings`, report artifact `cargo-timings`, run 37220592932, read by `scripts/ci-e2e-timing.ts
compile-share`). Of 6,683 s of summed unit compile time:

| | Seconds | Share |
|---|---:|---:|
| e2e test targets | 560 | 8.4% |
| other test targets | 3,563 | 53.3% |
| everything else (dependencies, libs, build scripts) | 2,561 | 38.3% |

- The **e2e leg would stop compiling 53.3%** — well over the 25% gate, so the split is worth doing.
- The **unit leg would stop compiling only 8.4%**. Most of the gain is on the e2e leg.
- Every binary in `.config/rust-e2e.filterset` appears in the report.

**The shares are of summed unit durations**, which cargo runs in parallel, so they approximate the
wall-clock saving rather than measuring it. The first split leg's real compile time is the evidence that
counts — see *What would close it*.

## The design

Already worked out and verified on a throwaway workspace; it is written down in § 3a of the note linked
above ("Design" and "Trade-offs to decide, not assume") and is not repeated here. In short: an empty `e2e`
feature on each package that owns an e2e test target, `required-features = ["e2e"]` on each such
`[[test]]`, the unit leg unchanged (cargo skips the gated targets), the e2e leg given a **generated**
`--features … --test …` list so the manifests become the single source of what is e2e and
`.config/rust-e2e.filterset` goes away.

Decide, in this order, before writing any manifest:

1. **What `./test` and a plain `cargo test` do locally.** Gated targets are skipped without the feature.
   Either `./test` enables every `e2e` feature by default, or it grows a flag — document the choice in
   `AGENTS.md`'s Commands table. A developer who silently stops running the LiveKit tests is the failure
   to avoid.
2. **Whether the filterset file survives.** `scripts/ci-e2e-timing.ts compile-share` reads it to decide
   what counts as an e2e target; if it goes away, the script must read the manifests instead (or the
   generated target list), or the next measurement silently counts nothing.

## Why it was left

The change touches ~12 shared `Cargo.toml` files and ~77 `[[test]]` stanzas, and changes what every local
test command runs — it is its own PR, and it was gated on the measurement, which only now exists.

## What would close it

- The e2e leg's **compile phase** measurably shorter than the ~1,525 s baseline (read it off the leg's
  step timings on two runs, with a warm and a cold cache), and the unit leg's no longer.
- **No loss of coverage**: the `tests run` count of the two legs together equals the count of one
  un-split run.
- A lint in `Rust lint` that fails when a test file uses `LiveKitTestkit`, the restructuring `harness`, or
  `CARGO_BIN_EXE_tddy-supervisor`/`-index-daemon`/`-daemon`/`-coder` and is **not** gated — otherwise a new
  e2e test lands in the unit leg unnoticed.
- The two decisions above written into `AGENTS.md` and `docs/dev/guides/ci.md`.

If the wall-clock saving turns out much smaller than the 53.3% proxy suggests, the alternative is
`cargo nextest archive` (one compile, both legs run from it); it halves CPU rather than shortening the
critical path, and the note compares the two.
