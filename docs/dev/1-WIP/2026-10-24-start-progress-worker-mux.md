# Changeset: The spawn worker serves requests concurrently

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — one crate but concurrency and ordering semantics over a pipe protocol, a forked-process seam with no existing tests
**Stack**: #start-progress 2/8. PR: #735. Base: `feature/start-progress/spawn-diag`.

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

- [x] Wire types with request ids, events and results
- [x] Worker: thread per request, single write lock, request id echoed on every line
- [x] Client: reader thread, per-request channels, `clone_repo_observed`
- [x] Worker EOF fails every pending request naming the worker pid
- [x] `spawn_worker.rs` does not grow; new code in `spawn_worker/`
- [~] Docs: `tddy-spawn` worker section — the README this names does not exist, and `packages/tddy-spawn/docs/` is written only by the wrap's changeset transfer; the protocol is recorded in State B and Decisions below until then

## Technical Changes

### State A
`WorkerRequest {op, payload}` / `WorkerResponse {op}` over two pipes; `spawn_worker_main` loops
`reader.lines()` synchronously; `SpawnClient { request_tx: Mutex<File>, response_rx: Mutex<BufReader<File>> }`
with write-then-read under separate locks; no tests fork or drive the worker.

### State B
Requests are `{"id":N,"op":…,"payload":…}`; the worker spawns a thread per request and writes
`{"line":"event","id":N,"event":{"kind":"started","pid":P}}` / `{"line":"result","id":N,"response":…}` lines
under one `Mutex<File>`. (The `line` tag, not `op`, is what `worker_concurrency_acceptance` pins.) The client owns
a reader thread holding `HashMap<RequestId, Sender<WorkerLine>>`; callers block on their own receiver.
EOF drains the map with `"spawn worker pid=<n> exited; request <id> abandoned"`.

### Delta
- `tddy-spawn`: new `spawn_worker/wire.rs`, `spawn_worker/client.rs`; `spawn_worker.rs` keeps
  `fork_spawn_worker` and the request handlers, loses `SpawnClient` and the old line types.

## Testing Plan

Integration tests in `packages/tddy-spawn/tests/` over `spawn_worker_over` + `client_over` (real
threads, real pipes, no fork), with fake programs written to a tempdir and `PATH` carried in the
request env (the seam discovery found). One test forks the real worker via `fork_spawn_worker` to
prove the production wiring — **dropped at the contract commit**: forking the multi-threaded test
harness and running the worker loop in the child risks the very allocator/lock deadlock the worker
exists to avoid. The production fork path's only change is the loop it runs, which
`spawn_worker_over` exercises.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_spawn_requested_during_a_slow_clone_completes_first` | `packages/tddy-spawn/tests/worker_concurrency_acceptance.rs` (new) | no head-of-line blocking |
| `concurrent_requests_each_receive_their_own_result` | same | id routing under interleaving |
| `a_clone_reports_its_child_pid_before_its_result` | same | `WorkerEvent::Started` via `clone_repo_observed` |
| `every_pending_request_fails_naming_the_worker_when_it_exits` | same | EOF fan-out |
| `every_worker_line_names_the_request_it_answers` | same | wire format |

## Technical Debt & Production Readiness

- **Load-sensitive acceptance bound (not fixed here).** `a_fast_clone_completes_while_a_slow_clone_is_still_running`
  asserts a quick clone finishes within 2 s of wall clock. Under host load average ~113 it failed 2 of 12 runs
  (quick clone 3.3–3.6 s, `id -un` and `sh` spawns starved); at load ~70 it passed 10 of 10. No lock spans a
  request in the worker or the client path, so this is process-spawn latency, not serialization. The test file
  is outside this node's scope: flagged for a human to decide whether the bound should widen.
- **Package docs deferred to the wrap.** `packages/tddy-spawn/README.md` (named in Affected Packages) does not
  exist; `packages/tddy-spawn/docs/` is written only by the wrap's changeset transfer.

## Decisions & Trade-offs

- **Thread per request, not fork per request.** Forking a thread-bearing worker is safe now that
  node 1 made every `pre_exec` async-signal-safe; a fork per request would need its own response
  plumbing for no gain.
- **Events are exercised through real ops.** No test-only request kind; `Started { pid }` is real
  behaviour (node 8 uses the pid to stop a clone).
- **No version skew to handle.** Worker and daemon are one binary image (fork without exec).
- **The clone pid comes through a sibling, not a new signature.** `clone_as_user_observed` takes an
  `on_started` callback and `clone_as_user_with_env` keeps its signature (it delegates with a no-op):
  `tddy-agent-launch`, `tddy-spawn`'s clone-privilege test and node 1 all call it directly. The change
  inside it is `spawn` + `wait_with_output` in place of `output`, which is what `Command::output` does.
- **The worker's pipes are close-on-exec.** `libc::pipe` leaves them inheritable, so a long-running
  `tddy-coder` the worker execs would keep the response pipe's write end open after the worker died: the
  daemon would never see EOF and its pending requests would hang. Found while greening; the change is
  confined to `fork_spawn_worker`'s two pipes.
- **The worker drains before it exits.** The request loop runs on scoped threads, so EOF on the request
  pipe returns only after every request still running has written its result.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer for the whole of pass 2, 2026-10-24)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [~] Run scoped tests (`./test -p tddy-spawn`); full-workspace result from CI — scoped: library 39/39, `worker_concurrency_acceptance` 5/5, the other suites green; inherited node-1 red, not this node's: `clone_privilege_acceptance` (1), `pre_exec_safety` (2)
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [x] Linting and formatting (scoped clippy, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
