# Changeset: The LiveKit e2e binaries run in parallel

**Date**: 2026-10-04
**Status**: 🚧 In Progress
**Type**: Feature (CI infrastructure)

Node 5 of 5 of the `#e2e-leg` stack, PR [#582](https://github.com/uppin/tddy-coder/pull/582) (branch `feature/e2e-leg/parallel-livekit`, PR base `master` — node 4, #581, has merged). The last node: it consumes nodes 1 and 4.

**Contract state (commit 2):** `scripts/nextest-docker-group.ts` published `binariesStartingTheTestkit`, `dockerGroup` and `isSerialised` with `throw` bodies; both tests in `scripts/nextest-docker-group.test.ts` failed on them (verified, 2 of 2). Since validation the files are `scripts/nextest-serial-groups.ts` / `.test.ts` and `dockerGroup` is `ciSerialisation` (see Validation Results); run with `./dev bun test ./scripts/nextest-serial-groups.test.ts`. The second test (names in the group must exist in the package they name) is a drift check and may pass once implemented even before the group is lifted. The acceptance-test review gate was not held separately (the developer asked for the whole stack to be prepared without stopping).

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

- [x] Derive the exact set of binaries that start the LiveKit testkit (the `docker` override's membership was **not** re-derived in discovery) and remove each from the `docker` group; keep the group only for what genuinely needs serialising, if anything — 35 binaries derived by `binariesStartingTheTestkit`; nothing else used the group, so `docker = { max-threads = 1 }` is removed
- [ ] Measure before and after on the same JUnit script (node 3): run time, per-binary time, flake count over several runs
- [ ] Decide on `retries` for the e2e leg with the measured flake rate: keep, lower, or remove (retries hide flakes as "flaky", and their reason — the port race — is gone)
- [ ] Fix, in its own package, any test shown to depend on running alone
- [x] Update the CI guide (the "docker group" and "Flaky tests" sections)

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

- [x] Set of LiveKit-testkit binaries derived and compared with the group
- [ ] Group lifted for them; run green — lifted in config; CI run pending
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

- `scripts/nextest-serial-groups.test.ts` (a script test, not run by CI: `./dev bun test ./scripts/nextest-serial-groups.test.ts`)
  - `no_binary_that_starts_the_livekit_testkit_is_in_a_serial_test_group` — any `[test-groups]` entry with `max-threads = 1`, not only one named `docker`
  - `every_binary_a_serial_group_names_exists_in_the_package_it_names` — every `package()` / `binary()` a serial group's filter names exists among the workspace's test binaries (qualified: in that package; unqualified: in any package; wholesale package: is a member)
  - plus focused unit tests on inline TOML fixtures and temporary workspaces (filter evaluation, rejected filter syntax, test-target discovery)

### CI (proof by run, not a local test)

- Run time of `Rust e2e tests` (run phase) below the 472 s baseline, with the number stated.
- Flake count over several consecutive runs; `retries` decision justified by it.

## Technical Debt & Production Readiness

- **Derivation (done).** `binariesStartingTheTestkit` finds 35 binaries whose source calls `LiveKitTestkit::start` (the testkit's only constructor; scanned in each auto-discovered test target and the `tests/<mod>/` modules it declares). Every one of them was serialised by the old group (wholesale `package(tddy-livekit)` / `package(tddy-livekit-testkit)`, qualified pairs, and the unqualified `binary(...)` tail), so the group missed nothing. Seven of its qualified `package(tddy-daemon) and binary(...)` pairs named binaries that no longer live in `tddy-daemon` (`common_room_duplicate_identity_repro`, `common_room_set_metadata_handshake_repro`, `forward_to_peer_shared_registry`, `livekit_peer_daemons_acceptance`, `session_room_livekit_acceptance` moved to `tddy-daemon-livekit`; `remote_git_livekit_acceptance` to `tddy-worktree-service`; `session_room_acceptance` to `tddy-session-lifecycle`); the unqualified tail is what still matched them.
- **Stale pairs remain in the override's filter.** The filter now scopes only node 4's `slow-timeout`, which this PR does not own, so the seven dead qualified pairs were left in place. Pruning them (or rewriting the filter as `package(...) and binary(...)` with the right owners) is a follow-up; the unqualified entries keep coverage correct meanwhile.
- **The name-existence test is no longer vacuous.** It now checks every serial group (today `rust-analyzer`, whose 10 names all exist) against all test binaries, not only the testkit users. Reintroducing the old `docker` override would make it report the seven dead `tddy-daemon` pairs below (checked by mutation).
- **Node 4's hang-guard test re-pointed here.** `scripts/livekit-ci-server.test.ts` found the LiveKit override by `test-group = "docker"`, which this node removes. With the developer's consent it now finds the override by `package(tddy-livekit-testkit)` in its filter and is renamed `the_livekit_override_kills_a_stuck_test`; the assertion (the override carries `slow-timeout` with `terminate-after`) is unchanged.
- **CI measurement pending.** Before/after run time of the `Rust e2e tests` run phase (baseline 472 s) and per-binary times from node 3's JUnit table, over several consecutive runs, with the flake count.
- **`retries` decision pending.** Left at `count = 2` with `TODO(parallel-livekit): decide retries from measured flake rate` in `.config/nextest.toml`; its original reason (the per-container port race) is gone on CI. Keep, lower or remove once the flake count above is in.
- **Shared filterset module (done).** `scripts/nextest-filterset.ts` is the one tokenizer/parser for both `scripts/ci-e2e-timing.ts` (node 3, now on `master`) and the drift check; `ci-e2e-timing.test.ts` is unchanged and green. One behaviour change for node 3's script: a filterset with glob syntax, `~` / `/regex/` / `#` matchers or an unrecognised character now throws instead of silently matching nothing.

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

The first push published `scripts/nextest-docker-group.test.ts` and the small helper it needed (`binariesStartingTheTestkit(repoRoot)`, `dockerGroup(nextestToml)`, `isSerialised(group, binary)`, `throw` bodies) with the two tests above failing; after validation these are `scripts/nextest-serial-groups.ts`'s `binariesStartingTheTestkit(repoRoot)`, `testBinaries(repoRoot)`, `ciSerialisation(nextestToml)`, `isSerialised(config, binary)`, `serialGroupReferences(config)` and `referencesMatchingNothing(references, workspace)`. `/green` lifts the group, records the numbers and decides `retries` in this same PR; it must never merge in the contract state.

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

### 2026-10-05 — drift check review (refactor pass)

Findings on the drift check and the fixes applied (files: `scripts/nextest-filterset.ts` (new), `scripts/nextest-serial-groups.ts` / `.test.ts` (renamed from `nextest-docker-group.*`), `scripts/ci-e2e-timing.ts`, `.config/nextest.toml` comment, `docs/dev/guides/ci.md`):

| Finding | Fix |
|---|---|
| Two filterset parsers (here and node 3's `ci-e2e-timing.ts`) | Extracted `scripts/nextest-filterset.ts` (`parseFilterset`, `matches`, `filterPredicate`, `filterReferences`); both scripts use it. `isSerialised` evaluates the parsed filter, so `not` is honoured; the bespoke `DockerGroup` / `addTerm` / `disjuncts` / `predicateArg` code is gone |
| Tokenizer silently dropped unknown characters (`package(a) and !binary(foo)` lost the `!`) | A real tokenizer whose catch-all throws `unrecognised filterset token: … at offset N`; glob syntax (`{a,b}`, `*`, …), `~` / `/` / `#` matchers and predicates other than `binary`, `package`, `kind(test)` are rejected by name |
| A `ci` profile with `inherits`, or a `platform`-scoped test-group override, would be misread | Both throw |
| Check only looked at a group literally named `docker` | Any `[test-groups]` entry with `max-threads = 1`; the first override that selects a binary and sets `test-group` decides its group (nextest's order: `ci`'s overrides, then `default`'s) |
| Second test misnamed and vacuous (compared the group's names against testkit users only) | `every_binary_a_serial_group_names_exists_in_the_package_it_names`, against all test binaries; covers qualified, unqualified and wholesale-package names |
| Discovery walked `packages/*`, ignored `[[test]]` / `autotests`, resolved `mod x;` to `tests/x/` only | Members from the root `Cargo.toml` (globs and `exclude` honoured); `[[test]]` / `autotests` throw; `mod x;` resolves to `x.rs` or `x/mod.rs` (transitively, `#[path]` honoured), and a declaration that resolves to neither, or to both, throws. Comments and string literals are blanked before scanning — a `mod` inside a fixture string in `tddy-code-restructuring/tests/cluster_move.rs` otherwise fails discovery |
| Only two live-repo tests | Kept both (they are the drift check); added 20 unit tests on inline TOML fixtures and `mkdtemp` workspaces |
| Nits: parser helper named `binary`; `name as string` cast | `leftAssociative`; a type guard (`isPredicateName`) |
| ci.md's retries sentence pointed at this changeset, which is deleted at wrap | Now "pending measurement on CI" |

Verification (scoped): `./dev bun test ./scripts/nextest-serial-groups.test.ts ./scripts/ci-e2e-timing.test.ts ./scripts/livekit-ci-server.test.ts` — 39 pass, 0 fail. Mutation check: re-adding a `docker = { max-threads = 1 }` group with the LiveKit override in it makes the first test report all 35 testkit binaries and the second the seven dead `tddy-daemon` pairs.

Still open: the script tests are not run by any CI job (wiring them in touches `.github/workflows/`, pending the developer's decision).

## TODO

- [x] Record initial discovery (`2026-10-04-e2e-leg-parallel-livekit-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (none: CI infrastructure, no product area)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (drift helper; config lift; CI numbers and `retries` still open)
- [ ] Update documentation with progress
- [ ] Read the CI e2e leg over several runs (`scripts/ci-status.sh --watch`, `--failures`)
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes the discovery file and narrows the source note
- [ ] USER REVIEW — work complete, decide next steps
