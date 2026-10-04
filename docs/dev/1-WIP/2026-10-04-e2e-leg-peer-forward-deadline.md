# Changeset: The peer-forward deadline is a daemon setting; `rpc_scenarios` is one test per scenario

**Date**: 2026-10-04
**Status**: 🚧 In Progress
**Type**: Feature (daemon setting) + Refactor (test split)

Node 2 of 5 of the `#e2e-leg` stack (branch `feature/e2e-leg/deadline-and-scenarios`, PR base `feature/e2e-leg/unique-rooms`). It consumes nothing from node 1.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-04-e2e-leg-peer-forward-deadline-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Affected Packages

- **tddy-daemon-kernel**: [README.md](../../packages/tddy-daemon-kernel/README.md) - `peer_forwarding` (handle type, deadline), `config.rs` (key, accessor); [daemon-kernel.md](../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) config fields
- **tddy-daemon-livekit**: [README.md](../../packages/tddy-daemon-livekit/README.md) - forwarding call sites in `livekit_peer_discovery.rs`, `peer_routing.rs`
- **tddy-host-service**, **tddy-session-lifecycle**, **tddy-session-agents**, **tddy-daemon-rpc**: call-site type change, `split_forward_deadline`
- **tddy-livekit**, **tddy-daemon**: test changes (`rpc_scenarios`, the deadline test)
- Root: `daemon.yaml.production` (commented example)

## Related Feature Documentation

- [PRD: peer-forward deadline setting](../ft/daemon/1-WIP/PRD-2026-10-04-peer-forward-deadline-setting.md)
- [LiveKit peer discovery](../ft/daemon/livekit-peer-discovery.md), [Remote managed worktree](../ft/daemon/remote-managed-worktree.md), [Session agent roster](../ft/daemon/session-agent-roster.md)

## Summary

`PEER_FORWARD_TIMEOUT` (30 s) becomes the default of a new `DaemonConfig::peer_forward_timeout_secs`. The deadline travels with a small common-room handle that every forwarding call already receives in place of the bare room slot. The silent-peer deadline test sets 2 s, and `rpc_scenarios` is split into one test per scenario.

## Background

The silent-peer deadline test waits out the full 30 s on purpose (a peer that is present but silent is exactly the case the deadline exists for), and there is no clock to pause: it is real time over a real network. A real, defaulted, operator-facing setting makes it testable in seconds without a branch that only tests take. `rpc_scenarios` is a single 30 s `#[serial]` test running ten scenarios in sequence; split, a failure names its scenario and the scenarios can run concurrently once the LiveKit tests are no longer serialised (node 5).

## Prerequisites

### ⚠ DURING — the source note — [`2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md`](../todo/2026-10-04-rust-e2e-leg-shared-livekit-and-deadline-waits.md)

This node delivers step 4 (§ 2) of five. Narrowed at wrap, not deleted. Corrections from discovery: `rpc_scenarios`' inner timeouts are 3/5/10/10 s plus one `sleep(2s)` (not "10-20 s"); the fixed-deadline call sites are about 24 and most hold only the room slot, which is why the deadline rides on the handle (decided with the developer) rather than being a per-call argument or a global.

### ⚠ DURING — [`oversized-file-config.md`](../../packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md)

`config.rs` is over the size guideline; this node adds a field, a default and an accessor only, and updates that record's measurement table. Recorded, not fixed here.

### ⚠ DURING — [`heavy-dependency-livekit-peer-forwarding.md`](../../packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md) and [`oversized-file-livekit-peer-discovery.md`](../../packages/tddy-daemon-livekit/docs/code-issues/oversized-file-livekit-peer-discovery.md)

Both name files this node edits. Both are **unclaimed**, so no wait-or-proceed fork applies. Edits are limited to the handle type at the seam; `peer_forwarding` stays the only SDK consumer in the crate. Recorded, not fixed here.

## Scope

