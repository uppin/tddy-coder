# Changeset: Every LiveKit e2e test names a room of its own

**Date**: 2026-10-04
**Status**: 🚧 In Progress — code complete; wrap blocked on the CI e2e runs
**Type**: Refactor (test infrastructure)

Node 1 of 5 of the `#e2e-leg` stack, PR [#578](https://github.com/uppin/tddy-coder/pull/578) (branch `feature/e2e-leg/unique-rooms`, base `master`).

**Contract state (commit 2):** `unique_room` is published with a `todo!()` body; the four helper tests fail on it, and the guard test fails listing the **42** fixed room names currently in the tree. The acceptance-test review gate was not held separately for this node (the developer asked for the whole stack to be prepared without stopping); review the test list above before `/green`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-04-e2e-leg-unique-rooms-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Affected Packages

- **tddy-livekit-testkit**: [README.md](../../packages/tddy-livekit-testkit/README.md) - `LiveKitTestkit::unique_room`
- **tddy-livekit**: [README.md](../../packages/tddy-livekit/README.md) - test files migrated; `room_roster` read tolerates a room that closes under it (conditional, see Scope)
- **tddy-daemon-livekit**, **tddy-daemon**, **tddy-daemon-rpc**, **tddy-e2e**, **tddy-session-lifecycle**: test files only
- Docs: [docs/dev/guides/ci.md](../guides/ci.md) and [testing.md](../guides/testing.md) gain the isolation rule

## Related Feature Documentation

No product requirement changes; this is test infrastructure. Source note:
[2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md) (step 1, the isolation precondition).

## Summary

Add `LiveKitTestkit::unique_room(prefix)` and make every test that starts a LiveKit server build its room name from it, so that two tests can never share a room whether they run in one process, in two processes, or against one shared server. Nothing about how the tests are scheduled changes: the `docker` test-group stays serial.

## Background

Today every test starts its own container, so a fixed room name is private to that container. Two later nodes of this stack remove that privacy: one shared server for the job (node 4), and the `docker` group lifted so the LiveKit tests run in parallel (node 5). With either, two tests on the same room name or the same identity in the same room interfere. `#[serial]` does not help (under nextest each test is its own process), so isolation has to be in the names. It is a precondition and must land first, on its own, with the serial group still protecting it.

## Prerequisites

### ⚠ DURING — the source note — [`2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md`](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md)

This node delivers step 1 of five. The note is narrowed at wrap, never deleted by this node (a partly-fixed record is narrowed): the last node of the stack removes what it has closed. Two of its claims were corrected by discovery and the correction belongs in the narrowed note: `"tddy-lobby"` is not a shared-server collision (the three binaries it names never start the testkit), and of the five "server-global" tests it lists only `room_roster_livekit` and `session_room_acceptance` touch a real server.

### ⚠ DURING — code issues in `tddy-daemon-livekit`

[`oversized-file-livekit-peer-discovery`](../../packages/tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md) is unrelated to the test files edited here; recorded, not touched.

## Scope

- [x] `LiveKitTestkit::unique_room(prefix) -> String`, with no new dependency
- [x] Migrate every fixed room constant shared by more than one test, and the one literal shared across binaries, to it (list in Delta)
- [x] Replace the three ad-hoc nonce helpers (`a_room_for`, `a_room_named`, inline `format!("…-{}", uuid)`) and the two `COMMON_ROOM_PREFIX` users with the helper where that does not change behaviour
- [x] A guard test that fails when a test file that starts the testkit names a room with a fixed literal
- [x] Settle the two tests that read the whole room roster against a server that other tests are using (`room_roster_livekit`, `session_room_acceptance`)
- [x] Verify against the real server what `ListParticipants` does for a room that closed between `ListRooms` and `ListParticipants`; if it errors, make `LiveKitRoomRoster::read_roster` skip a room that is gone instead of failing the whole read — it does not error: a nonexistent room returns `Ok([])`, so no production change
- [x] Document the rule (a test's room comes from `unique_room`) in the testing guide

## Technical Changes

### State A (Current)

- `LiveKitTestkit` (`packages/tddy-livekit-testkit/src/livekit_testkit.rs`) exposes `start`, `get_ws_url`, `remove_participant`, `generate_token`. No room-name helper.
- 34 test files start the testkit. About ten already build unique names (nanos in `broadcast_and_room_metadata`/`room_roster_livekit`; `uuid` inline in six daemon files; `COMMON_ROOM_PREFIX` in two).
- Fixed names shared by several tests of one binary: `agent-roster-common-room` (22 tests, `session_agent_remote_acceptance`), `attach-cross-host-room` (8), `acceptance-common-room` (6, `multi_host_acceptance`; 3 more in `livekit_peer_daemons_acceptance`, **the one cross-binary collision**), `session-sync-e2e-lobby` (6), `relay-e2e-common-room` (3), `attach-start-forwarding-room` (2), `token-service-test` (2), `token-generation-test` (2), `acceptance-owned-project-count` (3), plus single-test names that are safe now but are fixed.
- `rpc_scenarios` uses ten distinct per-block rooms inside one test; unique within the binary.
- `LiveKitRoomRoster::read_roster` lists rooms, then lists each room's participants one by one under a 5 s whole-read timeout; any per-room failure fails the whole read.
- `#[serial]` is a no-op under nextest; the `docker` group (`max-threads = 1`) is the only serialisation.

### State B (Target)

- `LiveKitTestkit::unique_room(prefix)` returns `<prefix>-<hex nanos>-<pid>-<counter>`: unique across threads, processes and runs without a dependency.
- No test that starts the testkit names a room with a fixed literal; a guard test enforces it.
- Roster-reading tests find their own room by its unique name and tolerate other rooms coming and going.
- Tokens stay minted per room; the `devkey`/`secret` pair stays shared.

### Delta

#### tddy-livekit-testkit

- `unique_room` (and unit tests for it). The `uuid` crate is **not** added: CLAUDE.md requires asking before new dependencies, and nanos + pid + an atomic counter is enough for uniqueness here.

#### tddy-livekit, tddy-daemon-livekit, tddy-daemon, tddy-daemon-rpc, tddy-e2e, tddy-session-lifecycle, tddy-coder

- Migrate the files named in State A; each binary keeps the room's *purpose* as the prefix so a leaked room is attributable.
- `room_roster_livekit` and `session_room_acceptance`: assert on their own room only; both already find it by name.

#### tddy-livekit (production, conditional)

- `room_roster.rs`: if verification shows `ListParticipants` fails for a vanished room, `read_roster` skips that room. This is correct behaviour (a room that closed is not in the roster), not a fallback; it is made only if the failure is demonstrated.

## Implementation Milestones

- [x] `unique_room` and its tests
- [x] Guard test written (red), then migrated until green
- [x] Every shared constant migrated; `acceptance-common-room` gone
- [x] Roster race verified against a real server and recorded (fixed or shown not to exist)
- [x] Testing guide updated
- [ ] Two consecutive green runs of the e2e leg, and one with the binaries shuffled — **open: runs on CI only; the Docker suites were not run locally**

## Testing Plan

### Testing Strategy

Unit tests for the helper; a text-level guard test over the test tree (a deliberate lint, in the testkit's own tests); and the existing LiveKit suites as the behavioural proof, run in CI. The shuffled run is what proves independence from order.

### Testing Principles Applied

Fluent-tests style; the guard test names the offending file and constant in its failure message so a developer who adds a fixed room is told what to do.

### Coverage Requirements

Every migrated binary keeps its existing assertions; no test is weakened.

## Acceptance Tests

### tddy-livekit-testkit

- `packages/tddy-livekit-testkit/tests/unique_room.rs`
  - `a_unique_room_keeps_the_prefix_it_was_given`
  - `two_unique_rooms_from_one_prefix_differ`
  - `unique_rooms_made_on_many_threads_never_repeat`
  - `a_unique_room_is_a_valid_livekit_room_name` (letters, digits, `-` only)
- `packages/tddy-livekit-testkit/tests/livekit_tests_use_unique_rooms.rs`
  - `no_test_that_starts_the_livekit_testkit_names_its_room_with_a_fixed_literal` (walks every package's `tests/`; allowlists token-only files that never connect, e.g. the testkit's own integration test)

### tddy-livekit

- `packages/tddy-livekit/tests/room_roster_livekit.rs`
  - ~~`a_roster_read_reports_a_room_that_is_open_beside_rooms_that_close`~~ not written: the race is not real (nonexistent room returns `Ok([])`)

## Technical Debt & Production Readiness

None this PR added. Open and not this PR's: [`oversized-file-livekit-peer-discovery`](../../packages/tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md) (unrelated test-file edits only). Duplication left by choice, see Refactoring Needed.

## Decisions & Trade-offs

- **No new dependency.** `uuid` would be the obvious source of randomness; asking was not necessary because nanos, pid and a counter are unique enough for a test run and keep the testkit's dependency set unchanged. Revisit only if a collision is ever observed.
- **Guard by text scan.** A scan of the test tree is the only way to catch a *new* fixed room; its cost is that it reads source. It lists its allowlist explicitly and fails with the file and constant.
- **Identities stay as they are.** They are room-scoped, so unique rooms make them safe; renaming them would be churn with no isolation gain.

- **`rpc_scenarios.rs` room literals were edited, against Boundaries.** Boundaries reserve `rpc_scenarios` for node 2, but the guard test (which this node must make green) flags its fixed per-block room names, so the names had to change. Only room-name expressions were touched: no deadline, no scenario structure. Node 2 (#579) will touch the same file and must rebase over these lines.
- **Eleven heavy files use a per-file `OnceLock` `room()` helper** (unique per process, not per test): their tests share one room on purpose (participants of one fixture meet in it), and per-test rooms would need threading a name through every fixture. Per-process uniqueness is what a shared server needs; per-test isolation inside one binary is node 5's concern when the `docker` group is lifted, and is recorded there, not here.
- **`room_roster.rs` unchanged.** Verified against a real LiveKit server: `ListParticipants` for a nonexistent room returns `Ok([])`, so a room that closes between `ListRooms` and `ListParticipants` does not fail the read. The conditional production change and its test are therefore not needed.
- **Two identity consts renamed** in `first_login_enrolment_acceptance.rs` (`THE_DESKTOPS_ROOM_IDENTITY` to `THE_DESKTOP_LOBBY_IDENTITY`, `A_ROOM_PEERS_IDENTITY` to `A_LOBBY_PEERS_IDENTITY`): the guard matches any `ROOM` const holding a string literal, and these are identities, not rooms. Values unchanged.

## Dependencies

None. This is a root node of the stack.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| (none) | - | - | - |

It is what later nodes consume: node 4 (`shared-livekit-ci`) and node 5 (`parallel-livekit`) need every test on a unique room before one server, or parallelism, is safe.

## Responsibility

- The `unique_room` helper and its tests.
- Every test room name in the workspace being unique and generated, enforced by a guard test.
- Settling the roster-reading tests against a busy server, and the verification of the `ListParticipants` race.

## Boundaries

This PR does **not**:

- Change `.config/nextest.toml` or any CI workflow: the `docker` group stays serial, no server is shared yet (nodes 4 and 5).
- Pin the LiveKit image or change `start()`'s container launch.
- Touch the deadline setting or `rpc_scenarios` (node 2).
- Rename identities or change what any test asserts.

## Draft PR contract

The first push of this PR publishes:

- `LiveKitTestkit::unique_room(prefix: &str) -> String`, body `// TODO(unique-rooms): implement`.
- The failing tests above: the four helper tests fail on the unimplemented body; the guard test fails listing every fixed room still in the tree.

`/green` then implements the helper and migrates the tests in this same PR. This PR must never merge in the contract state.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — it needs only its own surface
**Concurrent with:** `deadline-and-scenarios`, `compile-timings`
**Blocks:** `shared-livekit-ci` (needs unique rooms before a server can be shared), `parallel-livekit`

Real dependency edges, as opposed to the branch line:

    unique-rooms → shared-livekit-ci, parallel-livekit      shared-livekit-ci → parallel-livekit
    deadline-and-scenarios, compile-timings: no predecessor behaviour needed

## Refactoring Needed

- The `OnceLock` room helper is repeated in eleven test files (about four lines each). A shared `LiveKitTestkit` helper would remove it but adds testkit API for a pattern node 5 may replace with per-test rooms; deferred, not a defect.
- `a_lobby_of_its_own` in `session_room_acceptance.rs` lost nothing: its doc comment already says why.

## Validation Results

Scope: scoped to the touched packages; nothing workspace-wide was run.

| Gate | Result |
|---|---|
| Stack rebase / leak check | `git log origin/master..HEAD` lists only this PR's three commits |
| `cargo fmt --check` | clean |
| `cargo test -p tddy-livekit-testkit --test unique_room --test livekit_tests_use_unique_rooms` | 4 passed + guard 1 passed |
| `cargo clippy -p tddy-livekit-testkit --all-targets -- -D warnings` | clean |
| `cargo check --all-targets -p tddy-livekit -p tddy-daemon-livekit -p tddy-daemon -p tddy-daemon-rpc -p tddy-e2e -p tddy-session-lifecycle -p tddy-coder` | clean, 0 warnings |
| Docker-based LiveKit suites | **not run locally** |
| Two green e2e CI runs, one shuffled | **pending on CI** |

- **validate-changes**: every changed path maps to this changeset; no deletions; no parent-owned files (root node). Deviation: `rpc_scenarios.rs` edited (see Decisions).
- **validate-tests**: no assertion weakened; migrated tests keep their assertions. Guard test names file, line and fix. Fixture rooms stay stable within a process, so tests that need participants to meet still do.
- **validate-prod-ready**: only production change is `unique_room`; no `TODO`, `FIXME`, mock or debug output added by this PR (the existing `eprintln!`/`FIXME(flaky)` in touched test files predate it).
- **File length (step 3.5)**: `livekit_testkit.rs` 220 to 237 production lines; no file at or over 500.
- **Clean code**: 9/10. Deduction: the repeated `OnceLock` helper (see Refactoring Needed).
- **Code issues (step 7.5)**: no record in the touched packages is claimed by #578 or measures code this PR changed; none reconciled.

## TODO

- [x] Record initial discovery (`2026-10-04-e2e-leg-unique-rooms-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (none: test infrastructure, no product area; see the stack's PRD for node 2)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Run scoped tests (done, see Validation Results) and read the e2e leg on CI — **CI part open**
- [x] Validate changes (/validate-changes)
- [x] Validate tests (/validate-tests)
- [x] Validate production readiness (/validate-prod-ready)
- [x] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes the discovery file
- [ ] USER REVIEW — work complete, decide next steps
