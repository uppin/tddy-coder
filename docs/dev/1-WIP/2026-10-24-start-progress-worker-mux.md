# Changeset: The spawn worker serves requests concurrently

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — one crate but concurrency and ordering semantics over a pipe protocol, a forked-process seam with no existing tests
**Stack**: #start-progress 2/8. Base: `feature/start-progress/spawn-diag`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-start-progress-worker-mux-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md](../todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md) | ⚠ DURING | `spawn_worker.rs` is 648 lines. The multiplexing client and the wire types go in new modules under `spawn_worker/` (beside `clone_request.rs`); `spawn_worker.rs` must not grow. |
| `tddy-spawn` has no `docs/code-issues/` | ℹ | Not analyzed. |

## Responsibility

The forked spawn worker's protocol and concurrency. Owns:

- Request ids on every `WorkerRequest`; every worker line (`event` or terminal) carries the id it
  answers.
- The worker serving each request on its own thread, writing lines under one write lock.
- `SpawnClient` as a multiplexer: one reader thread dispatching lines by id to per-request channels;
  a request API that yields intermediate events before the terminal result.
- Worker death: EOF on the response pipe fails every pending request with an error naming the
  worker pid.

## Boundaries

- Defines one event, `WorkerEvent::Started { pid }` (the child's pid, sent as soon as it is spawned).
  Clone progress is node 5's (`WorkerEvent::CloneProgress` is added there); pre-exec stages stay in the
  log (node 1) and are not relayed.
- Does not cancel requests (node 8), restart a dead worker, or change spawn/clone semantics.
- Does not touch the supervisor backend.

## Dependencies

None in behaviour. The branch sits on `feature/start-progress/spawn-diag` (1/8) only because the stack
is a line; node 1 rewrites `spawner.rs`'s spawn functions, which this node's worker calls unchanged.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/start-progress/spawn-diag` (1/8) | `spawn_as_user` / `clone_as_user_with_env` on `StagedCommand` | the worker calls them as before | change their signatures, the privilege drop or the stage logging |

## Draft PR contract

```rust
// tddy-spawn/src/spawn_worker/wire.rs (new)
pub type RequestId = u64;
#[derive(Serialize, Deserialize)] pub struct WorkerEnvelope { pub id: RequestId, #[serde(flatten)] pub request: WorkerRequest }
#[derive(Serialize, Deserialize)] #[serde(tag = "op")]
pub enum WorkerLine {
    #[serde(rename = "event")]   Event   { id: RequestId, event: WorkerEvent },
    #[serde(rename = "result")]  Result  { id: RequestId, response: WorkerResponse },
}
#[derive(Serialize, Deserialize)] #[serde(tag = "kind")]
pub enum WorkerEvent { #[serde(rename = "started")] Started { pid: u32 } }

// tddy-spawn/src/spawn_worker/client.rs (new) — SpawnClient moves here, re-exported from spawn_worker
impl SpawnClient {
    pub fn spawn(&self, req: SpawnRequest) -> anyhow::Result<SpawnResult>;                       // unchanged signature
    pub fn clone_repo(&self, req: CloneRequest) -> anyhow::Result<()>;                            // unchanged signature
    pub fn clone_repo_observed(&self, req: CloneRequest, on_event: impl FnMut(WorkerEvent)) -> anyhow::Result<()>;
}
/// Test seam: a client over two arbitrary pipes and a worker loop over the other ends, without forking.
pub fn spawn_worker_over(request: std::fs::File, response: std::fs::File) -> std::thread::JoinHandle<()>;
pub fn client_over(request: std::fs::File, response: std::fs::File, worker_pid: u32) -> SpawnClient;
```

Failing tests pin concurrency (a slow request does not delay a fast one), id routing, events before
the result, and worker-death fan-out.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — tests drive the worker loop over in-process pipes with fake
programs as the current user; nothing here needs node 1's behaviour.
**Concurrent with:** #734 spawn-diag (1/8), supervisor-handoff (3/8), split-stream (4/8)
**Blocks:** clone-progress (5/8) — clone progress travels as worker events

    n1 → n5      n2 → n5      n5 → n6, n7, n8      n3 → n6      n4 → n8      n6 → n8

## Affected Packages

- **tddy-spawn**: [README.md](../../../packages/tddy-spawn/README.md) — worker protocol and client

## Related Feature Documentation

- PRD: [PRD-2026-10-24-start-progress-worker-mux.md](../../ft/daemon/1-WIP/PRD-2026-10-24-start-progress-worker-mux.md)

## Summary

The forked spawn worker multiplexes concurrent requests by id and can stream events for a request
before its result.

## Background

The worker serves one request at a time over newline-JSON pipes with no ids. A long clone blocks every
session spawn on a desktop or `--user` host, and because `send_and_recv` releases the write lock before
taking the read lock, two concurrent callers can read each other's responses. After a timeout the worker
stays wedged behind the hung request for good. Clone progress (node 5) needs events before the result.

## Scope

- [ ] Wire types with request ids, events and results
- [ ] Worker: thread per request, single write lock, request id echoed on every line
- [ ] Client: reader thread, per-request channels, `clone_repo_observed`
- [ ] Worker EOF fails every pending request naming the worker pid
- [ ] `spawn_worker.rs` does not grow; new code in `spawn_worker/`
- [ ] Docs: `tddy-spawn` README worker section

## Technical Changes

### State A
`WorkerRequest {op, payload}` / `WorkerResponse {op}` over two pipes; `spawn_worker_main` loops
`reader.lines()` synchronously; `SpawnClient { request_tx: Mutex<File>, response_rx: Mutex<BufReader<File>> }`
with write-then-read under separate locks; no tests fork or drive the worker.

### State B
Requests are `{"id":N,"op":…,"payload":…}`; the worker spawns a thread per request and writes
`{"op":"event","id":N,…}` / `{"op":"result","id":N,…}` lines under one `Mutex<File>`. The client owns
a reader thread holding `HashMap<RequestId, Sender<WorkerLine>>`; callers block on their own receiver.
EOF drains the map with `"spawn worker pid=<n> exited; request <id> abandoned"`.

### Delta
- `tddy-spawn`: new `spawn_worker/wire.rs`, `spawn_worker/client.rs`; `spawn_worker.rs` keeps
  `fork_spawn_worker` and the request handlers, loses `SpawnClient` and the old line types.

## Testing Plan

Integration tests in `packages/tddy-spawn/tests/` over `spawn_worker_over` + `client_over` (real
threads, real pipes, no fork), with fake programs written to a tempdir and `PATH` carried in the
request env (the seam discovery found). One test forks the real worker via `fork_spawn_worker` to
prove the production wiring.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_spawn_requested_during_a_slow_clone_completes_first` | `packages/tddy-spawn/tests/worker_concurrency_acceptance.rs` (new) | no head-of-line blocking |
| `concurrent_requests_each_receive_their_own_result` | same | id routing under interleaving |
| `a_clone_reports_its_child_pid_before_its_result` | same | `WorkerEvent::Started` via `clone_repo_observed` |
| `every_pending_request_fails_naming_the_worker_when_it_exits` | same | EOF fan-out |
| `the_forked_worker_serves_two_requests_at_once` | same | production `fork_spawn_worker` path |

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Thread per request, not fork per request.** Forking a thread-bearing worker is safe now that
  node 1 made every `pre_exec` async-signal-safe; a fork per request would need its own response
  plumbing for no gain.
- **Events are exercised through real ops.** No test-only request kind; `Started { pid }` is real
  behaviour (node 8 uses the pid to stop a clone).
- **No version skew to handle.** Worker and daemon are one binary image (fork without exec).

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-start-progress-worker-mux-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests (`./test -p tddy-spawn`); full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped clippy, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
