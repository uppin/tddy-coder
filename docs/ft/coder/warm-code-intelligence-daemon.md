# Warm Code-Intelligence Daemon

**Product area:** Coder / tddy-tools
**Status:** Active
**Updated:** 2026-09-16

## Summary

**`tddy-index-daemon`** holds a warm rust-analyzer index per workspace root and serves the
restructuring and analysis operations as `code_index.CodeIndexService`, over gRPC and stdio. It is
one binary with two lifetimes: given no transport argument it runs a single operation in process and
exits; given `--grpc`, `--grpc-uds` and/or `--stdio` it serves and stays alive.

Without it, every `tddy-tools restructure` invocation pays a full crate-graph load — six to ten
minutes on this workspace — because the CLI builds its language-server registry per process. An
iterative carve is a sequence of plan-fix-retry cycles, so it pays that cost once per retry.

**Measured: a second request against a warm root is ~2,500× faster than its cold load** (865 µs
against 2.15 s, real rust-analyzer, three-crate workspace).

## Two lifetimes, one implementation

```text
tddy-index-daemon restructure check --workspace-root <dir> <plan>   # run once, exit(0|1)
tddy-index-daemon analyze complexity --workspace-root <dir> <file>  # the analysis half, likewise
tddy-index-daemon --grpc 127.0.0.1:7777                            # serve gRPC over TCP
tddy-index-daemon --grpc-uds <path>                                # serve gRPC over a Unix socket
tddy-index-daemon --stdio                                          # serve over this process's stdio
tddy-index-daemon --grpc 127.0.0.1:7777 --stdio                    # both, concurrently, one index
tddy-index-daemon --ping <path>                                    # is a daemon serving this socket?
tddy-index-daemon                                                  # error: nothing to do
```

`--ping` is a third lifetime, and the shortest: it serves nothing, runs no operation, reaches no
language server and loads no crate graph. It dials the socket, issues `Workspaces`, and exits with
whether anything answered. It exists because the two cheaper probes both lie — a pid says something
with that number is alive, and a socket **file** outlives the process that bound it — and
`run-index-daemon --status` reported healthy daemons that refused the next connection on exactly
that pair.

Single-shot calls the generated service trait **in process** — prost structs in and out, no encode or
decode — so there is one code path rather than two that must be kept in step. Several transports
share one implementation instance, so they share one warm index and one per-root request queue.

Neither a subcommand nor a transport is an error rather than a default: a process started with
nothing to serve would serve nobody, and failing fast beats coming up and waiting forever.

`--stdio` dedicates fd 1 to RPC framing. The logger is forced off stdout before it is installed, and
`--log-file` redirects fd 2 when the operator names a destination — not unconditionally, because a
parent that deliberately piped stderr is the normal `--stdio` caller and hiding its errors would be a
silent failure.

## One process, many worktrees

Every request names its `workspace_root`, and one process holds one language server per root, keyed
and reaped by `tddy-lsp`'s registry. Nothing reads the process directory.

**The benefit is total on one root and narrows sharply across several.** The hosts are cheap — a
backend registry and a document-version map per root — but the rust-analyzers are not, and two
resident on one machine compete for CPU. Measured: a second request on one warm root is ~2,500×
faster than its cold load; re-asking a root after a *different* root's graph loaded ranged from
880 ms to 3.6 s against a 2.19 s cold load, the upper end slower than cold.

Requests touching the tree or the journal are serialized per root, because `.restructure/` is keyed
by root with no lock file and `open_run` refuses a second plan. `Warm`, `Workspaces` and `Complexity`
are not serialized: they touch neither, and queueing "is this root warm?" behind a 55-minute capture
would make it unanswerable.

## The service

| RPC | Shape | Notes |
|---|---|---|
| `Warm` | server-streaming | Loads a root and reports progress. Idempotent |
| `Check` | server-streaming | Every finding in a plan, no writes. Streams one `Finding` per finding |
| `Apply` | server-streaming | Executes a plan. Streams indexing, per-operation and outcome events |
| `Anchors` | unary | A range anchor covering named items |
| `PlanStatus` | unary | completed / in-flight / pending / failed |
| `Verify` | unary | Statement-multiset comparison against a git ref |
| `Workspaces` | unary | Which roots this process holds an index for |
| `Coverage` | server-streaming | Per-test coverage capture. Tens of minutes |
| `Report` | unary | CRAP leaderboard over a capture |
| `DuplicateTests` | server-streaming | Identical and subset coverage signatures |
| `Complexity` | unary | Per-function cyclomatic complexity of one file |

The long operations stream for two reasons. Progress happens *while* a call is in flight and has
nowhere else to go; and a stream is the only back-channel a handler gets, so **a send failing into a
dropped receiver is how the service learns its caller has gone away** and cancels the work. A unary
handler has no way to find that out.

