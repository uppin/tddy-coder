# Warm Code-Intelligence Daemon - PRD

**Date**: 2026-09-15
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Rust Code Restructuring](../rust-code-restructuring.md) — gains a daemon
  host, a workspace-root parameter, per-request progress streaming, the removal of the indexing
  budgets in favour of cancellation, and a distinguishable indexing-timeout error class. The
  documented CLI surface stays valid apart from `--indexing-budget`, which is withdrawn.
- **Primary Feature**: [Rust Code Analysis](../rust-code-analysis.md) — gains the same daemon host
  and a content-addressed complexity cache; its long captures become server-streaming jobs.
- **Related Feature**: [Reusable LSP](../reusable-lsp.md) — requirement 14 names `tddy-daemon` as
  the primary owner of the `LspRegistry`. This PRD adds a **second** host and says why; it also
  closes the gaps that make the registry unsafe for any host that outlives one request
  (`didChange` version state, idle-timer refresh, concurrent cold spawn).
- **Related Feature**: [RPC multi-transport](../rpc-multi-transport.md) — a new service is added
  under its existing contract; no new transport is introduced.
- **Related Feature**: [gRPC remote control](../grpc-remote-control.md) — the `--grpc` / `--stdio`
  concurrency rule is reused verbatim for a second binary.

## Summary

Running `tddy-tools restructure apply` pays a full rust-analyzer cold start — six to ten minutes on
this workspace — on **every invocation**, because `restructure_cli.rs` builds a `TaskRegistry` and
an `LspRegistry` per process and drops both on exit. An iterative carve, which is a sequence of
plan-fix-retry cycles, pays that cost once per retry.

This PRD introduces **`tddy-index-daemon`**: one binary that owns a warm rust-analyzer index and the
warm in-memory state built on top of it, and exposes the restructure and analyze operations as a
single service. The binary has two lifetimes selected by its arguments:

- **no transport argument** — single-shot. Run the requested operation, print, exit with a status.
- **`--grpc` and/or `--stdio`** — stay alive, serve that transport (or both concurrently) over one
  warm index.

Both lifetimes call the **same service trait**. Single-shot calls it in-process, passing Rust
values with no encode/decode; the transports wrap the identical implementation. There is one code
path, not two that must be kept in step.

## Background

### Why the CLI is cold every time

`packages/tddy-code-restructuring/src/restructure_cli.rs:97-131` constructs
`LspRegistry::new(restructure_allow_list(), TaskRegistry::new(), 600s)` as a **local variable**,
spawns rust-analyzer through it, and lets both drop when `run` returns. The 600-second idle timeout
never matters because the process exits first. The log line *"acquiring shared rust-analyzer
client"* means shared **within this process**.

Separately, `RustBackend::indexed` (`backends/rust.rs:544`) is per-backend and a fresh `RustBackend`
is built per invocation (`runner.rs:214`), so even against an already-warm server the warm-up probe
is re-paid.

### A separate process, whose lifecycle the daemon manages

[Reusable LSP](../reusable-lsp.md) requirement 14 makes `tddy-daemon` the owner of the
`LspRegistry`, and it already is one: `runtime.rs:874` registers an executor and `runtime.rs:304`
drives a 60-second reaper. This PRD does not contradict that. It splits *ownership of the index*
from *ownership of the lifecycle*:

- **The index lives in its own process.** rust-analyzer on this workspace is a multi-gigabyte
  resident child; the warm state built on it is worth restarting independently of the session
  daemon, and worth not paying for in headless or sandboxed deployments that never restructure
  anything. It also needs a different rust-analyzer handshake — `tddy-daemon` registers
  `LspAllowList::rust_only()`, which advertises **no** client capabilities, and rust-analyzer
  returns no code actions and sends no `$/progress` to such a client. The handshake is fixed at
  spawn, so the two cannot share one registry entry.
