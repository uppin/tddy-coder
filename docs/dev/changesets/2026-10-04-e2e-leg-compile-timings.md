# 2026-10-04 — CI reports where the e2e leg's compile and run time go

**Type:** Feature (CI observability)

`#e2e-leg` 3/5, PR [#580](https://github.com/uppin/tddy-coder/pull/580),
`feature/e2e-leg/compile-timings`. No Rust package changed; the change is root CI.

Two numbers that decide the rest of the e2e-leg work are now visible without downloading artifacts:

- **Per-binary run time.** The `Rust e2e tests` step summary lists every test binary, slowest first, with
  its time and test count and the total, read from the JUnit nextest already writes. It is written also
  when tests fail.
- **Compile share.** A manual-dispatch-only job, `Rust compile timings`, runs a cold
  `cargo test --no-run --workspace --locked --timings`, uploads the report as the `cargo-timings`
  artifact and writes the share of compile time spent on e2e test targets, on other test targets and on
  everything else, with a `proceed`/`stop` verdict against a 25% threshold.

Both are `scripts/ci-e2e-timing.ts` (`junit` and `compile-share` commands), a dependency-free script
over file contents, so its tests need no cargo. The e2e set is evaluated from
`.config/rust-e2e.filterset` itself — `binary()`, `package()`, `kind(test)` with `and`/`or`/`not` — so a
term such as `package(tddy-daemon) and binary(first_login_enrolment_acceptance)` counts one binary, not the
package's eighty. A predicate it cannot evaluate is an error, and filterset binaries absent from the report
are named. The CI guide (`docs/dev/guides/ci.md`) describes both.

## Final measurement

The first dispatch (run 37220592932) on a cold cache, 6,683 s of summed unit compile time:

| | Seconds | Share |
|---|---:|---:|
| e2e test targets | 560 | 8.4% |
| other test targets | 3,563 | 53.3% |
| everything else | 2,561 | 38.3% |

The e2e leg would skip **53.3%** of its compile, over the 25% gate: **proceed** with splitting the compile
by test target. The unit leg would skip 8.4%. The shares are of summed, parallel unit durations, so they
approximate the wall-clock saving.

That run's own step summary read `0%` / `stop`: the parser had been written against a report shape cargo
does not produce (`mode: "test"`), whereas a real report has `mode: "todo"` and marks a test unit in its
target string. The parser and fixtures follow the real shape; the figures above come from re-reading the
uploaded artifact with the corrected script.

## Backlog

- **Narrowed, kept:** `2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits` — the per-binary timing
  and the `--timings` measurement are delivered; the rest of that entry is still open.
- **Filed:** `2026-10-04-split-e2e-leg-compile-by-test-target` (the split the measurement gates) and
  `2026-10-04-confirm-compile-timings-summary-on-ci` (the corrected summary has not yet run in the job).
- **Untouched:** `2026-09-09-keeping-target-from-overhogging-the-disk` — a `--timings` build compiles every
  test target, on manual dispatch on a runner that already reclaims disk; recorded, not fixed.
