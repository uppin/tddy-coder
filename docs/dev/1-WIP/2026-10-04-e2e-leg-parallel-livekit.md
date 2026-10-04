# Changeset: The LiveKit e2e binaries run in parallel

**Date**: 2026-10-04
**Status**: 🚧 In Progress
**Type**: Feature (CI infrastructure)

Node 5 of 5 of the `#e2e-leg` stack (branch `feature/e2e-leg/parallel-livekit`, PR base `feature/e2e-leg/shared-livekit-ci`). The last node: it consumes nodes 1 and 4.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-04-e2e-leg-parallel-livekit-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Affected Packages

- Root CI: `.config/nextest.toml`, [docs/dev/guides/ci.md](../guides/ci.md)
- No Rust package changes expected; any test found to depend on running alone is fixed in its own package.

## Related Feature Documentation

No product requirement changes. Source note: [2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md) (step 3 of its order, "lift the `docker` group").

## Summary

Remove the LiveKit binaries from the `docker` test-group so nextest runs them concurrently against the shared server. With one server, unique rooms and a hang guard in place, the only reason for the serialisation (a port-allocation race between per-test containers) no longer exists, and the leg's run phase stops being 1.0× parallel.

## Background

The leg's tests ran one at a time (482 s of summed test time in a 472 s run) because each test started its own container on ports found by `bind(:0)` and release. The `docker` group was the mitigation, with retries as the backstop. After node 4 there is one server and no per-test port, after node 1 no two tests share a room, and after node 4 a dead server is bounded by `slow-timeout`. The runner has four vCPUs and each test starts several in-process daemons, so CPU, not the server, is expected to be the limit; the speed-up is bounded accordingly and **not predicted** here (the note's 1.5×–3× is a guess).

## Prerequisites

### ⚠ DURING — the source note — [`2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md`](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md)

Delivers step 3 of five and is the last node of the stack. At wrap: delete the note **only if** nothing in it is still open. Still open after this stack: the conditional compile split (§ 3a), decided by node 3's measured verdict; the `rust-analyzer` group's 10-of-31 drift (§ 4, whose own note says to act after this node, when the rest of the leg is fast enough that it is the long pole); and the deliberate 12 s window in `common_room_set_metadata_handshake_repro`. Narrow the note to those; do not delete it.

### ⚠ DURING — flaky tests — [`2026-09-16-grpcsessionterminalresume-reconnects-in-tail-under-ci-load.md`](../todo/2026-09-16-grpcsessionterminalresume-reconnects-in-tail-under-ci-load.md), [`2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md`](../todo/2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md), [`2026-09-24-worktree-size-reload-test-races-the-persist-it-reads.md`](../todo/2026-09-24-worktree-size-reload-test-races-the-persist-it-reads.md)

None of these is a LiveKit test, but they are the known concurrency-sensitive suite and set the baseline against which any new flake is read. Recorded, not fixed here.

## Scope

- [ ] Derive the exact set of binaries that start the LiveKit testkit (the `docker` override's membership was **not** re-derived in discovery) and remove each from the `docker` group; keep the group only for what genuinely needs serialising, if anything
- [ ] Measure before and after on the same JUnit script (node 3): run time, per-binary time, flake count over several runs
- [ ] Decide on `retries` for the e2e leg with the measured flake rate: keep, lower, or remove (retries hide flakes as "flaky", and their reason — the port race — is gone)
- [ ] Fix, in its own package, any test shown to depend on running alone
- [ ] Update the CI guide (the "docker group" and "Flaky tests" sections)

## Technical Changes

### State A (Current)

- `.config/nextest.toml` `[test-groups] docker = { max-threads = 1 }`; the `profile.ci` override lists ~40 LiveKit binaries by package and name; `retries = { count = 2, backoff = exponential }`.
- The unit leg has no container; the e2e leg (after node 4) has one shared server and `slow-timeout` on the group.
- Per-test timing: median 1.1 s, mean 2.2 s, p90 4.7 s; 66 tests of 2–5 s hold 51% of test time.

### State B (Target)

- LiveKit binaries run with nextest's default parallelism (the runner's vCPUs), no `docker` group membership; the e2e leg's run phase is measurably shorter and flake-free over the measured runs.