- **`tddy-daemon` decides when it runs.** It lazily spawns the index daemon on first need,
  supervises it, restarts it on crash, and stops it when every workspace root has gone idle —
  the same get-or-spawn-and-reap shape it already applies to rust-analyzer itself, one level up.
- **A developer can also start it by hand.** `run-index-daemon` starts the same binary with the
  same flags in a bare worktree with no daemon installed. The only difference is who called it.

### One process, many worktrees

The service is **root-parameterised**: every request carries `workspace_root`, and one process holds
one rust-analyzer per root. This is not new machinery — `LspRegistry` is already a
`HashMap<LspKey, ServiceEntry>` keyed by `(workspace root, language)` (`registry.rs:48`), with
`workspace_root_for` resolving to the outermost `Cargo.toml` ancestor (`:206`). Restricting the
daemon to a single worktree would mean *adding* a limitation and then re-implementing per process
what the registry already provides.

The usual arguments for process-per-worktree do not apply here:

| Concern | Why one process is fine |
|---|---|
| Memory | rust-analyzer is a child process either way — N roots means N servers whether the hosts are 1 or N. Per-root host overhead is a `BackendRegistry`, a `RustBackend` and a document-version map |
| Crash isolation | Already in-process: `get_or_spawn` checks `!handle.status().is_terminal()` and respawns a dead server rather than returning a dead handle (`registry.rs:78-93`), covered by `registry_reuse_test.rs:144` |
| Toolchain pinning | Set on the rust-analyzer `Command` at spawn (`rust.rs:754`), so it is per-root regardless |
| Journal collisions | `.restructure/` is keyed by root on disk with no lock (`runner.rs:797`) — per-root serialization is required in either topology |

The one real cost is that **a host restart loses every warm index at once**, and at roughly seven
minutes per root that is not the same as losing one. Two things bound it: roots re-warm lazily and
independently — nothing re-indexes until a request names that root — and because the service is
root-parameterised, a worktree that wants its own process gets one by pointing at a different
socket. Same binary, same code path, no special mode.

Clients therefore need no discovery protocol: **`TDDY_INDEX_SOCKET` names one endpoint**, and the
`workspace_root` already on every request is what routes it.

### Seven things that are safe in a one-shot process and break in a daemon

Recorded in full in the changeset's State A. In brief:

| # | Defect | Why a daemon exposes it |
|---|---|---|
| 1 | The whole budget cascade: `WARMUP_BUDGET`, `SETTLE_BUDGET`, `settle_budget_for` (`rust.rs:455-477`), and `resolution_budget()` switching to `settle` once `indexed` is true | In a warm daemon `indexed` is true from request #2 — **every** request runs on the settle path, so a 900s budget becomes a 45s ceiling. More fundamentally, a server has no business inventing a deadline the caller never asked for |
| 2 | `did_change` and its version counter live in `RustBackend` (`rust.rs:941`), not `LspClient` | Version restarts at 1 against a server that saw 40. `LspClient` has only `did_open`, hard-coded to version 1 |
| 3 | Seven `current_dir()` call sites; nothing takes a root | One process cannot serve two workspace roots |
| 4 | `progress: fn(&str)` that `println!`s (`runner.rs:846`) | Bare fn pointers cannot capture a per-request channel; `rust.rs:529` warns a server's stream would be corrupted |
| 5 | `.restructure/` keyed by root, no lock, no plan identity; `open_run` refuses with `JournalExists` | Two concurrent clients on one root collide |
| 6 | Idle timer refreshed only by `get_or_spawn` (`registry.rs:88`) | A reaper can reap rust-analyzer mid-restructure |
| 7 | `get_or_spawn` releases the map lock before spawning (`registry.rs:95`) | Concurrent cold requests double-spawn and orphan a live task |

### Two premises this PRD does not carry forward

