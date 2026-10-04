# Changeset: CI reports where the e2e leg's compile and run time go

**Date**: 2026-10-04
**Status**: 🚧 In Progress
**Type**: Feature (CI observability)

Node 3 of 5 of the `#e2e-leg` stack, PR [#580](https://github.com/uppin/tddy-coder/pull/580) (branch `feature/e2e-leg/compile-timings`, PR base `feature/e2e-leg/deadline-and-scenarios`). It consumes nothing from nodes 1 or 2.

**Green state:** all five script tests pass; the workflow steps and `ci.md` are in. Still open: dispatch `Rust compile timings` once, then record the verdict in the source note. The `UNIT_DATA` shape for lib/bin unit-test units (no `(test "x")` target) is assumed to be `mode: test` with the package as `name` — unverified against real cargo output until that run.

**Contract state (commit 2, now implemented):** `scripts/ci-e2e-timing.ts` publishes `perBinaryTimings`, `compileShare`, `verdict` and `SPLIT_WORTH_IT_PERCENT` with `throw` bodies; all five tests in `scripts/ci-e2e-timing.test.ts` fail on them (verified, 5 of 5; inline fixtures, no fixture files). Run with `./dev bun test ./scripts/ci-e2e-timing.test.ts` (bun needs the `./` to treat it as a path). The acceptance-test review gate was not held separately (the developer asked for the whole stack to be prepared without stopping).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-04-e2e-leg-compile-timings-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Affected Packages

- Root CI: `.github/workflows/ci.yml`, new `scripts/ci-e2e-timing.ts` (+ `.test.ts`), [docs/dev/guides/ci.md](../guides/ci.md)
- No Rust package changes.

## Related Feature Documentation

No product requirement changes. Source note: [2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md) (§ 3a gate and § 4 "report per-binary timing").

## Summary

Make the two numbers that decide the rest of the work visible without anyone downloading artifacts by hand: (1) the **per-binary run time** of the e2e leg, written to the check's step summary on every run, and (2) the **share of compile time spent on test targets** that the e2e leg would skip, from a manual `cargo --timings` run. The second is the gate for the compile split (§ 3a of the note): proceed only if the skippable share is at least 25% of the leg's compile.

## Background

The e2e leg is 35 minutes of which about 1,525 s is compile and 472 s is run. Splitting the compile by test target (feature-gating the e2e binaries) touches ~12 manifests and ~77 `[[test]]` stanzas, and saves only the test-target compile and link — not the dependency graph. Whether that is worth doing is an empirical question the note says to answer first, and every regression after this should show up as a number rather than a 35-minute job.

## Prerequisites

### ⚠ DURING — the source note — [`2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md`](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md)

Delivers step 5's measurement (§ 3a gate) and § 4's "report per-binary timing". Narrowed at wrap, not deleted. Its cache question (§ 3) was answered by the note itself (the e2e leg now shares the `test` key, restore-only); this node does not re-measure it.

### ⚠ DURING — [`2026-09-09-keeping-target-from-overhogging-the-disk.md`](../todo/2026-09-09-keeping-target-from-overhogging-the-disk.md)

A `--timings` build compiles every test target (the 80-binary `tddy-daemon` case). It runs only on manual dispatch, on a runner that already reclaims disk, so it does not add to `target/` on a developer machine. Recorded, not fixed here.

## Scope

- [x] `scripts/ci-e2e-timing.ts` with two commands: `junit <path>` (per-binary run time table) and `compile-share <timings.html> <filterset>` (share of compile on e2e test targets vs the rest)
- [x] The e2e leg appends the per-binary table to `$GITHUB_STEP_SUMMARY` after nextest (also when tests fail)
- [x] A manual-dispatch-only job `Rust compile timings` runs `cargo test --no-run --workspace --locked --timings`, uploads the HTML report as an artifact and writes `compile-share` to its summary
- [ ] The measured verdict (share, threshold 25%, decision) is recorded in the source note when this node is greened

## Technical Changes

### State A (Current)

- `ci.yml` has `workflow_dispatch:` at the top level. The `rust-test` matrix runs `nextest run --workspace --profile ci -E <filter>`, publishes JUnit (`junit-rust`, `junit-rust-e2e`) and a detailed report; there is no per-binary summary and no `--timings` anywhere.
- The e2e set is `.config/rust-e2e.filterset` (binary names and packages).
- Precedent for a script with a test: `scripts/generated-code.sh` + `scripts/generated-code.test.ts` (bun test).

### State B (Target)

- Every e2e run shows a per-binary run-time table, sorted by time, with the total.
- A manually dispatched job produces a cargo timings report and a one-line verdict on the e2e share of compile time.

### Delta

- `scripts/ci-e2e-timing.ts`, `scripts/ci-e2e-timing.test.ts`, inline fixtures (a JUnit document, and a timings report in the shape cargo writes: a `UNIT_DATA` array with `name`, `target`, `mode`, `duration`).
- `.github/workflows/ci.yml`: summary step in the `rust-test` job (e2e leg), new `workflow_dispatch`-guarded job.
- `docs/dev/guides/ci.md`: describe both.

## Implementation Milestones

- [x] Script and its tests against fixtures
- [x] Summary step in the e2e leg
- [ ] Manual timings job, run once, report kept — job added; **not yet dispatched** (needs the branch on GitHub)
- [ ] The verdict recorded and the decision written into the source note

## Testing Plan

### Testing Strategy

Unit tests (bun) over fixture inputs for both commands; the workflow is verified by running it on CI.

### Testing Principles Applied

The script has no network or repo access of its own: parsers take file contents, so tests need no cargo. The compile-share parser reads cargo's `UNIT_DATA` block from the HTML report; a fixture pins the shape, so a future cargo change fails a named test rather than producing a wrong number silently.

### Coverage Requirements

Both commands: normal input, empty input, binaries in the filterset that never appear in the report (named in the output, not hidden).

## Acceptance Tests

### root scripts

- `scripts/ci-e2e-timing.test.ts`
  - `the_per_binary_table_is_sorted_by_time_and_totals_the_binaries`
  - `a_junit_file_without_test_cases_reports_an_empty_table_not_a_crash`
  - `the_compile_share_counts_only_test_targets_named_by_the_filterset`
  - `a_filterset_binary_missing_from_the_report_is_named_in_the_output`
  - `the_verdict_says_proceed_at_or_above_twenty_five_percent_and_stop_below`

## Technical Debt & Production Readiness

(Populated during development.)

## Decisions & Trade-offs

- **Manual dispatch for `--timings`.** It compiles every test target (cold, ~20+ minutes) and the measurement is a one-off decision input; running it on every PR would add a leg's worth of CI for a number that changes slowly.
- **Per-binary table on every e2e run.** It is derived from JUnit already produced, so it costs nothing.
- **25% threshold** is the note's rule of thumb, stated here as the gate and recorded in the verdict output.
- **`nextest --cargo-timings` not assumed.** Whether nextest forwards `--timings` is unverified; the standalone `cargo test --no-run --timings` is what the note verified.

## Dependencies

None.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `unique-rooms` | `unique_room` | nothing | touch test files |
| `deadline-and-scenarios` | deadline setting, split scenarios | nothing | change any Rust |

## Responsibility

The measurement and its reporting: the script, the step summary, the manual timings job, and the recorded verdict.

## Boundaries

This PR does **not**:

- Change what the e2e leg runs, its filterset, features or `[[test]]` stanzas (the compile split itself is a conditional later node, decided by this node's verdict).
- Change `.config/nextest.toml`, the `docker` group, or start a shared server (nodes 4 and 5).
- Touch the cache keys or `save-if`.

## Draft PR contract

The first push publishes `scripts/ci-e2e-timing.ts` with exported `perBinaryTimings(junit)`, `compileShare(timingsHtml, filterset)` and `verdict(share)` (bodies `// TODO(compile-timings): implement`), plus fixtures and the five failing tests above. `/green` implements them and adds the workflow steps in this PR; it must never merge in the contract state.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — pure functions over fixtures
**Concurrent with:** `unique-rooms`, `deadline-and-scenarios`
**Blocks:** the conditional compile-split node (it consumes this node's verdict, not its code)

Real dependency edges:

    unique-rooms → shared-livekit-ci, parallel-livekit      shared-livekit-ci → parallel-livekit

## Refactoring Needed

(Populated by validation commands.)

## Validation Results

(Populated by validation commands.)

## TODO

- [x] Record initial discovery (`2026-10-04-e2e-leg-compile-timings-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (none: CI infrastructure, no product area)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the script tests (`./dev bun test ./scripts/ci-e2e-timing.test.ts`) and read the CI summary
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes the discovery file
- [ ] USER REVIEW — work complete, decide next steps
