# Changeset: The e2e leg runs against one pinned LiveKit server, and a stuck test is killed and named

**Date**: 2026-10-04
**Status**: 🚧 In Progress
**Type**: Feature (CI infrastructure)

Node 4 of 5 of the `#e2e-leg` stack, PR [#581](https://github.com/uppin/tddy-coder/pull/581) (branch `feature/e2e-leg/shared-livekit-ci`, PR base `feature/e2e-leg/compile-timings`). It consumes node 1's isolation.

**Contract state (commit 2):** `scripts/livekit-ci-server.sh` is a skeleton that prints `TODO(shared-livekit-ci)` and exits 1; all six tests in `scripts/livekit-ci-server.test.ts` fail on it (verified, 6 of 6; run with `./dev bun test ./scripts/livekit-ci-server.test.ts`). The script tests put stub `docker` and `curl` on `PATH`; the readiness wait is bounded by `LIVEKIT_CI_READY_TIMEOUT_SECS`, which the script must honour. The acceptance-test review gate was not held separately (the developer asked for the whole stack to be prepared without stopping).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-04-e2e-leg-shared-livekit-ci-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Affected Packages

- Root CI: `.github/workflows/ci.yml`, `.config/nextest.toml`, `./run-livekit-testkit-server`, new `scripts/livekit-ci-server.sh` (+ test)
- **tddy-livekit-testkit**: [README.md](../../packages/tddy-livekit-testkit/README.md) - the pinned image reference; the env-URL path's failure message
- Docs: [docs/dev/guides/ci.md](../guides/ci.md)

## Related Feature Documentation

No product requirement changes. Source note: [2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md) (step 2).

## Summary

The e2e leg starts one LiveKit server before nextest and removes it after, pointing every test at it with `LIVEKIT_TESTKIT_WS_URL`; the image is pinned, not floating. nextest gets a `slow-timeout` so a test stuck on a dead server is killed and reported by name instead of waiting for the 150-minute job limit. The `docker` test-group stays serial in this node: it proves isolation and lifecycle before node 5 lifts it.

## Background

Today every one of 159 tests starts its own container (~0.5 s each) and CI sets no `LIVEKIT_TESTKIT_WS_URL`, so the testkit's reuse path is unused. The image is the floating `livekit/livekit-server:master`: a different server on every run, pulled every time. A shared server has no per-test port race, which is what lets node 5 drop the serialisation, but a server that outlives the tests is a new way for a job to hang: if it dies mid-run every test blocks on its own awaits. Nothing in the repo bounds that today (`nextest.toml` has no `slow-timeout`).

## Prerequisites

### ⚠ DURING — the source note — [`2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md`](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md)

Delivers step 2 of five. Narrowed at wrap, not deleted. Corrections from discovery that change the design: `./run-livekit-testkit-server` publishes container ports 7880/7881/7882 on **random** host ports and sets no `--config-body`/`UDP_PORT`, whereas `LiveKitTestkit::start()` documents that host port MUST equal container port because LiveKit embeds container ports in ICE candidates. The CI server must copy `start()`'s mapping, not the script's. The testkit's env-URL timeout error already names the URL.

### ⚠ DURING — [`2026-09-16-grpcsessionterminalresume-reconnects-in-tail-under-ci-load.md`](../todo/2026-09-16-grpcsessionterminalresume-reconnects-in-tail-under-ci-load.md) and [`2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md`](../todo/2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md)

Existing flakes under load are the baseline against which node 5's flake count is read; this node records no change to them. Not fixed here.

## Scope

- [x] `scripts/livekit-ci-server.sh start|stop`: `docker run -d --rm` of the **pinned** image with host port = container port for signalling, ICE/TCP and ICE/UDP (as `start()` does), waits for the Twirp API, writes `LIVEKIT_TESTKIT_WS_URL=…` to `$GITHUB_ENV`, prints the image reference and digest to the log; `stop` is idempotent
- [x] Workflow: start step (e2e leg only, `timeout-minutes` short, fails loudly) before nextest and `if: always()` stop step after it
- [ ] Pin the image: one source of truth for the reference used by the testkit and the script (chosen at green by running the suite twice on a release tag/digest; not decided here)
- [x] `.config/nextest.toml`: `slow-timeout` (period, `terminate-after`) on the `docker` override, sized above the longest legitimate test, measured from node 3's per-binary table
- [x] `./run-livekit-testkit-server --stop`, and the same host=container mapping so the local reuse path works for media too
- [x] The kill-the-server drill, runnable from `workflow_dispatch` with an input that stops the server mid-run (`kill_livekit_after_seconds`). Its recorded result is below, once run
- [ ] Tighten the leg's `timeout-minutes` once real runtime is known (separate from the 150-minute shared job default; may need the job split or a per-leg expression)

## Technical Changes

### State A (Current)

- `LiveKitTestkit::start()` reuses a server when `LIVEKIT_TESTKIT_WS_URL` is set (waits ≤ 15 s); CI sets nothing. Image `livekit/livekit-server:master`, three host ports found by `bind(:0)`, mapped host=container.
- `./run-livekit-testkit-server`: named persistent container, fixed container ports on random host ports, no `--stop`, no pin.
- `nextest.toml`: `docker` group (`max-threads = 1`) over ~40 LiveKit binaries; no `slow-timeout`/`global-timeout`.
- nextest 0.9.132 has setup scripts only behind `[experimental] setup-scripts`, and **no teardown hook**.
- `ci.yml` `rust-test` runs both legs through one nextest step; `./dev` execs `nix develop -c`, so `$GITHUB_ENV` reaches the tests.

### State B (Target)

- One server per e2e run, started and removed by workflow steps; all e2e LiveKit tests use it. The unit leg is untouched and starts no container.
- A test hung on a dead server fails the run in minutes with its name.
- A bare `cargo test` with no URL set behaves exactly as before.

### Delta

- `scripts/livekit-ci-server.sh`, `scripts/livekit-ci-server.test.ts`.
- `ci.yml`: start/stop steps, drill input.
- `.config/nextest.toml`: `slow-timeout` on the `docker` override.
- `run-livekit-testkit-server`: `--stop`, host=container mapping.
- `tddy-livekit-testkit`: pinned reference.
- `docs/dev/guides/ci.md`.

## Implementation Milestones

- [x] Script and stubbed-docker tests
- [ ] Image pin chosen and verified on two consecutive runs
- [ ] Workflow steps; leg green with the group still serial
- [x] `slow-timeout` sized from measured timings (60 s × 3; slowest test 39.8 s in run 37222869563)
- [ ] Kill-the-server drill run and recorded
- [x] Local script `--stop` and port mapping

## Testing Plan

### Testing Strategy

The script is tested with a stub `docker` on `PATH` (no daemon needed) for its arguments and outputs. The lifecycle and hang protection are properties of CI, proven by runs: two consecutive green e2e runs against the shared server, and the drill.

### Testing Principles Applied

No production code branches on CI. The testkit keeps both paths; the only runtime change is which image it asks for.

### Coverage Requirements

Script: start publishes host=container ports, pins the image (never `:master`), exports the URL, and stop is safe to run twice.

## Acceptance Tests

### root scripts

- `scripts/livekit-ci-server.test.ts`
  - `start_publishes_each_port_on_the_same_number_inside_and_outside_the_container`
  - `start_uses_a_pinned_image_never_the_floating_master_tag`
  - `start_exports_the_websocket_url_for_the_rest_of_the_job`
  - `start_fails_loudly_when_the_api_never_answers`
  - `stop_removes_the_container_and_is_safe_to_run_twice`
- `.config/nextest.toml` shape: `the_docker_override_kills_a_stuck_test` (parses the file; asserts a `slow-timeout` with `terminate-after` on the override that names the LiveKit binaries)

### CI (proof by run, not a local test)

- Two consecutive green `Rust e2e tests` runs; one run with the binaries shuffled.
- The drill: stop the server mid-run; the leg fails within minutes naming a test.

## Technical Debt & Production Readiness

(Populated during development.)

## Decisions & Trade-offs

- **Workflow steps own the lifecycle**, not a nextest setup script: setup scripts are experimental in 0.9.132 and there is no teardown. The runner is ephemeral, so the stop step is for the log and for reused runners.
- **Host port = container port** for all three ports, as `start()` does, because of ICE candidates; not the local script's random mapping.
- **Pin by digest or release tag, chosen at green** against a suite run, not guessed here.
- **`slow-timeout` only on the LiveKit override**, not the whole leg: the restructuring tests have their own long, legitimate runtimes and have not been sized.
- **Retries** (`count = 2` in the `ci` profile) exist for the port TOCTOU race, which this node removes for the e2e leg. They are left in place here and revisited in node 5, once the leg's flake rate against a shared server is measured.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**; implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `unique-rooms` | `LiveKitTestkit::unique_room`; every test on a unique room; roster tests settled | tests may safely share one server | add or change room names, or the guard test |
| `compile-timings` | per-binary timing table in the step summary | sizes `slow-timeout` from measured test times | add the script or the summary step |
| `deadline-and-scenarios` | 2 s deadline test; split `rpc_scenarios` | shorter longest-test when sizing | touch the deadline setting |

## Responsibility

The server lifecycle in CI (start, readiness, stop), the image pin, `slow-timeout`, the local `--stop`, and the kill-the-server drill.

## Boundaries

This PR does **not**:

- Remove LiveKit binaries from the `docker` group, or change `max-threads` (node 5).
- Change `retries` (node 5).
- Rename rooms or edit tests (node 1).
- Choose or implement the compile split.

## Draft PR contract

The first push publishes `scripts/livekit-ci-server.sh` as a skeleton whose `start` and `stop` print `TODO(shared-livekit-ci)` and exit non-zero, the stub-docker tests above (failing) and the `nextest.toml` shape test (failing). `/green` implements the script, the workflow steps, the pin, the timeout and the drill in this same PR; it must never merge in the contract state.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — a shared server is safe only once every test is on a unique room, which is `unique-rooms`' behaviour, and `slow-timeout` is sized from `compile-timings`' table. The script and its tests can be greened before that; the CI proof cannot.
**Concurrent with:** none (wave 2 has this node alone)
**Blocks:** `parallel-livekit` (needs a shared server and its hang protection)

Real dependency edges:

    unique-rooms → shared-livekit-ci, parallel-livekit      shared-livekit-ci → parallel-livekit

## Refactoring Needed

(Populated by validation commands.)

## Validation Results

(Populated by validation commands.)

## TODO

- [x] Record initial discovery (`2026-10-04-e2e-leg-shared-livekit-ci-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (none: CI infrastructure, no product area)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the script tests and read the CI e2e leg (`scripts/ci-status.sh --watch`)
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes the discovery file
- [ ] USER REVIEW — work complete, decide next steps