The analysis this work derives from asserts that `restructure apply` exits 0 on refusal. It does
not: refusals propagate through `main() -> Result<()>` and exit **1**
(`tddy-tools/src/main.rs:157`). There is no `process::exit` in the crate. The real defect, recorded
in [`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`](../../dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md),
is the error **class** — an indexing timeout is reported as `plan is malformed` — which in a daemon
decides the gRPC status code and therefore whether a client should retry.

The same analysis treats "apply observability" as landed WIP on another branch. It is not present in
this worktree. It is not a separate deliverable here either: per-request progress **falls out** of
making the operations server-streaming, because a stream is the only place progress can go once
`println!` is off the table.

## Proposed Changes

### What's Changing

#### A new package: `packages/tddy-index-daemon`

Owns `proto/code_index.proto` (`package code_index;`, bare snake — a versioned name is refused by
[`2026-09-09-versioned-proto-package-names.md`](../../dev/todo/2026-09-09-versioned-proto-package-names.md)),
serves it, and publishes its coordinate. Follows `packages/tddy-terminal-rpc` exactly: a two-pass
`build.rs`, one implementation of the generated `tddy-rpc` trait, gRPC free via the generated
`CodeIndexServiceTonicAdapter`.

```
service CodeIndexService {
  rpc Warm(WarmRequest)                     returns (stream IndexProgress);
  rpc Check(CheckRequest)                   returns (stream RestructureEvent);
  rpc Apply(ApplyRequest)                   returns (stream RestructureEvent);
  rpc Anchors(AnchorsRequest)               returns (AnchorsResponse);
  rpc PlanStatus(PlanStatusRequest)         returns (PlanStatusResponse);
  rpc Verify(VerifyRequest)                 returns (VerifyResponse);
  rpc Coverage(CoverageRequest)             returns (stream AnalyzeEvent);
  rpc Report(ReportRequest)                 returns (ReportResponse);
  rpc DuplicateTests(DuplicateTestsRequest) returns (stream AnalyzeEvent);
  rpc Workspaces(WorkspacesRequest)         returns (WorkspacesResponse);
}
```

**Every request carries `string workspace_root`.** That is the fix for the seven `current_dir()`
sites and the thing that makes one process able to serve two worktrees.

**The four long operations are server-streaming.** `Apply`, `Check`, `Coverage` and
`DuplicateTests` emit progress as it happens and a terminal outcome at the end. This is what
replaces the `println!` sinks; it is also what makes a run observable, which no amount of stderr
formatting in a one-shot process can do for a caller on the other end of a socket.

#### Two lifetimes, one implementation

```bash
# single-shot: no transport argument
tddy-index-daemon restructure apply plan.jsonl
tddy-index-daemon analyze coverage --path packages/tddy-core
  → cold index, run, render to stderr, exit(0|1)

# daemon: a transport argument is present
tddy-index-daemon --grpc-uds /run/tddy/index.sock
tddy-index-daemon --grpc 127.0.0.1:7777 --stdio
  → warm index, serve, stay alive until SIGTERM / stdin close
```

Single-shot builds the service implementation and **calls the trait method directly**. The generated
trait's methods take and return prost message structs inside `tddy_rpc::Request`/`Response`, so an
in-process call passes Rust values with no serialization. The streamed responses render to stderr
through the same renderer shape `analyze_cli.rs:100` already uses.

**Starting with neither a subcommand nor a transport is an error**, not a default. This mirrors
`tddy-sandbox-runner`: *"Neither flag is a default for the other: started with no transport at all,
the jail would serve nobody, and failing fast beats a process that comes up and waits forever."*

`--grpc` and `--stdio` together serve **concurrently** over one warm index, exactly as
`tddy-coder` already does. Under `--stdio`, `tddy_core::stdio_safety::enforce_stdio_safe_log_output`
runs before `init_tddy_logger` and stderr is redirected to a file — stdout belongs to RPC framing.

#### Opt-in client wiring in `tddy-tools`