- [ ] `peer_forward_timeout_secs` on `DaemonConfig` (default 30, accessor clamps to 1 s), commented example in `daemon.yaml.production`
- [ ] A common-room handle type holding the room slot and the deadline, built from `DaemonConfig`
- [ ] `forward_to_peer`, `forward_to_peer_within` and `forward_server_stream_to_peer` take the deadline from the handle; the **open** deadline of a stream follows the setting; the **idle** timeout stays fixed
- [ ] All ~24 call sites moved onto the handle; `split_forward_deadline` and `remote_managed_worktree_acceptance` follow the setting
- [ ] The deadline test sets 2 s, keeps asserting `DeadlineExceeded`, and its comments about the "30 s deadline" are corrected
- [ ] `rpc_scenarios` split into one test per scenario, each with its own `unique`-style room (node 1's helper is not available here; each test builds its room from its scenario name, which is already distinct)
- [ ] Docs: feature docs, `daemon-kernel.md`, `oversized-file-config` record

## Technical Changes

### State A (Current)

- `peer_forwarding.rs`: `PEER_FORWARD_TIMEOUT = 30s` (:93) and `PEER_FORWARD_STREAM_IDLE_TIMEOUT = 30s` (:106); `forward_to_peer` wraps `forward_to_peer_within(…, deadline)`; `forward_server_stream_to_peer` uses the constants directly (open :220/:224, idle :246/:256).
- All forwarders take `room_slot: &Arc<RwLock<Option<Arc<Room>>>>`. Only `PeerRouting`, `HostServiceImpl` and the lifecycle `ConnectionService` family also hold a `DaemonConfig`; the `tddy-session-agents` forwarders and `tddy-daemon-rpc` exec-tool ports hold only the slot.
- `DaemonConfig` precedents: `spawn_worker_request_timeout_secs` (default 300, `.max(1)` accessor, tests in `mod spawn_timeout_tests`).
- The deadline test (`session_attach_cross_host_acceptance.rs:441`) aborts the peer's RPC task and waits out 30 s under a 60 s outer timeout.
- `rpc_scenarios` (`packages/tddy-livekit/tests/rpc_scenarios.rs`, 1253 lines) is one `#[tokio::test] #[serial]` with ten blocks sharing one testkit, each with its own `room_name` and harness.

### State B (Target)

- `DaemonConfig::peer_forward_timeout()` returns the configured deadline; the handle carries it; no forwarder reads `PEER_FORWARD_TIMEOUT` except as the documented default.
- The deadline test runs in a few seconds.
- Ten `rpc_scenarios`-derived tests, one per block, sharing fixtures, with no scenario lost.

### Delta

#### tddy-daemon-kernel

- `config.rs`: `peer_forward_timeout_secs`, `default_peer_forward_timeout_secs`, `peer_forward_timeout()`, tests beside `spawn_timeout_tests`.
- `peer_forwarding.rs`: the handle type; forwarders take it; `forward_server_stream_to_peer` opens within the handle's deadline.

#### tddy-daemon-livekit, tddy-host-service, tddy-session-lifecycle, tddy-session-agents, tddy-daemon-rpc

- Type change at each forwarding site; `split_forward_deadline` reads the accessor.

#### tddy-daemon, tddy-livekit (tests)

- Deadline test uses 2 s; `rpc_scenarios` split.

## Implementation Milestones

- [ ] Config key, accessor and tests
- [ ] Handle type; forwarders on it
- [ ] Call sites migrated crate by crate (each compiles on its own)
- [ ] Deadline test at 2 s; comments corrected
- [ ] `rpc_scenarios` split; scenario count unchanged
- [ ] Docs and code-issue measurements updated

## Testing Plan

### Testing Strategy

Unit tests for the key (default, override, clamp) and for the forwarder's deadline using a room that never answers; the silent-peer acceptance test against a real LiveKit server for the end-to-end claim; the split scenarios as the regression proof.

### Testing Principles Applied

The deadline test keeps asserting `DeadlineExceeded` (not a bare `is_err()`), so a missing deadline cannot pass as an up-front refusal. The guard test that pins the idle timeout against the roster's service threshold is left untouched.

### Coverage Requirements

No scenario of `rpc_scenarios` is dropped; the number of scenario tests equals the number of blocks (10) plus the existing second test.

## Acceptance Tests

### tddy-daemon-kernel

- `packages/tddy-daemon-kernel/src/config.rs` (`#[cfg(test)] mod peer_forward_timeout_tests`)
  - `the_peer_forward_deadline_defaults_to_thirty_seconds`
  - `the_peer_forward_deadline_is_read_from_the_daemon_yaml`
  - `a_zero_peer_forward_deadline_is_clamped_to_one_second`

### tddy-daemon

- `packages/tddy-daemon/tests/session_attach_cross_host_acceptance.rs`
  - `a_forwarded_rpc_to_a_peer_that_stopped_answering_fails_within_its_deadline` — sets `peer_forward_timeout_secs: 2`, asserts `DeadlineExceeded` naming 2 s and that it returns well inside the old 30 s

### tddy-livekit

- `packages/tddy-livekit/tests/rpc_scenarios.rs`: one test per scenario — `unary_calls_echo_empty_unicode_sequential_oversized_and_unknown`, `server_streams_deliver_in_order_and_refuse_unknown_services`, `client_stream_concatenates`, `bidi_stream_echoes_each_message`, `realtime_bidi_delivers_before_the_stream_closes`, `responses_reach_only_the_caller`, `stateful_bidi_uses_a_single_handler`, `bidi_input_arrives_in_order_under_rapid_send`, `loopback_tunnel_answers_ping_pong`, `a_duplicate_identity_replaces_the_earlier_participant`

## Technical Debt & Production Readiness

(Populated during development.)

## Decisions & Trade-offs

- **Name `peer_forward_timeout_secs`**, matching the other durations on `DaemonConfig`, not the `_seconds` of the source note.
- **The deadline rides with the room slot** (developer's decision): one type change at the seam, no global (the e2e tests run several daemons in one process), no ~25 new arguments.
- **Idle timeout stays fixed.** It is pinned below by the roster's service threshold (test) and above by `ROSTER_KEEPALIVE_INTERVAL * 2` (compile-time assert).
- **Stream-open deadline follows the setting**; the per-frame idle does not.
- The deliberate 12 s negative window in `common_room_set_metadata_handshake_repro` is left alone: it is the property under test.

## Dependencies

None. This node needs nothing from `unique-rooms`; it touches different files.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `unique-rooms` (`LiveKitTestkit::unique_room`) | the helper | not at all: split tests use their scenario's existing distinct room name | use or re-implement `unique_room` |

## Responsibility

- The `peer_forward_timeout_secs` key, the handle type and the forwarders' deadline.
- The 2 s deadline test and the `rpc_scenarios` split.

## Boundaries

This PR does **not**:

- Share a server or change `.config/nextest.toml`, the `docker` group or any CI workflow (nodes 4 and 5).
- Add `unique_room` or migrate room names (node 1).
- Change the stream idle timeout or `PASS_LONG_ENOUGH_TO_BE_SERVICE`.
- Change `refuse_departed_daemon` (a gone peer is still refused up front).

## Draft PR contract

The first push of this PR publishes:

- `DaemonConfig::peer_forward_timeout_secs` field, `peer_forward_timeout()` accessor (body `// TODO(deadline-and-scenarios): implement`), and the common-room handle type with the signatures the forwarders will take.
- Failing tests: the three config tests; the deadline test at 2 s. The split `rpc_scenarios` tests are written as the ten scenario tests and pass-through the existing bodies — they are a refactor and are not expected to fail.

`/green` implements the deadline end to end and moves every call site in this same PR; it must never merge in the contract state.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — the config tests are unit tests; the deadline test needs only this node's handle plus Docker
**Concurrent with:** `unique-rooms`, `compile-timings`
**Blocks:** nothing in the stack (its payoff — the split scenarios running in parallel — arrives with `parallel-livekit`)

Real dependency edges:

    unique-rooms → shared-livekit-ci, parallel-livekit      shared-livekit-ci → parallel-livekit

## Refactoring Needed

(Populated by validation commands.)

## Validation Results

(Populated by validation commands.)

## TODO

- [x] Record initial discovery (`2026-10-04-e2e-leg-peer-forward-deadline-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests (`./test -p tddy-daemon-kernel -p tddy-daemon-livekit -p tddy-host-service -p tddy-session-lifecycle -p tddy-session-agents -p tddy-daemon-rpc -p tddy-livekit`) and read the e2e leg on CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes the discovery file
- [ ] USER REVIEW — work complete, decide next steps
