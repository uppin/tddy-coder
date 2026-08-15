# Remote git service module (`tddy_daemon::remote_git_service`)

## Role

Serves each of the daemon's **projects** as a git remote over any tddy-rpc transport. One `Serve`
stream carries one git pack operation: the client is `tddy-remote-git-repo`, which git execs as its
`GIT_SSH_COMMAND`; this module is the far end. Product contract:
[remote-git-repo.md](../../../docs/ft/daemon/remote-git-repo.md).

The file is two layers with a one-way dependency. Admission — verb, project, argv, environment — is
pure or pure-plus-filesystem and is unit-testable without a process. The relay underneath owns a
live child and its pipes. `serve` is the seam: it admits, then spawns, and holds nothing else.

## Why this is not the terminal RPC

The daemon already streams PTY bytes over RPC, and reusing that was the obvious plan. It cannot
carry git, for three independent reasons, each of which corrupts a packfile *silently* rather than
failing:

1. `open_replay_ack_live` (`tddy-terminal-rpc/src/bridge.rs:190`) emits the capture's mouse-tracking
   DECSET sequence before any payload byte. Git reads those escapes as pkt-line data.
2. The live bridge reads a `broadcast::Receiver` that drops on `Lagged`, from a ring that evicts. A
   terminal tolerates a dropped repaint; a packfile does not.
3. It is a PTY. The line discipline rewrites `\n` → `\r\n` and interprets `^C`/`^D`.

The terminal proto also carries no exit code and no separate stderr, and git needs both. So the wire
format is its own (`tddy-service/proto/remote_git.proto`); what was reused is the *client*
scaffolding, now `tddy_livekit::client_connect`.

## What keeps it from being a remote shell

Two properties, both enforced here rather than on the client, and both covered by tests that fail
when the property is removed:

- **The verb whitelist is closed.** `resolve_git_verb` accepts exactly `git-upload-pack` and
  `git-receive-pack` (in both spellings git uses) and returns `PERMISSION_DENIED` for everything
  else, near-misses like `git-upload-archive` included. The client checks too, but that is a fast
  path, never the gate.
- **The repository path never comes from the request.** `resolve_project_repo` looks `project_ref`
  up in the calling OS user's own registry and returns the row's `main_repo_path`. A `project_ref`
  that looks like a path (`/etc`, `../../etc`) is simply an unknown *name* → `NOT_FOUND`. There is
  no code path where a client-supplied string reaches a directory, a shell, or an argv position git
  would read as an option — `git_child_argv` emits `["git", <verb>, "--", <repo>]`.

## Admission order

`authorize_open` runs the whole decision **before anything is spawned**, and every failure is a
`Status` with no side effect:

| Step | Failure |
|------|---------|
| `session_token` → GitHub login (same resolver as every other daemon RPC, `auth.rs`) | `UNAUTHENTICATED` |
| login → OS user via the config's `users:` map | `PERMISSION_DENIED` |
| OS user → that user's `~/.tddy/projects/` | `INTERNAL` (a resolver that yields nothing is a fault, not a silent skip) |
| `project_ref` → registry row, by `project_id` then `name` | `NOT_FOUND` |
| row's `main_repo_path` exists | `FAILED_PRECONDITION` |
| `verb` → the closed whitelist | `PERMISSION_DENIED` |
| a free concurrency slot | `RESOURCE_EXHAUSTED` |

`session_token` is never logged, never interpolated into a `Status`, and never printed via prost's
`Debug`. Everything else about an admitted stream is logged at `info` so a failed clone is
diagnosable from the journal alone.

## Impersonation, and the environment that goes with it

The child runs as the **project's** OS user. `git_argv` resolves the account through
`pty_runtime::resolve_pty_os_user` and wraps the argv with
`pty_runtime::wrap_argv_for_privilege_drop` — the same helpers the PTY path uses, so the two cannot
drift on how a privilege drop is spelled. When the target *is* the daemon's own identity, no wrapper
is added.