When **`TDDY_INDEX_SOCKET`** is set and non-empty, `tddy-tools restructure …` and
`tddy-tools analyze …` connect to the daemon at that path. When it is unset or empty, the current
cold-spawn path runs unchanged. **CI is untouched by construction**, and no existing invocation
changes behaviour without an explicit environment change. Empty-as-unset matches
`LIVEKIT_TESTKIT_WS_URL`.

A root script **`run-index-daemon`** starts or reuses a daemon and prints narration on stderr and a
single `export TDDY_INDEX_SOCKET=…` line on stdout, so `eval $(./run-index-daemon | grep '^export ')`
works — the handshake `run-livekit-testkit-server` established.

#### Budgets are removed, and cancellation replaces them

`WARMUP_BUDGET`, `SETTLE_BUDGET`, `settle_budget_for`, `resolution_budget()`, the `warmup`/`settle`
fields, `with_indexing_budget` and the `--indexing-budget` flag all go away. A server should not
invent a deadline the caller never stated; how long to wait is the caller's to decide.

What replaces them is **cancellation**, not a longer number. Three loops terminate today only
because of those deadlines — `rust.rs:980`, `:2058`, `:2086` — and `LspClient::request` bounds
itself with `tokio::time::timeout`. Each becomes "wait until ready, or until the caller goes away",
driven by a `CancellationToken` threaded through the backend. The primitive is already the house
one: `tokio-util` is a dependency of eight workspace crates, and `tddy-task` states the rule at
`task.rs:410` — *"Await `ctx.cancel_token().cancelled()` in each `tokio::select!` wait."*

Where the deadline then comes from, per caller:

| Caller | Bound |
|---|---|
| any streaming client | dropping the response stream fails the next send, which is the daemon's signal to cancel |
| gRPC client | tonic's own `grpc-timeout`, on top of the above |
| single-shot CLI | none by default — it waits and streams progress; `^C` cancels |
| `tddy-daemon`-managed | the daemon cancels on reap and on shutdown |

The token itself comes from a `TaskBody` in a `TaskRegistry`, and is checked **inside** the
synchronous poll loops. That indirection is not ceremony: `tddy_rpc::RpcService` has no cancellation
surface (`bridge.rs:33-69`), `ServerEngine`'s peer-disconnect abort is not in the tonic or
Connect-HTTP path at all, and aborting a future only stops it at an await point — while the
restructure backend runs inside `spawn_blocking` with `std::thread::sleep`. Recorded as
[`2026-09-15-rpcservice-has-no-cancellation-surface.md`](../../dev/todo/2026-09-15-rpcservice-has-no-cancellation-surface.md).

This is deliberately *not* a client-side deadline policy for `tddy_rpc::ClientEngine`.
[`2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`](../../dev/todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md)
records that no such policy exists and states why it must not ride along with a feature:

> It is a policy decision affecting every LiveKit RPC in the repo — including long-lived streams,
> which must not inherit a unary timeout — so it needs its own change rather than riding along with
> a feature.

This change supplies the **server** half — a handler that stops when its caller is gone — and
leaves that entry open.