`Warm.ready` currently means a live server holds the root, not that its graph is loaded — see the
backlog entry of that name.

### Refusals

One mapping, in one exhaustive `match` with no catch-all arm, so a refusal cannot reach two
transports as two different codes and a new error variant is a compile error:

| Class | Meaning to a caller |
|---|---|
| `InvalidArgument` | the request is wrong — a malformed plan, code text in a plan, an unsupported operation, a file no backend handles, unparseable Rust |
| `FailedPrecondition` | the tree is wrong — an unreachable root, a relative root, a plan that is not there, a snapshot mismatch, an existing journal, a missing coverage capture |
| `DeadlineExceeded` | the caller's own deadline expired, or it cancelled; the message says how far the index got |
| `Unavailable` | the server asked to be asked again |
| `Internal` | neither caused by the caller nor fixable by them |

## Running it

```bash
eval $(./run-index-daemon | grep '^export ')   # exports TDDY_INDEX_SOCKET
tddy-tools restructure check plan.jsonl        # now costs the assist, not the index
./run-index-daemon --status                    # dials the socket; non-zero if nothing answers
./run-index-daemon --stop
```

**The daemon is started in a session of its own**, via `setsid` from the dev shell, and its pid is
written from inside the process that becomes the daemon rather than read from the starting shell's
`$!`. Both matter, and neither is incidental: a background job started with `nohup` alone stays in
the process group of the shell that launched it, so an agent harness, a CI step or a job-control
terminal tearing that group down takes the daemon with it — which is how several starts announced
`listening on …` and then refused the connection seconds later, leaving the socket file behind. And
`setsid` forks when its caller is already a process group leader, so `$!` names the daemon in some
shells and a wrapper in others; `--stop` reading the wrong one would kill the wrapper and orphan a
multi-gigabyte rust-analyzer.

One daemon per checkout, keyed by a checksum of the resolved root — reusing another worktree's daemon
would run *that* worktree's restructuring code against this tree's source. Its socket, pid file and
log live under `TDDY_INDEX_RUNTIME_DIR` (default `$TMPDIR`), not in the checkout, because an AF_UNIX
path is about 104 bytes and a repo-local one does not fit.

`tddy-tools restructure` uses the daemon when **`TDDY_INDEX_SOCKET`** is set and non-empty, and
otherwise spawns its own language server exactly as before. A socket that is set but unreachable is an
error, not a silent fall back to the cold path.

The two front ends produce the same console: the answer on stdout, the server's narration on stderr,
each line stamped with the time since the line before it.

## What the daemon reports about itself

Per request, at `INFO`: the method, the root it named, **whether that root was already warm**, the
outcome, and how long it took.

```text
listening on /run/tddy/index.sock
anchors arrived for `/trees/one`, which has no index yet
anchors for `/trees/one`: answered (+2.1s)
anchors arrived for `/trees/one`, which is already warm
anchors for `/trees/one`: answered (+4ms)
```

`Workspaces` logs at `DEBUG`: it names no root, does no work, and is what a dashboard polls. A root
being reaped and a caller hanging up are logged too. An operation's own progress goes to its caller's
stream, never to the log, and nothing in the process writes to stdout.

## Lifecycle

`tddy-daemon` can own the process — lazily spawned on first need, supervised, restarted when it
exits, stopped when every root has gone idle, and cancelled rather than orphaned on shutdown. It is
off unless the daemon's configuration carries an `index_daemon:` section, so an existing deployment
is unaffected, and it also requires a user resolver, like every other service the daemon assembles.

A developer can equally start the same binary by hand with `run-index-daemon`, which is the only
path that exists without a daemon installed.

## Related documentation

- [Rust code restructuring](rust-code-restructuring.md) — the operations, the plan format, waiting
- [Rust code analysis](rust-code-analysis.md) — coverage, CRAP, duplicate tests, the complexity cache
- [Reusable LSP](reusable-lsp.md) — the registry both hosts share, and what a long-lived host needs
- [RPC multi-transport](rpc-multi-transport.md) — the transport contract this service is served under
- Package: [`packages/tddy-index-daemon/docs/code-index-service.md`](../../../packages/tddy-index-daemon/docs/code-index-service.md)

## Known limitations

- **`Warm.ready` means a live server holds the root**, not that its graph is loaded. Read the log's
  warm/cold line for the real answer.
- **A capture stops between tests, not mid-test.** A client that hangs up is noticed within one
  test of a capture and one signature of a duplicate-test detection, and the instrumented build is
  a single unit that cannot be interrupted once it has begun — so a disconnect during the build
  costs the rest of the build.
- **Warming several worktrees at once narrows the per-root benefit**, because the resident
  rust-analyzers compete for CPU.