The environment is **built, never inherited**. `setpriv` is deliberately environment-preserving, so
a child crossing a uid boundary would otherwise receive every variable the daemon was started with —
including `LIVEKIT_API_SECRET`, which signs session tokens, and which `git receive-pack` hooks and
`uploadpack.packObjectsHook` would see. `spawn_with_env` clears the environment and hands the child
exactly the `HOME`/`PATH` that `pty_runtime::pty_user_env_overrides` computes for the target user;
that is also what makes git read the right `.gitconfig`. `serve` uses this path exclusively.
`spawn_under_daemon_identity` is named for its one precondition, because the failure it guards
against is silent.

## Three bounded resources

Each replaces a way the workload could otherwise consume the host:

- **`GIT_FRAME_CHANNEL_CAPACITY = 8`** — the output channel is bounded, so a `pack-objects` that
  outruns the wire cannot accumulate the pack in the daemon's heap (a 2 GB clone becoming 2 GB of
  RSS). Backpressure lands in the kernel pipe buffer and then in the child's own `write`. Measured
  to cost nothing: 8 and 64 both clone 150 MiB at 2.54 MiB/s, so the transfer is wire-bound.
- **`MAX_GIT_FRAME_BYTES = 32 KiB`** — kept under `tddy_livekit::chunking::MAX_CHUNK_FRAME_BYTES`
  (60 000) so a git frame is never chunk-framed; a lost chunk wedges a call with no error, and a
  pack is exactly the workload that would produce oversized frames.
- **`MAX_CONCURRENT_GIT_STREAMS = 16`** — a semaphore across every user. Global rather than
  per-user, which bounds the host but still lets one busy user take every slot; recorded in
  `docs/dev/TODO.md`.

## The child does not outlive the connection

This is the deliberate difference from the terminal path, which leaves its PTY running on a client
disconnect by design. The relay is owned solely by `serve`'s inbound-pump task, so the child's
lifetime *is* the inbound stream's: when the client closes it — or the transport closes it because
the peer left the room — the loop ends, the relay drops, and the child is signalled.

The signal goes to the child's **process group** (`.process_group(0)` at spawn, `kill(-pid, …)` at
teardown), SIGTERM then SIGKILL after `CHILD_TERMINATION_GRACE`. `git-upload-pack` forks
`pack-objects`, so signalling only the direct child would leak a grandchild holding the object
database open.

`supervise_child` joins the output pumps before emitting the final `done` frame, so bytes written
just before exit are not lost to the teardown race. That join is bounded by `PUMP_STALL_TIMEOUT`,
but only while the pumps are making *no* progress at all — neither delivering a frame nor parked
writing one — so a slow wire cannot trip it. A flat deadline would have truncated a legitimate pack
on a slow link, trading a hang for silent data loss.

## Wire contract

`Serve(stream GitClientFrame) returns (stream GitServerFrame)`. The first client frame carries
`open` (`session_token`, `project_ref`, `verb`); its `stdin`/`stdin_eof` are honoured too, so a
client that packs payload alongside its open loses nothing. Server frames carry `stdout` and
`stderr` in separate fields — git writes progress to stderr while the pack goes down stdout, and
interleaving them would corrupt the pack — and the stream ends with exactly one frame carrying
`exit_code` and `done = true`.

## Related

- [remote-git-repo.md](../../../docs/ft/daemon/remote-git-repo.md) — product contract and acceptance criteria
- [project-concept.md](../../../docs/ft/daemon/project-concept.md) — the registry `project_ref` resolves against
- [session-auth.md](../../../docs/ft/daemon/session-auth.md) — the token model `session_token` belongs to
- `packages/tddy-daemon/src/auth.rs` — `LiveKitTokenServiceImpl`, the room-JWT mint that lets a client reach this service without holding the fleet secret
