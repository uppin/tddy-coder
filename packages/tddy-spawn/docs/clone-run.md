# Running a clone — progress, landing, stopping

`clone_run::clone_with_progress(os_user, git_url, destination, env, on_started, on_progress)`
(`src/clone_run.rs`) is the one implementation of a clone for the in-process and forked-worker backends.
`spawner::clone_as_user_with_env` is a thin call into it with no-op callbacks, and the worker's
`CloneRequest::run` calls it directly. The supervised backend is
`supervisor_spawn::clone_repo_via_supervisor_with_progress` (same callbacks), described
[below](#supervised-clones).

## What it does

1. **Already present → nothing.** A destination that exists returns `Ok` without a child, before any
   privilege check.
2. **Plan the privilege change** (`resolve_target_account`, `plan_privilege_change`) before any fork, so a
   process that cannot become `os_user` fails before anything is created
   ([spawning-as-another-user.md](spawning-as-another-user.md)).
3. **Spawn `git clone --progress <url> <dest>.partial-<uuid>`** through the kernel's `StagedCommand`, with
   `HOME` set to the account's home, `env` on the child, stdin and stdout null, stderr piped, and the
   child in **its own process group** (`process_group(0)`). `on_started` receives a `CloneHandle` once
   git is running.
4. **Read stderr as it is written.** Git ends a line with `\r` while a stage redraws and `\n` when it
   ends; each completed line is handed over immediately. A line that parses as progress goes through the
   throttle to `on_progress`; any other line is kept in a tail of the last 8.
5. **Land or clean up.** Exit 0 → `rename(partial, destination)` (a sibling, so one filesystem and atomic).
   Anything else → remove the partial directory and fail with `git clone failed (<status>): <tail>`. A
   failed rename removes the partial directory too.

A destination therefore either does not exist or holds a complete clone. A daemon that dies mid-clone
leaves only a `.partial-*` sibling, which no "already present" check matches.

## `CloneHandle`

`CloneHandle::for_process_group(pid)` names the clone by its git process, which leads the group.
`stop()` sends SIGTERM to the group, waits up to 2 seconds for it to empty, then sends SIGKILL; a group
that is already gone is not an error. It reaches ssh and index-pack as well as git. The running
`clone_with_progress` then sees git exit unsuccessfully and removes the partial directory. Unix only;
elsewhere `stop` returns an error. Cancelling a start (`#start-progress` 8/8) is its caller.

## Supervised clones

Under `tddy-supervisor` the daemon cannot fork git as another user, so
`supervisor_spawn::clone_repo_via_supervisor_with_progress(socket, os_user, git_url, dest, env,
on_started, on_progress)` makes **one** server-streaming `SpawnStreamed` call, built by
`plan_supervised_clone(os_user, clone_helper, git, git_url, destination, env)`: the clone helper
**`tddy-git-clone`** run as `os_user` with `--git <git> -- <url> <destination>`, `env` passed whole,
no scope. The supervisor knows nothing about clones; it runs one binary and streams its stderr.

- **The helper** is found through `TddyToolchain`, beside the daemon (the install contract `./install`,
  `./release`, `./publish.sh` and `scripts/dev-runtime-binaries.sh` keep); missing is a "failed
  precondition" refusal. It must sit under the supervisor's `exec_roots`.
- **git** is the absolute `git` on the spawn-child `PATH` (`git_for_supervised_clone`); none is a
  "failed precondition" refusal naming that `PATH`, never a bare `git` the session user's `PATH` would
  resolve. The plan refuses a relative helper, git or destination.
- **The helper does steps 3–5 above, as `os_user`**: `git clone --progress -- <url>
  <destination>.partial-<random>`, rename into place on success, removal of the partial directory on
  failure, refusal of an existing destination, and exit with git's code. Git's stderr passes through.

The stream is `Started{pid}`, one `Line{line}` per stderr line, then `Exited{code, signal, stderr_tail}`.

- `Started` → `on_started(CloneHandle::through_supervisor(socket, pid))`. That handle's `stop()` calls
  `StopSession(pid)` on a helper thread with its own runtime (it may be called from inside the daemon's
  runtime); the helper then removes the partial directory.
- `Line` events go through the same `git_progress::parse` and `ProgressThrottle` as a local clone; a
  line that is not progress is ignored.
- `Exited` with `code: Some(0)` is success; anything else fails with `git clone failed: <how> (dest=…):
  <stderr_tail>`. `code: None` means git was ended by a signal (`signal` is never set).
- A refusal (user, exec root, environment or scope) fails the call before any event. Dropping the
  receiver closes the connection, and the supervisor stops a child nobody is listening to.

`clone_repo_via_supervisor` is the no-callback wrapper, used by `CreateProject` / `AddProjectToHost`,
which report no progress.

## `git_progress`

`git_progress::parse(line) -> Option<GitProgress>` reads one line of git's progress:

```text
remote: Counting objects: 100% (1200/1200), done.
Receiving objects:  45% (36555/81234), 38.50 MiB | 4.25 MiB/s
Resolving deltas: 100% (51234/51234), done.
```

into `GitProgress { stage, percent, objects_done, objects_total, bytes_received, bytes_per_second, line }`
— the stage without its `remote: ` prefix, sizes in bytes from git's binary units (`bytes`, `KiB`, `MiB`,
`GiB`, `TiB`), 0 for a figure the line does not carry, and the trimmed line itself. A line without a
`NN% (done/total)` part (`Cloning into '…'...`, `Enumerating objects: 81234, done.`) is `None`.

`ProgressThrottle::admit(&progress, now)` forwards the first line, every stage change, every 100%, and
otherwise at most one line per `PROGRESS_INTERVAL` (250 ms). Throttling happens where git runs, so neither
the worker pipe nor a relayed start stream carries git's per-tick redraws.

`GitProgress` is `Serialize`/`Deserialize`: the worker sends it as `WorkerEvent::CloneProgress`
([spawn-worker.md](spawn-worker.md)).

## Testing

- `tests/git_progress.rs` — the parser on real git lines (bytes and throughput, KiB figures, the
  `remote: ` prefix and `, done.` tail, lines that are not progress) and the throttle.
- `tests/clone_progress_acceptance.rs` — a real `git clone` of a generated repository over `file://`
  (which, unlike a plain path, makes git transfer objects and report progress) lands at its destination
  with nothing beside it and reports `Receiving objects`; a failed clone leaves nothing and carries git's
  message; a clone stopped through its handle (a fake `git` on the child's `PATH`) ends promptly, its
  process is gone and nothing is left.
- `tests/worker_concurrency_acceptance.rs` — `the_worker_relays_clone_progress_before_the_clone_result`.
- `tests/supervised_clone_plan.rs` — a supervised clone is one `SpawnStreamed` of the clone helper as the
  session user, and an SSH clone's env needs no supervisor configuration.
- `packages/tddy-git-clone/tests/clone_helper.rs` — the helper's partial-sibling rename, its cleanup on
  failure, and its refusal of an existing destination.