`IndexingIncomplete { seconds, last, environment }` survives, repurposed: it is no longer a
budget expiry but the report of **how far the index had got when the request was cancelled**, which
is the part of it that was always the useful part (`rust.rs:418`'s `how_far()`). `ServerNotSettled`
is added so a server that cannot answer a method is distinguishable from a malformed plan.

Roughly ten existing unit tests assert the derivation being deleted (`rust.rs:5488-5513`,
`runner.rs:908-971`, `restructure_cli.rs:267-349`). They are replaced by cancellation tests, not
weakened or removed silently.

#### Error classes become a mapped surface

| `RestructureError` | gRPC status | Client meaning |
|---|---|---|
| `MalformedPlan`, `CodeTextInPlan`, `UnsupportedOp`, `NoBackend` | `InvalidArgument` | fix the plan |
| `SnapshotMismatch`, `AnchorInvalidated`, `JournalExists`, `CheckpointDivergence`, `IndeterminateJournal`, `NotAGitWorktree` | `FailedPrecondition` | fix the tree or the journal |
| `IndexingIncomplete`, `ServerNotSettled` | `DeadlineExceeded` | the caller's own deadline expired, or it cancelled; the message says how far the index got |
| `ServerCatchingUp` | `Unavailable` | retry |
| `Io` | `Internal` | — |

#### Warm state the daemon keeps, per workspace root

| State | Today | In the daemon |
|---|---|---|
| rust-analyzer process + `LspClient` | per invocation | one per `(root, Rust)`, reused |
| `BackendRegistry` / `RustBackend` (`indexed`, `doc_version`, `chatter`, `unresolved_token`) | per invocation | one per root, kept warm |
| Open-document versions | `RustBackend.doc_version`, dies with the process | moves into `LspClient`, per-URI |
| `complexity::file_complexity` results | recomputed every call | content-hash cache |
| `Overlay`, `PositionLedger`, `Journal` | per invocation | per **request**, never shared |

Index freshness is **rust-analyzer's own**: the client already auto-acknowledges
`client/registerCapability`, so the server registers and drives its own file watching. No `notify`
dependency is added — the workspace has none today, and `tddy-core/src/usage_watcher.rs:14` records
a deliberate decision to poll rather than take one.

### What's Staying the Same

- **Every documented `tddy-tools restructure` and `tddy-tools analyze` invocation keeps working,
  unchanged, with the same flags and the same exit codes.** The daemon is opt-in.
- The eight restructure operations, the JSONL plan format, the snapshot header, the refusal to
  accept `text`/`code`/`content` in a plan.
- `.restructure/journal.jsonl` and `ledger.json` stay on disk, keyed by root, in the same format.
- `git mv` for renames; the write-ahead commit sequence; `verify --against`.
- The handshake: `client_capabilities()`, `server_settings()`, utf-8 position encoding and the
  refusal of any other.
- `RESTRUCTURE_TRACE`.
- `tddy-daemon`'s own `LspRegistry` usage. It gains the registry fixes; nothing else changes.
- No new transport. `tddy-rpc` / `tddy-stdio` / tonic, as they are.

## Impact Analysis

### Technical Impact

**Code changes**

- `packages/tddy-index-daemon` (new) — proto, two-pass `build.rs`, service impl, workspace registry,
  single-shot renderer, `main.rs`.
- `packages/tddy-lsp` — `did_change` / `did_close` on `LspClient` with per-URI version state and an
  open-document set; `record_activity` on client use so a long run is not reaped; an in-flight
  placeholder in `get_or_spawn`; a drained stderr on `LspServerBody`.
- `packages/tddy-code-restructuring` — a workspace root parameter replacing seven `current_dir()`
  calls; `progress` becomes a boxed sink rather than a `fn` pointer; the whole budget cascade
  removed and a `CancellationToken` threaded through the three wait loops; `ServerNotSettled`;
  `StatePaths` / `open_run` / `commit_operation` promoted to `pub(crate)` or `pub` so the daemon
  can drive the apply loop without re-deriving it.
- `packages/tddy-code-analysis` — a content-hash complexity cache behind a trait so the daemon owns
  the storage; no change to the capture pipeline.
- `packages/tddy-tools` — client mode behind `TDDY_INDEX_SOCKET`.
- Root: `run-index-daemon`, `install` (`INSTALLED_BINARIES`), `Cargo.toml` members, `BUILD.yaml`.

**Dependencies.** `tddy-index-daemon` takes `tddy-rpc`, `tddy-stdio`, `tddy-service` (for
`to_tonic_status`), `tonic`, `prost`, `tddy-lsp`, `tddy-task`, `tddy-code-restructuring`,
`tddy-code-analysis`, `tddy-core`, `clap`, `tokio`, `anyhow`, `log`; build-deps `prost-build`,
`tddy-codegen`, `tonic-build`. All are existing workspace members or already-used crates —
**no new third-party dependency**, and specifically no `notify`.

**Performance.** First request per root pays the full index (unchanged, minutes). Later requests
within the idle window pay the assist only. Removing the budgets means a large-file operation waits
as long as its caller is willing to, instead of being refused at 45 seconds.

**Memory.** One rust-analyzer per warm workspace root, each multi-gigabyte on a workspace this size.
Bounded by the registry's existing idle reaping, and by `tddy-daemon` stopping the child once every
root is idle. A host that holds three warm roots holds three rust-analyzers — the same total as
three separate hosts would.

**Integration points.** `tddy-daemon` inherits the `tddy-lsp` fixes. Nothing else consumes these
libraries — `tddy-tools` and the crates themselves are the only workspace consumers.

### User Impact

**Workflow.** A carve session becomes: start the daemon once, export one variable, then run plans at
the cost of the assist rather than the index. Without the variable, nothing changes.

**Breaking changes.** One: **`--indexing-budget` is withdrawn**. A run now waits until it succeeds
or its caller stops it, so there is no budget to state. Scripts passing the flag must drop it; it is
named in `docs/ft/coder/rust-code-restructuring.md` § CLI and in
[`.agents/skills/code-restructuring`](../../../.agents/skills/code-restructuring/SKILL.md), both of
which this change updates. Two non-breaking behavioural changes: an indexing timeout no longer
reports as `plan is malformed`, and a long operation is no longer refused at 45 seconds.

**Migration.** None required; the daemon is opt-in.

## Implementation Plan

Delivered as **one pull request landed in milestones**, each a commit that leaves the tree green.
Ordered so every earlier milestone is independently valuable before any later one exists.

1. **Make the shared registry safe for a host that outlives one request** (`tddy-lsp`) —
   `didChange`/`didClose` with per-URI version state and an open-document set on `LspClient`; idle
   refresh on client use (`registry.rs:88`); an in-flight placeholder in `get_or_spawn`
   (`registry.rs:95`); a drained `stderr` (`server_body.rs:61`). Four defects in the registry
   `tddy-daemon` already runs today, so this is worth landing on its own terms. Proven against
   `fake_lsp`, in seconds rather than minutes.
2. **Cancellation replaces the budgets** (`tddy-code-restructuring`) — delete the cascade, thread a
   `CancellationToken` through the three wait loops, add `ServerNotSettled`. This is the ⛔ blocking
   item: it also fixes the CLI's present inability to apply a plan to `tddy-daemon` at all.
3. **Root-parameterised and sink-driven** (`tddy-code-restructuring`) — a workspace root on every
   entry point in place of the seven `current_dir()` calls; `progress: fn(&str)` becomes a boxed
   sink; the private apply-loop helpers promoted. `tddy-tools` passes cwd explicitly; behaviour
   identical.
4. **The crate, the proto, the service, both transports** (`tddy-index-daemon`) —
   `code_index.proto`, the two-pass `build.rs`, the service over a warm per-root registry,
   single-shot plus `--stdio` plus `--grpc`, and the coordinate-integrity trio. The daemon exists
   and works.
5. **The developer workflow** — `tddy-tools` client mode behind `TDDY_INDEX_SOCKET`,
   `run-index-daemon`, and `install`. Start once, export one variable, retries cost the assist.
6. **Daemon-managed lifecycle** (`tddy-daemon`) — lazily spawn, supervise, restart and stop the
   index daemon; cancel in-flight work on reap and shutdown. Automates milestone 5's manual step.
7. **The analyze surface** — the analyze RPCs and a content-hash complexity cache. Separate from
   milestone 4 because RPCs in a proto with nothing behind them are worse than RPCs not yet added.
8. **Documentation** — the three feature docs, the restructuring agent skill, and
   `packages/tddy-index-daemon/docs/`.

## Acceptance Criteria

- [ ] A second `Apply` against a warm daemon completes without re-indexing
      ([Rust Code Restructuring](../rust-code-restructuring.md))
- [ ] A long operation runs to completion instead of being refused at a derived budget
      ([Rust Code Restructuring](../rust-code-restructuring.md))
- [ ] A cancelled request stops the in-flight wait and reports how far the index got
      ([Rust Code Restructuring](../rust-code-restructuring.md))
- [ ] A disconnected stdio client cancels the work it started
      ([RPC multi-transport](../rpc-multi-transport.md))
- [ ] An indexing timeout is reported as a deadline, distinct from a malformed plan
      ([Rust Code Restructuring](../rust-code-restructuring.md))
- [ ] Two workspace roots are served by one process, each with its own index, and warming one does
      not warm the other ([Reusable LSP](../reusable-lsp.md))
- [ ] `tddy-daemon` starts the index daemon on first need and stops it when every root is idle
      ([Reusable LSP](../reusable-lsp.md))
- [ ] The same binary serves a developer who runs it by hand with no daemon present
      ([gRPC remote control](../grpc-remote-control.md))
- [ ] `--grpc` and `--stdio` serve concurrently from one process over one warm index
      ([gRPC remote control](../grpc-remote-control.md))
- [ ] Single-shot mode runs the same service implementation with no serialization, and exits
      non-zero on refusal ([Rust Code Restructuring](../rust-code-restructuring.md))
- [ ] Progress reaches a streaming client during a long apply, and nothing is written to stdout
      under `--stdio` ([RPC multi-transport](../rpc-multi-transport.md))
- [ ] A long-running operation is not reaped by the idle reaper ([Reusable LSP](../reusable-lsp.md))
- [ ] Concurrent cold requests for one root spawn exactly one server
      ([Reusable LSP](../reusable-lsp.md))
- [ ] A second document edit in one warm session carries a monotonically increasing version
      ([Reusable LSP](../reusable-lsp.md))
- [ ] `tddy-tools restructure` with `TDDY_INDEX_SOCKET` unset behaves exactly as today
      ([Rust Code Restructuring](../rust-code-restructuring.md))
- [ ] `analyze` operations are served from the daemon, with complexity cached by content hash
      ([Rust Code Analysis](../rust-code-analysis.md))
- [ ] The service answers at the coordinate its `.proto` declares
      ([RPC multi-transport](../rpc-multi-transport.md))
- [ ] Starting with no transport and no subcommand fails fast
      ([gRPC remote control](../grpc-remote-control.md))
- [ ] Tests passing for all affected features

## References

### Affected Features (Complete List)

- [Rust Code Restructuring](../rust-code-restructuring.md) — daemon host, root parameter, streaming
  progress, cancellation in place of budgets, error classes
- [Rust Code Analysis](../rust-code-analysis.md) — daemon host, streaming captures, complexity cache
- [Reusable LSP](../reusable-lsp.md) — a second registry host; document-sync, idle-refresh and
  cold-spawn fixes to the shared registry
- [RPC multi-transport](../rpc-multi-transport.md) — a new service under the existing contract
- [gRPC remote control](../grpc-remote-control.md) — the `--grpc` / `--stdio` concurrency rule

### Related Documentation

- Changeset: [2026-09-15-warm-code-intelligence-daemon.md](../../dev/1-WIP/2026-09-15-warm-code-intelligence-daemon.md)
- Initial discovery: [2026-09-15-warm-code-intelligence-daemon-initial-discovery.md](../../dev/1-WIP/2026-09-15-warm-code-intelligence-daemon-initial-discovery.md)
- Generated tonic adapters: [`packages/tddy-codegen/docs/tonic-adapter.md`](../../../packages/tddy-codegen/docs/tonic-adapter.md)
- The dual-transport worked example: [`packages/tddy-terminal-rpc/docs/terminal-session-service.md`](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md)
- Backlog items this change runs into — see the changeset's `## Prerequisites`