### Delta

- `.config/nextest.toml`: the override/group changes; retries decision.
- `docs/dev/guides/ci.md`.

## Implementation Milestones

- [ ] Set of LiveKit-testkit binaries derived and compared with the group
- [ ] Group lifted for them; run green
- [ ] Before/after numbers recorded in this changeset
- [ ] Several runs for the flake count; any dependent test fixed or isolated
- [ ] `retries` decision recorded

## Testing Plan

### Testing Strategy

The behaviour is a property of CI runs: run time and flake rate before and after, read with node 3's per-binary table. A local check is a drift test (below) so the config cannot silently diverge from the testkit users again.

### Testing Principles Applied

The drift test derives the set from the source (test files that start the testkit) and compares it with the nextest config, so it fails with the names of binaries that disagree instead of relying on a list a human maintains.

### Coverage Requirements

No test is removed or skipped to gain speed.

## Acceptance Tests

### root / config

- `scripts/nextest-docker-group.test.ts`
  - `no_binary_that_starts_the_livekit_testkit_is_in_the_serial_docker_group`
  - `every_binary_in_the_serial_docker_group_still_needs_to_be_serial` (the group, if kept, names only what is justified in a comment)

### CI (proof by run, not a local test)

- Run time of `Rust e2e tests` (run phase) below the 472 s baseline, with the number stated.
- Flake count over several consecutive runs; `retries` decision justified by it.

## Technical Debt & Production Readiness

(Populated during development.)

## Decisions & Trade-offs

- **Two steps, not one**: node 4 proves isolation and lifecycle with the group serial; this node lifts it. If it flakes, the revert is one config change.
- **Speed-up not promised**: bounded by CPU on four vCPUs; the number is whatever the measurement says.
- **`rust-analyzer` group untouched**: it names 10 of 31 binaries; serialising all 31 would cost ~8–10 minutes and is for after this node, when it is the long pole.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `unique-rooms` | unique rooms, `unique_room`, settled roster tests | removes the reason tests could not run beside each other | add or change room names |
| `shared-livekit-ci` | one pinned shared server, `slow-timeout`, drill | tests run concurrently against it; a stuck test is killed and named | start or stop the server, set `slow-timeout`, or run the drill |
| `compile-timings` | per-binary timing table | before/after comparison | change the script |
| `deadline-and-scenarios` | 2 s deadline test, split `rpc_scenarios` | the split scenarios are what actually run in parallel | touch either |

## Responsibility

The `docker` group's membership and `retries`, and the before/after measurement that justifies them.

## Boundaries

This PR does **not**:

- Change workflow steps, the pinned image or `slow-timeout` (node 4).
- Change any room name (node 1).
- Touch the `rust-analyzer` group.
- Decide or implement the compile split.

## Draft PR contract

The first push publishes `scripts/nextest-docker-group.test.ts` and the small helper it needs (`binariesStartingTheTestkit(repoRoot)`, `dockerGroupBinaries(nextestToml)`, bodies `// TODO(parallel-livekit): implement`) with the two tests above failing. `/green` lifts the group, records the numbers and decides `retries` in this same PR; it must never merge in the contract state.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — its proof is a CI run of the e2e leg against the shared server (node 4's behaviour) with unique rooms (node 1's behaviour). The drift test and helper alone can be greened earlier.
**Concurrent with:** none (wave 3 has this node alone)
**Blocks:** nothing in the stack; the `rust-analyzer` group follow-up and the conditional compile split come after it

Real dependency edges:

    unique-rooms → shared-livekit-ci, parallel-livekit      shared-livekit-ci → parallel-livekit

## Refactoring Needed

(Populated by validation commands.)

## Validation Results

(Populated by validation commands.)

## TODO

- [x] Record initial discovery (`2026-10-04-e2e-leg-parallel-livekit-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (none: CI infrastructure, no product area)
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Read the CI e2e leg over several runs (`scripts/ci-status.sh --watch`, `--failures`)
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes the discovery file and narrows the source note
- [ ] USER REVIEW — work complete, decide next steps
