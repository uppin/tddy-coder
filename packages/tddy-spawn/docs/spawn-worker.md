# The spawn worker

On a host without `tddy-supervisor` (Tddy Desktop, `./install --user`), the daemon spawns sessions and
clones repositories through a **spawn worker**: a child process forked by `fork_spawn_worker` before the
async runtime starts, so no tokio thread can hold an allocator or libc lock across the fork. The daemon
talks to it through `SpawnClient` over two close-on-exec pipes.

**Which backend applies** is decided once, before any thread starts, by
`supervisor_client::SpawnBackendChoice::adopt_from_environment`: `Supervisor { socket_path }` when
`tddy-supervisor` started the process with `TDDY_SUPERVISOR_SOCKET`, `ForkedWorker` otherwise. There is
no config switch. `tddy-daemon` `main` and `tddy-desktop` `run` then call
`consume_from_process_environment()`, which removes the variable from the process environment while
that is still sound, so no child the daemon spawns itself inherits it. `spawn_worker_for` forks the
worker only for `ForkedWorker`; the choice is carried in `RuntimeOptions` and injected into every
spawn and clone site, and an unreachable supervisor fails the operation (`connect_supervisor`) with no
fallback to a worker.

## Modules

| Module | Holds |
|---|---|
| `spawn_worker.rs` | `fork_spawn_worker`, the request types (`WorkerRequest`, `SpawnRequest`, `WorkerResponse`), `run_spawn` |
| `spawn_worker/wire.rs` | `RequestId`, `WorkerEnvelope`, `WorkerLine`, `WorkerEvent` |
| `spawn_worker/serve.rs` | the worker loop (`serve`) |
| `spawn_worker/client.rs` | `SpawnClient`, the line router, and the fork-free seams `spawn_worker_over` / `client_over` |
| `spawn_worker/clone_request.rs` | `CloneRequest` and how it runs |

## Wire protocol

Newline-delimited JSON, one line per message.

- **Request** (daemon → worker): `{"id":N,"op":"spawn"|"clone","payload":…}` — a `WorkerEnvelope`, the
  `WorkerRequest` flattened beside its id. Ids are assigned by the client from a per-client counter.
- **Event** (worker → daemon): `{"line":"event","id":N,"event":{"kind":"started","pid":P}}` — something
  that happened to request `N` before its result. `Started { pid }` is sent as soon as a clone's `git`
  child is running (`clone_run::clone_with_progress` hands its `CloneHandle` out between `spawn` and
  `wait`); `CloneProgress(GitProgress)` (`"kind":"clone_progress"`) for each throttled line of git's
  progress, all before the clone's result. See [clone-run.md](clone-run.md).
- **Result** (worker → daemon): `{"line":"result","id":N,"response":…}` — the last line for request `N`.

Worker and daemon are one binary image (fork without exec), so there is no version skew to negotiate.

## Concurrency

- **Worker**: each request line is served on its own scoped thread; every line it writes is serialized
  and flushed under one `Mutex<File>`, so lines never interleave and a slow request never holds up a fast
  one. When the request pipe closes, `serve` returns only after every running request has written its
  result. A request line that does not decode is answered with an error only when it still names an id.
- **Client**: one reader thread routes each line by id to the waiting caller's channel. A route is
  registered **before** the request is written, so an answer cannot arrive ahead of its route. Callers
  block on their own channel only; `clone_repo_observed` hands each `WorkerEvent` to a callback before
  returning the result.
- **Worker death**: EOF (or an undecodable line) on the response pipe closes the pending table — every
  waiting caller fails with `spawn worker pid=<n> exited; request <id> abandoned`, and later requests fail
  with `… not sent`. A dead worker is not restarted, and requests cannot be cancelled.

## Close-on-exec pipes

`libc::pipe` leaves both pipes inheritable. A long-running process the worker execs would otherwise keep
the response pipe's write end open after the worker died, and the daemon would never see EOF. Both pipes
are marked `FD_CLOEXEC` right after creation.

## Tests

`tests/worker_concurrency_acceptance.rs` drives the real worker loop on a thread over real pipes
(`spawn_worker_over` + `client_over`, no fork — forking the multi-threaded test harness risks the very
deadlock the worker exists to avoid), cloning with a fake `git` placed on the child's `PATH` through the
request's own environment as the current user.
