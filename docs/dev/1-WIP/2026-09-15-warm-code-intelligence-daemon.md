# Changeset: Warm Code-Intelligence Daemon

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

[2026-09-15-warm-code-intelligence-daemon-initial-discovery.md](./2026-09-15-warm-code-intelligence-daemon-initial-discovery.md)
— four exploration passes: parent-driven orientation, the `docs/dev/todo/` cross-check, the
`tddy-lsp` lifecycle, the restructure/analysis CLI architecture, and the gRPC + stdio house pattern.

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ⛔ BLOCKING — `--indexing-budget` is not honoured — [`2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md`](../todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md)

The entry's § 2 records `--indexing-budget 900` indexing for ~20 minutes, reaching `working (100%)`,
then failing with *"rust-analyzer had not finished indexing after 46s"*.

Discovery found the mechanism. The budget **is** plumbed — into `warmup`. But
`resolution_budget()` (`backends/rust.rs:2043`) returns `settle` the moment `indexed` flips true,
and `settle = settle_budget_for(warmup) = max(30s, warmup / 20)` (`rust.rs:476`) — 45 s for a 900 s
budget. The refusal is that ceiling, not the budget.

**Every route around it is wrong for this change.** In a warm daemon `indexed` is true from request
#2 onward — that is the entire feature — so *every* request runs on the settle path. A daemon built
over this code turns an occasional CLI failure into a universal one. Raising the ratio would only
move the cliff; the code's own comment at `rust.rs:463` already says the premise behind the ratio is
false at this workspace's scale.

Fixed here, in `## Scope` and milestone 2: the cascade is deleted and a `CancellationToken`
replaces it. The entry's § 1 (a cluster of mutually entangled modules cannot be expressed
one-module-at-a-time) is **not** addressed and the entry therefore stays.

### ⚠ DURING — `plan is malformed` is the wrong error class — [`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md)

From § `#unbundle` node 6:

> **`plan is malformed` is the wrong error class.** The plan was not malformed; the indexer did not
> settle. A caller cannot distinguish a genuine schema problem from an indexing timeout, so the
> advice "fix your plan" is actively misleading.

A daemon makes this load-bearing rather than cosmetic: the error class decides the gRPC status, and
therefore whether a client retries. Milestone 2 adds `ServerNotSettled` and maps every
`RestructureError` variant to a status. The entry's other findings — the repo-scoped journal, the
self-dependency on a sibling move, the facade-cycle refusal, the cosmetic `pub use` duplication,
and `check --deep` disagreeing with `apply` — are untouched, so the entry stays.

### ⚠ DURING — the journal is repo-scoped, not plan-scoped — [`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md)

> **The journal is repo-scoped, not plan-scoped** (`.restructure/journal.jsonl`). A *completed* plan
> blocks the next one with *"a journal already exists for this plan — pass `--resume`"*, and
> `--resume` would resume the wrong plan.

`open_run` (`runner.rs:771`) is the only concurrency gate that exists, and there is **no lock file
and no plan identity in the path**. One process serving several clients can now reach one root
concurrently, which the CLI never could. **Recorded, not fixed here** — re-keying the journal is a
change to the on-disk state format with its own migration question, and it would bury this diff.
What this change does instead is refuse to make it worse: the daemon serializes requests per
workspace root, so two clients on one root queue rather than collide, and `JournalExists` still
surfaces as `FailedPrecondition` rather than being silently worked around.

### ⚠ DURING — `generate_tonic_adapter` hardcodes its status-conversion path — [`2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md`](../todo/2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md)

The generator emits handler bodies calling `tddy_service::to_tonic_status`, which is *"deliberately
not configurable"*. It only bites a crate emitting adapters into `tddy-service`'s own `OUT_DIR`.
`tddy-index-daemon` is a separate crate, so it takes a `tddy-service` dependency for the conversion
pair and the generator works as designed. **Recorded so the dependency is not mistaken for
accidental coupling**; nothing to fix.

### ⚠ DURING — the protos have no versioned package names — [`2026-09-09-versioned-proto-package-names.md`](../todo/2026-09-09-versioned-proto-package-names.md)

> Worth doing as its own change, across all protos at once, after the `#unbundle` stack lands — **a
> partial rename is worse than none, because it makes the convention unreadable.**

This change adds a proto and is therefore tempted to start the convention. It does **not**:
`code_index.proto` declares a bare `package code_index;`, matching every other crate-owned proto.
Introducing `tddy.code_index.v1` for one new service is precisely the partial adoption the entry
forbids. Recorded so the choice reads as deliberate rather than careless.

### ⚠ DURING — no RPC call has a client-side deadline — [`2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md)

> A deadline on `RpcClient` would cover both. It is a policy decision affecting every LiveKit RPC in
> the repo — including long-lived streams, which must not inherit a unary timeout — so it needs its
> own change rather than riding along with a feature.

Directly in this change's path, because removing the indexing budgets means the *caller's* bound is
the only one left. This change supplies the **server** half only — a handler that stops when its
caller is gone, via `CancellationToken` — and adds no client-side deadline policy to
`tddy_rpc::ClientEngine`. gRPC callers get tonic's `grpc-timeout`; a stdio caller's closed pipe
cancels its in-flight requests. The entry stays open for the client half.

### ⚠ DURING — `./test` reports green on a red suite — [`2026-09-09-the-test-script-reports-green-on-a-red-suite.md`](../todo/2026-09-09-the-test-script-reports-green-on-a-red-suite.md)

> **The exit code is `tail`'s, not cargo's.** … Anything trusting that status — an agent, a hook, a
> CI step — reads a red suite as passing.

Constrains how this change is verified, not what it builds: every claim of green in this changeset
quotes the **content** of `.verify-result.txt`, never a `./test` exit status. **Recorded, not fixed
here** — the entry says the fix is a root-script change with its own blast radius and belongs in its
own PR.

### ⚠ DURING — `execute-tool-stdio-fixture` forces three dev-dependencies into `[dependencies]` — [`2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md`](../todo/2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md)

`tddy-tools` declares a `[[bin]]` whose only purpose is to be a test fixture, and because plain
`cargo build` builds it, three dependencies sit in `[dependencies]` for a test. This change adds a
**real** binary with acceptance tests that spawn it. Constraint: `tddy-index-daemon`'s own binary is
genuine product, so tests spawn it via `CARGO_BIN_EXE_tddy-index-daemon` and **no fixture `[[bin]]`
is added to any crate**. Recorded so the same debt is not re-created. Not fixed here.

### ⚠ DURING — the daemon orphans its sandbox children on shutdown — [`2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)

Raised by this change's own discovery while looking for a child-lifecycle pattern to copy.
`main.rs:145-177` kills only `cli_sessions`, so a sandboxed runner survives the daemon; and
`dial_and_bridge` discards the relay's `JoinHandle` (`sandbox_session.rs:418-426`), so nothing
watches a runner for death.

**The constraint on this change is that it must not copy that path.** Milestone 6 follows
`LspServerBody` instead (`server_body.rs:148-173`), which selects on `child.wait()` so an exit is
observed rather than discovered, registers the child pid for the `TaskRegistry` escalation, and
performs a bounded graceful shutdown before killing — and wires an explicit shutdown hook rather
than inheriting `main.rs`'s. **Recorded, not fixed here**: repairing the sandbox path means touching
session lifecycle and shutdown ordering, which an unrelated feature has no business rewriting.

### ℹ ANSWERED — `move_module_to_crate` ignores `--indexing-budget` — [`2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md`](../todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md)

The entry asks why a flag *"accepted on the command line"* is *"then measured against what looks
like a fixed 46-second ceiling"*. Answer: it is not a fixed ceiling but a derived one,
`settle_budget_for(warmup) = max(30s, warmup/20)` (`rust.rs:476`), reached via `resolution_budget()`
once `ensure_indexed` sets `indexed` (`rust.rs:2023`). The entry's § 2 is closed by this change; its
§ 1 is not, so the file stays and § 2 is edited to record the answer rather than deleted.

### ✅ RESOLVED HERE — restructure progress cannot reach a non-CLI front end

Not a `docs/dev/todo/` entry — recorded here because the source analysis treats it as outstanding
work and the code already names it. `backends/rust.rs:529` documents the gap:

> It stays a sink rather than a `println!` because this library is not the only possible front end:
> anything that speaks a protocol on stdout, a persistent server most obviously, would have its
> stream corrupted by an engine writing progress into it.

Closed by milestone 3 (a boxed sink replacing `progress: fn(&str)`) plus milestone 4 (streaming
RPCs carrying it). No backlog file to delete.

## Affected Packages

- **tddy-index-daemon** (new): [README.md](../../packages/tddy-index-daemon/README.md) — the crate,
  its proto, its service and its binary
  - [docs/code-index-service.md](../../packages/tddy-index-daemon/docs/code-index-service.md) — the
    service contract, the two lifetimes, the warm-state model
- **tddy-lsp**: [README.md](../../packages/tddy-lsp/README.md) — document sync with per-URI
  versions; idle refresh on client use; single-spawn under concurrency; drained server `stderr`
- **tddy-code-restructuring**:
  [README.md](../../packages/tddy-code-restructuring/README.md) — cancellation replaces the budget
  cascade; a workspace root on every entry point; a boxed progress sink; new error variants; the
  apply-loop helpers promoted
- **tddy-code-analysis**: [README.md](../../packages/tddy-code-analysis/README.md) — a
  content-addressed complexity cache behind a trait
- **tddy-tools**: [README.md](../../packages/tddy-tools/README.md) — client mode behind
  `TDDY_INDEX_SOCKET`; `--indexing-budget` withdrawn from the `restructure` subcommands
- **tddy-daemon**: [README.md](../../packages/tddy-daemon/README.md) — lazy spawn, supervision,
  restart and idle stop of the index daemon

## Related Feature Documentation

- [PRD: Warm code-intelligence daemon](../../ft/coder/1-WIP/PRD-2026-09-15-warm-code-intelligence-daemon.md)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md)
- [Rust code analysis](../../ft/coder/rust-code-analysis.md)
- [Reusable LSP](../../ft/coder/reusable-lsp.md)
- [RPC multi-transport](../../ft/coder/rpc-multi-transport.md)
- [gRPC remote control](../../ft/coder/grpc-remote-control.md)

## Summary

Add `tddy-index-daemon`: one binary that holds a warm rust-analyzer index per workspace root and
serves the restructure and analyze operations over gRPC and stdio, or runs a single operation
in-process and exits. Fix the six defects in `tddy-lsp` and `tddy-code-restructuring` that are
harmless in a process that handles one request and structural in one that does not, and replace the
indexing budgets with cancellation.

## Background

`tddy-tools restructure apply` pays a full rust-analyzer cold start — six to ten minutes on this
workspace — on every invocation, because `restructure_cli.rs:97-131` builds a `TaskRegistry` and an
`LspRegistry` as **local variables** and drops both when the process exits. An iterative carve is a
sequence of plan-fix-retry cycles, so it pays that cost once per retry.

The warm-server machinery already exists: `LspRegistry` is keyed by `(workspace root, language)`
with idle reaping, crash respawn and `shutdown_all`, and `tddy_lsp_executor::register` is the
process-global host pattern that `tddy-daemon` and `tddy-sandbox-app` already run. What is missing
is a host for the **library** path, and the fixes that make that registry correct once a process
serves more than one request.

The dual-transport machinery exists too. `packages/tddy-terminal-rpc` is the worked example of one
crate owning, serving and dual-transporting its own proto, and `tddy-codegen` generates both the
`tddy-rpc` server and the tonic adapter from one `.proto`. The generated service trait takes and
returns prost structs inside `tddy_rpc::Request`/`Response`, which is what lets single-shot mode
call the same implementation in-process with no serialization.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **Blocking prerequisite**: delete the budget cascade and replace it with cancellation
      (`docs/dev/todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md` § 2)
- [~] **Package Documentation**: the three feature docs and the agent skill are updated; **package
      READMEs and `packages/*/docs/` are `/wrap-context-docs`'s to write** — `CLAUDE.md` forbids
      editing them directly. The three feature docs;
      the `code-restructuring` agent skill
- [x] **Implementation**: nine milestones (M4 split into 4a/4b) across seven packages plus root
      scripts, `install`, `release` and `publish.sh`
- [x] **Testing**: 582 tests green against `fake_lsp`, plus a 3-test `#[ignore]` production tier over
      a real rust-analyzer that measures the reuse claim (~2,500× on one root). The real-index claims covered by
      `#[ignore]` production tests
- [x] **Integration**: `tddy-daemon` inherits the registry fixes without behaviour change (82 + 27
      tests green); an unset `TDDY_INDEX_SOCKET` is pinned to the unchanged cold path. CI
      unaffected with `TDDY_INDEX_SOCKET` unset
- [~] **Technical Debt**: no mock code, no test branches, no fallbacks, no stubs, no new
      dependencies. Five gaps deferred with written reasons and owning packages
- [x] **Code Quality**: `cargo fmt --all --check` clean; `clippy --all-targets -- -D warnings` clean
      on all seven packages; scoped tests green per package

## Technical Changes

### State A (Current)

**Every CLI run is cold.** `restructure_cli.rs:97-131` constructs
`LspRegistry::new(restructure_allow_list(), TaskRegistry::new(), 600s)` per invocation; both are
locals. `LspKey` is `(std::env::current_dir(), Rust)`. `RustBackend::indexed` (`rust.rs:544`) is
per-backend and a fresh backend is built per run (`runner.rs:214`), so the warm-up probe is re-paid
even against a warm server.

**Budgets bound every wait.** `WARMUP_BUDGET = 600s`, `SETTLE_BUDGET = 30s`,
`settle_budget_for(w) = max(30s, w/20)` (`rust.rs:455-477`). `resolution_budget()` (`:2043`) returns
`warmup` while `indexed` is false and `settle` afterwards. `ensure_indexed` (`:2002-2035`) polls
`textDocument/hover` every 2 s until non-null or `chatter.quiescent`, then sets `indexed = true`.
`request_settled` (`:860-871`) retries `ContentModified` 30 × 200 ms, and `lsp_bridge.rs:66` maps
`LspError::Timeout` into the same retry, so a whole request timeout consumes one retry.

**Nothing takes a workspace root.** `std::env::current_dir()` at `runner.rs:226`, `:321`, `:350`,
`:419`, `:454` and `restructure_cli.rs:101`.

**Progress is a bare function pointer that prints.** `report_progress` `println!`s and
`report_progress_aside` `eprintln!`s (`runner.rs:846-852`); being `fn` pointers they cannot capture
per-request state. `RESTRUCTURE_TRACE` is read once at registry construction (`:215`).

**`LspClient` cannot track document versions.** Only `did_open`, hard-coded to `"version": 1`
(`client.rs:191`); no per-URI counter, no open-document set. The working `did_change` lives in
`RustBackend` (`rust.rs:941-953`) and dies with the backend.

**`LspRegistry` assumes a short-lived host.** The idle tracker is refreshed only by `get_or_spawn`
(`registry.rs:88`), never by client use; `get_or_spawn` releases the map lock before spawning
(`:95`), so concurrent cold requests double-spawn and orphan a task; `LspServerBody` pipes `stderr`
and never reads it (`server_body.rs:61`).

**`.restructure/` is keyed by root**, no lock, no plan identity (`runner.rs:797-820`); `open_run`
(`:771`) refuses a second plan with `JournalExists`. `StatePaths`, `open_run`, `restore_ledger`,
`commit_operation` and `Rehearsal` are private.

**Refusals already exit non-zero** — `main() -> Result<()>` with `?` (`tddy-tools/src/main.rs:157`);
there is no `process::exit` in the crate. The defect is the error *class*, not the code.

**`tddy-code-analysis` is synchronous, LSP-free and cacheless.** Every entry point takes explicit
paths; `capture_coverage` already takes `&mut dyn FnMut(CaptureProgress)`;
`complexity::file_complexity(source)` is pure over a `&str`. No `tests/` directory exists.

**Neither library is exposed over RPC.** `tddy-tools` dispatches both as CLI subcommands
(`main.rs:155-158`); neither appears in the MCP catalog.

### State B (Target)

**One process, many warm workspace roots.** `tddy-index-daemon` holds an `LspRegistry` plus a warm
`BackendRegistry` per root. Every request carries `workspace_root`; `TDDY_INDEX_SOCKET` names one
endpoint and the root routes within it. `tddy-daemon` lazily starts the process, supervises it,
restarts it on crash and stops it when every root is idle; `run-index-daemon` starts the same binary
by hand.

**Two lifetimes, one implementation.** No transport argument → run the operation in-process against
the generated service trait, render to stderr, exit with a status. `--grpc` and/or `--stdio` →
serve, concurrently if both. Neither a subcommand nor a transport is an error, not a default.

**Waits end on readiness or cancellation, never on a guessed budget.** The cascade is deleted; a
`CancellationToken` threads through `ensure_indexed`, `wait_until_resolved` and the settle retry, and
is checked **inside** the blocking loops rather than relied on at an await point — see
`## Decisions & trade-offs` § *How cancellation actually reaches the work*.

Each long request runs as a `TaskBody` in the daemon's `TaskRegistry`, so cancellation is
`cancel_task`, which already carries the SIGTERM→SIGKILL net (`tddy-task/src/registry.rs:289-329`).
The daemon cancels on three signals: the response stream's receiver dropping (a disconnected
client), its own shutdown, and an idle reap.

**Errors map to statuses.** `MalformedPlan`/`CodeTextInPlan`/`UnsupportedOp`/`NoBackend` →
`InvalidArgument`; `SnapshotMismatch`/`AnchorInvalidated`/`JournalExists`/`CheckpointDivergence`/
`IndeterminateJournal`/`NotAGitWorktree` → `FailedPrecondition`;
`IndexingIncomplete`/`ServerNotSettled` → `DeadlineExceeded`; `ServerCatchingUp` → `Unavailable`;
`Io` → `Internal`.

**Progress streams.** `Apply`, `Check`, `Coverage` and `DuplicateTests` are server-streaming; the
library writes to an injected sink and never to stdout.

**`tddy-tools` is unchanged unless opted in.** With `TDDY_INDEX_SOCKET` unset or empty, the current
cold path runs exactly as today.

### Delta (What's Changing)

#### tddy-lsp

- **API**: `LspClient::did_change(uri, text)`, `did_close(uri)`; per-URI version state and an
  open-document set held by the client, replacing `RustBackend::doc_version`.
- **Implementation**: `record_activity` on client use so a long request cannot be reaped;
  an in-flight placeholder in `get_or_spawn` so concurrent cold requests spawn one server;
  `LspServerBody` drains `stderr` to the log instead of letting the pipe fill.
- **API**: request waits accept a `CancellationToken` alongside the existing timeout.

#### tddy-code-restructuring

- **Architecture**: `WARMUP_BUDGET`, `SETTLE_BUDGET`, `settle_budget_for`, `resolution_budget`, the
  `warmup`/`settle` fields and `with_indexing_budget` deleted; a `CancellationToken` threaded
  through the three wait loops (`rust.rs:980`, `:2058`, `:2086`) and checked beside each
  `std::thread::sleep`, because these loops are synchronous and dropping the calling future does
  not stop them.
- **Dependencies**: `tokio-util` added to this crate's manifest. It is **not** a workspace
  dependency — it is declared per-crate, at `0.7`, by eight crates already (`tddy-task:17`,
  `tddy-vm:11`, `tddy-actions:21`, `tddy-daemon-sandbox:19`, and four with `features = ["compat"]`)
  — so this is a new manifest line against a version already in `Cargo.lock`, not a new third-party
  crate in the workspace.
- **API**: every entry point takes a workspace root; `Options` gains `root`; the seven
  `current_dir()` calls go. `progress` becomes a boxed sink (`Arc<dyn Fn(&str) + Send + Sync>`).
- **API**: `RestructureError::ServerNotSettled { method, seconds, last }`; `IndexingIncomplete`
  repurposed as "cancelled, and here is how far the index got".
- **API**: `StatePaths`, `open_run`, `restore_ledger`, `commit_operation` promoted so a host can
  drive the apply loop without re-deriving the write-ahead sequence.
- **CLI**: `--indexing-budget` withdrawn from `apply`, `check` and `anchors`.

#### tddy-code-analysis

- **API**: a `ComplexityCache` trait keyed by content hash, with an in-memory implementation the
  daemon owns and a pass-through the CLI uses.
- **Dependencies**: none added.

#### tddy-index-daemon (new)

- **Proto**: `proto/code_index.proto`, `package code_index;`, `service CodeIndexService` — `Warm`,
  `Check`, `Apply`, `Anchors`, `PlanStatus`, `Verify`, `Coverage`, `Report`, `DuplicateTests`,
  `Workspaces`. `Apply`/`Check`/`Coverage`/`DuplicateTests` are server-streaming.
- **Codegen**: the `tddy-terminal-rpc` two-pass `build.rs` — `prost_build` with
  `TddyServiceGenerator { generate_rpc_server: true, generate_tonic_adapter: true, rpc_crate_path:
  "tddy_rpc", tonic_trait_path: Some("crate::proto::tonic_code_index::code_index_service_server") }`,
  then `tonic_build` into `OUT_DIR/tonic_code_index` with `.extern_path(".code_index",
  "crate::proto::code_index")`, then a descriptor-set-only pass.
- **Architecture**: `CodeIndexPorts` (registry, workspace store, clock) in the
  `TerminalSessionPorts` shape; `WorkspaceIndex` holding the warm per-root state; per-root request
  serialization so two clients on one root queue rather than collide on `.restructure/`.
- **API**: `pub const CODE_INDEX_SERVICE: &str = "code_index.CodeIndexService";` and
  `pub fn build_code_index_entry(ports) -> tddy_rpc::ServiceEntry`.
- **Binary**: `clap` args — subcommands `restructure` / `analyze` for single-shot, flags `--grpc`,
  `--grpc-uds`, `--stdio`, `--config`; `stdio_safety` before `init_tddy_logger`;
  `select!(ctrl_c, SIGTERM)` shutdown; fail fast on neither subcommand nor transport.

#### tddy-tools

- **Integration**: `restructure` and `analyze` connect to `TDDY_INDEX_SOCKET` when set and non-empty,
  else the current path. Empty-as-unset, matching `LIVEKIT_TESTKIT_WS_URL`.
- **CLI**: `--indexing-budget` removed from the three subcommands that carried it.

#### tddy-daemon

- **Integration**: a get-or-spawn registry keyed by nothing more than "the index daemon", built in
  the `LspRegistry::get_or_spawn` shape (`tddy-lsp/src/registry.rs:70-144`) and wired beside
  `tasks.lsp_idle_reaper` (`runtime.rs:873-881`). The child is a `TaskBody` in the shape of
  `LspServerBody` (`server_body.rs:39-175`), which — unlike the sandbox path — detects child death
  by selecting on `child.wait()` and performs a bounded graceful shutdown before killing.
- **Integration**: an explicit shutdown hook in `main.rs`. The daemon today kills only
  `cli_sessions` (`main.rs:145-177`), so a sandboxed runner is orphaned when it exits; the index
  daemon must not inherit that.
- **Reuse, not reinvention**: `resolve_sandbox_runner_path`'s three-tier binary search
  (`tddy-daemon-sandbox/src/sandbox_session.rs:557-581`), `connect_uds_channel` for the
  gRPC-over-UDS client (`tddy-sandbox-runner/src/runner.rs:2610-2624`), the
  bind-then-write-the-marker readiness contract with a per-tick child-death check
  (`sandbox_session.rs:118-157`), and `terminate_sandbox_process` for process-group termination
  (`:687-706`).

#### Root

- `run-index-daemon` (new) — narration on stderr, one `export TDDY_INDEX_SOCKET=…` on stdout.
- `install` — `tddy-index-daemon` added to `INSTALLED_BINARIES`.
- `Cargo.toml` — the new workspace member; `packages/tddy-index-daemon/BUILD.yaml`.

## Implementation Milestones

- [x] **M1** `tddy-lsp`: document sync with per-URI versions, idle refresh on client use, single
      spawn under concurrency, drained `stderr` — **9/9 acceptance tests green**; 39 passing across
      the package's five test binaries, no regressions, clippy clean. Adds `LspError::DocumentNotOpen`, an `ActivityHook` the
      registry installs on the client, and a per-key spawn gate
- [x] **M2** `tddy-code-restructuring`: budget cascade deleted, cancellation threaded through the
      three wait loops, `ServerNotSettled` added ⛔ *blocking item* — **4/4 acceptance tests green**;
      294 lib tests (baseline 293, net +1 after replacing 8 with 9), clippy clean, `tddy-tools`
      still builds. The error→status mapping is defined in `## Technical Changes` but has no code
      until M4 gives it a transport to map onto
- [x] **M3** `tddy-code-restructuring`: workspace root on every entry point, boxed progress sink,
      apply-loop helpers promoted — **5/5 tests green** (3 were red on behaviour, 2 cover the sink
      type change); 294 lib + 4 cancellation held; exactly one `std::env::current_dir()` remains, in
      `restructure_cli.rs:98`, where a command line legitimately makes the process directory the
      root
- [x] **M4a** `tddy-index-daemon`: crate, proto, both codegen passes, the service over a warm
      per-root registry — **27 tests green** (13 unit + 14 acceptance), clippy clean. All seven RPCs
      implemented; the error→status mapping is one exhaustive `match` in `src/status.rs:20` with no
      catch-all arm; per-root serialization is an owned mutex guard held in the host, not a lock file
      in the library
- [x] **M4b** `tddy-index-daemon`: single-shot + `--stdio` + `--grpc`, and `src/main.rs`
      — **49 tests green**. `main.rs` split into `cli` / `serve` / `single_shot` / `render`;
      both transports share one `Arc` via `CodeIndexServiceServer::from_arc`; a UDS socket is
      unlinked by an RAII guard held across the serve await, because shutdown *aborts* the
      transport and anything after the await never runs
- [x] **M5** `tddy-tools` client mode behind `TDDY_INDEX_SOCKET`; `run-index-daemon`; `install`
      — **5 acceptance + 8 unit tests green**, the 3 existing CLI acceptance tests untouched.
      `./release`, `./install`, `./publish.sh` and the `AGENTS.md` script table all carry the new
      binary. One daemon per checkout, keyed by a checksum of the resolved root; socket under
      `$TMPDIR` because an AF_UNIX path is ~104 bytes and a repo-local one is 108 on this machine
- [x] **M6** `tddy-daemon`: lazy spawn, supervision, restart, idle stop — **11 tests green**;
      78 + 4 in `tddy-daemon-kernel` held. Config-gated by the presence of an `index_daemon:`
      section, copying the `supervisor:` precedent, so absent means off and every existing
      deployment is byte-identical
- [x] **M7** analyze RPCs and the content-hash complexity cache — **62 tests in
      `tddy-index-daemon`, 39 in `tddy-code-analysis`** (the crate's first `tests/` directory). Four
      RPCs added to the proto; `AnalysisError` mapped in the same exhaustive `match`; md5 for
      content addressing, so no dependency was added
- [x] **M8** documentation: the three feature docs are updated (`rust-code-restructuring.md`,
      `rust-code-analysis.md`, `reusable-lsp.md`), the PRD is linked from `1-OVERVIEW.md`, and the
      root-script table in `AGENTS.md` carries `./run-index-daemon`.
      **`packages/tddy-index-daemon/docs/` is deliberately not written here**: `CLAUDE.md` forbids
      modifying `packages/*/docs/` directly, so the package doc is `/wrap-context-docs`'s to create
      from this changeset. The `code-restructuring` agent skill still documents `--indexing-budget`
      and is the one outstanding doc item — see `## Technical Debt`

## Testing Plan

### Testing Strategy

**Primary test approach: integration tests against a fake language server, plus a small production
tier for the claims only a real index can prove.**

The feature's whole point is behaviour across *more than one request*, which no unit test reaches;
but the honest end-to-end version — boot rust-analyzer, index 1,692 crate units, apply, apply again
— is six to ten minutes per run. By [`docs/dev/guides/testing.md`](../guides/testing.md) § Production
Tests that is an `#[ignore]` test excluded from CI, not an acceptance gate.

`packages/tddy-lsp/tests/bin/fake_lsp.rs` is what makes the gate fast and deterministic: a
`[[bin]]` fake language server reached through `CARGO_BIN_EXE_fake_lsp`, already used by all six
`registry_reuse_test.rs` tests. It can be driven to be slow, to wedge, to report `$/progress`, and
to record the versions it was sent — which is exactly the set of behaviours this change turns on.

Transport coverage uses the two established Rust harnesses rather than anything new: in-process
dispatch at the literal registered coordinate
(`packages/tddy-terminal-rpc/tests/terminal_session_service_acceptance.rs`), and two
`StdioEndpoint::from_duplex` halves over `tokio::io::duplex`
(`packages/tddy-tools/tests/session_tool_streaming_dispatch.rs:64`).

### Testing Options Analysis

#### Option 1 (chosen): integration tests over `fake_lsp` + `tokio::io::duplex`, with a production tier

**Test level**: Integration
**Scope**: every behaviour this change adds — document-version continuity across requests, idle
refresh, single spawn under concurrency, cancellation replacing budgets, error→status mapping,
root routing, streaming progress, both transports concurrently, single-shot in-process dispatch,
client opt-in, daemon-managed lifecycle, complexity caching.

**Assertions**:
- [ ] The second `didChange` for a URI carries a strictly greater version than the first, **across
      two separate requests** — the exact defect `RustBackend`'s per-process counter causes
- [ ] Exactly one server task exists after N concurrent cold requests for one root
- [ ] A server sent a slow index is not reaped while a request is in flight
- [ ] A request against a deliberately slow fake completes rather than being refused
- [ ] A cancelled request returns `IndexingIncomplete` naming the furthest phase reached
- [ ] `handle_rpc` at `"code_index.CodeIndexService"` answers; a neighbouring coordinate returns
      `not_found`
- [ ] The stream carries ≥1 progress event before its terminal event
- [ ] Nothing is written to stdout while serving over stdio

**Reliability**: `fake_lsp` is deterministic and process-local; no network, no container, no
toolchain discovery. Per-test budget under the 1,000 ms integration ceiling except the two that
drive an artificial delay, which carry a comment stating the delay and why.

**Implementation location**: `packages/{tddy-lsp,tddy-code-restructuring,tddy-index-daemon,tddy-tools,tddy-daemon}/tests/`

#### Option 2 (rejected as the gate, kept as a tier): real rust-analyzer end to end

**Description**: index this workspace, apply a plan, apply a second plan, assert the second did not
re-index.

**Why not the gate**: six to ten minutes per run, needs a real toolchain and a warm `target/`, and
[`2026-09-09-keeping-target-from-overhogging-the-disk.md`](../todo/2026-09-09-keeping-target-from-overhogging-the-disk.md)
records `target/` reaching 37 GB under test load. It is the only way to prove "a second apply skips
the crate-graph load", so it is kept as `#[ignore]`, run via `cargo test -- --ignored`.

#### Option 3 (rejected): unit tests over the budget arithmetic

**Description**: keep testing `settle_budget_for` and friends at the function level.
**Why not**: the arithmetic is being deleted. Its ten existing tests are replaced by cancellation
tests rather than adapted, because the behaviour they pin is the behaviour being removed.

### Coverage Requirements

Every milestone lands with its acceptance tests in the same commit. No milestone is checked off
while its tests are absent, `#[ignore]`d, or passing for a reason other than the behaviour named.

## Acceptance Tests

### tddy-lsp — `packages/tddy-lsp/tests/warm_host_document_sync_test.rs` (new)

| Test | Validates |
|---|---|
| `a_second_edit_to_one_document_carries_a_greater_version` | Per-URI version state exists at all |
| `a_document_edited_across_two_requests_keeps_one_version_sequence` | The daemon defect: the counter no longer dies with a backend |
| `closing_a_document_forgets_its_version_so_a_reopen_starts_again` | `did_close` clears state; a reopened file is version 1 legitimately |
| `an_unopened_document_is_refused_rather_than_changed` | `did_change` before `did_open` is a caller error, not a silent no-op |

### tddy-lsp — `packages/tddy-lsp/tests/registry_reuse_test.rs` (extended)

| Test | Validates |
|---|---|
| `a_request_on_a_borrowed_client_keeps_its_server_alive_past_the_idle_timeout` | `record_activity` on client use — a long restructure is not reaped mid-flight |
| `concurrent_requests_for_one_cold_workspace_spawn_exactly_one_server` | The in-flight placeholder; asserts `tasks.list().len() == 1` |
| `a_server_that_writes_continuously_to_stderr_keeps_answering_requests` | The drained `stderr` pipe |

### tddy-code-restructuring — `packages/tddy-code-restructuring/tests/cancellation_acceptance.rs` (new)

| Test | Validates |
|---|---|
| `an_operation_waits_for_a_slow_index_instead_of_refusing_at_a_derived_budget` | The ⛔ blocking fix: no 45 s ceiling |
| `a_cancelled_operation_stops_waiting_and_reports_how_far_the_index_got` | Cancellation is the replacement, and `how_far()` survives as the useful half |
| `a_server_that_cannot_answer_a_method_is_refused_as_unsettled` | `ServerNotSettled` exists and is reached |
| `an_unsettled_server_is_not_reported_as_a_malformed_plan` | The TODO's error-class complaint, pinned |

### tddy-code-restructuring — `packages/tddy-code-restructuring/tests/workspace_root_acceptance.rs` (new)

| Test | Validates |
|---|---|
| `an_operation_resolves_against_the_root_it_was_given_not_the_process_directory` | The seven `current_dir()` calls are gone |
| `two_roots_in_one_process_keep_separate_journals` | `.restructure/` follows the parameter |
| `progress_reaches_the_sink_the_caller_installed` | The boxed sink replaces the `fn` pointer |
| `a_run_writes_nothing_to_standard_output` | The `rust.rs:529` invariant, now enforceable |

### tddy-index-daemon — `packages/tddy-index-daemon/tests/code_index_service_acceptance.rs` (new)

| Test | Validates |
|---|---|
| `names_the_service_the_wiring_layer_registers` | Coordinate trio 1 — literal string |
| `registers_at_the_coordinate_its_generated_server_answers_to` | Coordinate trio 2 — against `Server::NAME` |
| `publishes_the_coordinate_its_schema_declares` | Coordinate trio 3 — parsed from the `.proto` on disk |
| `applies_a_plan_and_streams_one_event_per_operation` | Streaming shape and per-op progress |
| `refuses_a_plan_carrying_source_text_as_an_invalid_argument` | Error→status mapping, `InvalidArgument` leg |
| `reports_an_indexing_cancellation_as_a_deadline_not_an_invalid_argument` | Error→status mapping, the leg the TODO is about |
| `warms_two_workspace_roots_independently` | Root routing; warming one does not warm the other |
| `queues_a_second_request_for_one_root_instead_of_colliding_on_its_journal` | The journal constraint is respected, not worsened |

### tddy-index-daemon — `packages/tddy-index-daemon/tests/dual_transport_acceptance.rs` (new)

| Test | Validates |
|---|---|
| `serves_one_warm_index_over_grpc_and_stdio_concurrently` | The headline dual-transport claim; modelled on `stdio_remote_control_acceptance.rs:141` |
| `runs_an_operation_in_process_when_no_transport_is_requested` | Single-shot calls the same trait with no serialization |
| `exits_non_zero_when_a_single_shot_plan_is_refused` | Pins the behaviour the source analysis claimed was broken |
| `refuses_to_start_with_neither_a_subcommand_nor_a_transport` | Fail fast, per the sandbox-runner rule |
| `writes_nothing_to_standard_output_while_serving_over_stdio` | `stdio_safety` is actually applied |
| `cancels_in_flight_work_when_a_streaming_client_disconnects` | The server half of the missing-deadline entry; the send-into-a-dropped-receiver signal |
| `stops_a_blocking_index_wait_rather_than_only_dropping_its_future` | The subtle half: `spawn_blocking` work is not stopped by a dropped future, so the token must be checked inside the loop |
| `cancels_every_in_flight_request_on_shutdown` | The index daemon is not orphaned the way a sandboxed runner is |

### tddy-tools — `packages/tddy-tools/tests/index_daemon_client_acceptance.rs` (new)

| Test | Validates |
|---|---|
| `restructure_uses_the_warm_daemon_when_the_socket_variable_is_set` | Client mode works |
| `restructure_spawns_its_own_language_server_when_the_socket_variable_is_unset` | CI is unaffected by construction |
| `an_empty_socket_variable_is_treated_as_unset` | Empty-as-unset, matching `LIVEKIT_TESTKIT_WS_URL` |

### tddy-daemon — `packages/tddy-daemon/tests/index_daemon_lifecycle_acceptance.rs` (new)

| Test | Validates |
|---|---|
| `starts_the_index_daemon_on_the_first_request_that_needs_it` | Lazy spawn |
| `restarts_the_index_daemon_after_it_exits` | Supervision |
| `stops_the_index_daemon_once_every_workspace_root_is_idle` | Idle stop, and that it cancels rather than orphans |

### tddy-code-analysis — `packages/tddy-code-analysis/tests/complexity_cache_acceptance.rs` (new — the crate has no `tests/` today)

| Test | Validates |
|---|---|
| `reuses_a_cached_complexity_result_for_unchanged_source` | The cache is reached |
| `recomputes_complexity_when_the_source_changes` | It is keyed by content, not by path |
| `a_pass_through_cache_computes_every_time` | The CLI keeps today's behaviour |

### Production tier — `packages/tddy-index-daemon/tests/warm_index_production.rs` (new, `#[ignore]`)

| Test | Validates |
|---|---|
| `a_second_apply_against_a_warm_index_does_not_reload_the_crate_graph` | The claim the whole change exists for. Real rust-analyzer; minutes; excluded from CI |

## Technical Debt & Production Readiness

### `did_open` on an already-open document still restarts its version sequence (M1)

`LspClient::did_open` records the URI at version 1 whether or not it was already open. That is
deliberate and load-bearing: `LspRegistry::bind_target` (`registry.rs:146-160`) re-opens every
`src` on **every** call, and `tddy-lsp-executor` calls it per tool invocation
(`tddy-lsp-executor/src/lib.rs:95-115`), so refusing a re-open would break that consumer's bind
path. It is also not a regression — the previous code always sent `"version": 1`.

But it is the same defect class this milestone set out to fix: on a warm host, a re-`did_open`
without a `did_close` restarts a sequence the server has already advanced. No test pins it either
way, because the honest fix is `bind_target` issuing `did_change` for a URI that is already open —
a behaviour change to a consumer's path, and therefore a decision rather than an implementation
detail. Raised by the M1 implementer rather than silently changed.

Resolve it in M4 or M5, when the daemon is the consumer that actually depends on version continuity
across requests, or record it in `docs/dev/todo/` at wrap if it is still open.

### ✅ The library is silent; results are values now (M4a → resolved)

**Resolved.** `runner.rs` had 23 `println!`; it now has none. Five entry points return what they used
to print — `apply → RunSummary`, `status → PlanProgress`, `check → Vec<Finding>`,
`anchors → Range`, `verify → Comparison` — and `dispatch` returns an `Outcome` enum the front end
renders. Printing lives in exactly one module, `restructure_cli.rs`, and
`only_the_command_line_front_end_writes_to_standard_output`
(`tests/library_returns_its_results.rs`) enforces that by reading the crate's own `src/` and
asserting the printing-module list is exactly `["restructure_cli.rs"]` — structural on purpose,
since no unit test can observe another module's stdout.

Three design consequences worth keeping:

- **Sinks ride on `Options`** (`progress`, `account`, `trace`), not in parameter lists, because the
  tests pin all five parameter lists. That turned out better than a parameter would have been:
  `serve_check` can now install a **per-request** progress sink, which was the half of the `Check`
  gap that was not about findings.
- **`check` no longer errors on findings.** It returns `Ok(findings)`; the CLI prints them and then
  returns an error, so `tddy-tools` still exits 1 — pinned by
  `tddy-tools/tests/restructure_cli_acceptance.rs`, which still passes. A caller that receives
  findings as values decides for itself what they mean, which is exactly what the daemon needs.
- **`Verify` over RPC improved.** A non-holding comparison used to be raised as an error by the
  library; it is now a successful `VerifyResponse` with `holds: false` and the statement lists
  filled, which is what the schema's `holds` field is for.

`restructure_cli.rs` reached 534 lines, so the command line's *shape* (clap types, clap → `Options`,
and their 8 unit tests) moved to a new `restructure_args.rs` (242 lines), leaving
`restructure_cli.rs` at 314. Note the split had to move **non-printing** code: a
`restructure_cli/report.rs` would have appeared in the printing list and broken the invariant above.

### ✅ `Check` and `VerifyResponse` are value-complete (M4a → resolved)

Both followed from the above. `serve_check` streams one `Finding` event per finding, attributed to
the operation that caused it, and `serve_verify` fills `before` / `after` / `missing` / `added` from
the returned `Comparison` instead of leaving them at proto defaults. `serve_plan_status` also stopped
reimplementing `runner::status`'s arithmetic — ~25 lines of `Plan::parse` + `Journal::load` +
`saturating_sub` collapsed into mapping four `usize`s, the duplication having existed only because
that function printed.

New coverage: `streams_a_finding_attributed_to_the_operation_that_caused_it`, and
`holds_a_tree_against_the_ref_it_was_committed_as` now asserts the whole `VerifyResponse` rather
than just `holds`.

### A mutation test found a defect no test could, and the fix was structural (M6)

Deleting the line that clears a dead predecessor's socket left **all 11 lifecycle tests green**: a
stale socket satisfied "the path exists", so a child that died on its first breath was reported as
serving.

The fix was not another test. `clear_the_socket_of_a_process_that_is_gone` now returns a
`#[must_use] struct SocketWasCleared` which `became_ready` takes as an **argument**, so the same
mutation becomes `error[E0425]: cannot find value 'cleared' in this scope`. The marker type's own
comment says why it is a type rather than a convention: *"the two steps are one fact split across
twenty lines, and the failure mode of separating them is silent."*

### Readiness is the socket appearing, not the child's log line (M6)

A deviation from the brief, and a better answer than the brief had. `UnixListener::bind` creates the
path and starts accepting in one call, so the file's existence is evidence of the one thing a caller
needs. Scraping the child's `listening on …` stderr line would instead depend on the child's
inherited `RUST_LOG` — `tddy_core::default_log_config` logs at `Info`, so a daemon running with
`RUST_LOG=warn` would make a stderr-scraping wait time out on a perfectly healthy child. The
`listening on` line is still drained into the log, and the child's last line is what a death message
quotes. This is also the contract this changeset already named: *"the bind-then-write-the-marker
readiness contract with a per-tick child-death check"*.

### ⚠ `IndexDaemonRegistry::connect` has no in-crate caller (M6, open)

M6 delivers the lifecycle; the request path that would *use* an index daemon from inside
`tddy-daemon` is not in this changeset. M5's client dials `TDDY_INDEX_SOCKET` directly, so the
daemon's job is to have the process running, not to proxy for it. `connect()` is therefore public API
with no caller and no acceptance test on its success leg — the only fixture that could bind a real
gRPC-over-UDS socket would be a fixture `[[bin]]`, which
[`2026-09-10-the-execute-tool-stdio-fixture-…`](../todo/2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md)
records as debt not to re-create. Its failure leg is tested, and the dialable path is covered from
both ends by M4b's transport suite and M5's client suite.

Either give it a caller — the daemon serving code intelligence to the web or to sessions — or drop it
until there is one.

### The registry is built only when the daemon has a user resolver (M6)

It is constructed inside `runtime.rs`'s `if let Some(user_resolver) = …` block, beside
`tasks.lsp_idle_reaper`, so a daemon with no user resolver assembles no index daemon — exactly as it
assembles no LSP executor, local socket or LiveKit service. Consistent with every neighbour, but it
is a second condition beyond `index_daemon:` being present, and worth knowing before someone debugs
why their configured section did nothing.

### ⚠ The console rendering is now restated three times (M5, open)

`restructure_cli`'s `report`, `report_findings`, `report_comparison` and `install_console` are all
**private**, and that module's only public entry point is `run(RestructureArgs)` — which owns the
whole run *including spawning the language server*, the one thing a client of the daemon must not do.
So there is no reachable renderer, and the same output shapes are now stated in three places:
`restructure_cli.rs` (the cold CLI), `tddy-tools/src/index_console.rs` (the warm client), and
`tddy-index-daemon/src/render.rs` (the binary's own single-shot console, `pub(crate)` in a binary
crate).

Mitigated, not solved: the `check --budget` acceptance test in `index_daemon_client_acceptance.rs`
asserts the **whole** stdout vector against the literal lines `restructure_cli_acceptance.rs` pins
for the cold path, so the two front ends drifting apart fails a test rather than going unnoticed.

Closing it means publishing a renderer in `tddy-code-restructuring` over `Outcome` / `Finding` /
`Comparison`. `TODO(tddy-code-restructuring)` at `tddy-tools/src/index_console.rs:15`. Two smaller
duplications ride along: items normalisation and the UTF-8 path refusal, each private in two crates.

### A production line was added because a test needed the behaviour observable (M5)

For every LSP-free operation the warm and cold paths produce byte-identical stdout — that *is* the
feature — so nothing on the console could tell a test which path ran. Rather than weaken the test,
the client now writes one line to **stderr**:
`restructure: running against the warm index daemon at <socket>`.

Recorded because it is behaviour introduced under test pressure, and that deserves a second look.
It reads as genuine product value: without it a developer cannot tell warm from cold, nor a daemon
serving *their* tree from one serving the wrong one. Both warm tests assert it; both cold tests
assert its absence.

### ⚠ A coverage capture cannot be stopped (M7, open)

The honest weakness in the analyze streaming story. `capture_coverage` takes no cancellation
surface, so a client that hangs up one minute into a 55-minute capture leaves the other 54 running.
The progress sink notices the dropped receiver and stops sending, but has no way to *cancel*.

Giving it one is an API change to the capture pipeline: a `CancellationToken` cannot cross the
boundary because `tddy-code-analysis` has no `tokio-util` and adding a dependency needs the
developer's consent. This changeset's `#### tddy-code-analysis` delta explicitly scoped the capture
pipeline out, so it was recorded rather than smuggled in. `DuplicateTests` has the same gap plus no
progress at all, because `analyze_coverage_dir` takes no sink either.

The restructure half does not share this problem — M2 threaded a token through its three wait loops.
The asymmetry is worth closing.

### ⚠ The in-memory complexity cache is unbounded (M7, open)

Keyed by content hash, so over a tree under active edit it grows with the number of *versions* of a
file, not the number of files. It needs an eviction policy and an idle-reap hook, and the daemon has
no reaping story for analysis state at all — only for language servers.

### ⚠ The binary has no `analyze` subcommand (M7, open)

`cli.rs` offers only `restructure`. The four analyze RPCs are reachable over gRPC and stdio but not
from this binary's own command line, so single-shot mode covers half the service. The changeset's
Binary bullet under M4b implies both. Small, and worth closing before the wrap.

### ⚠ `Warm.ready` is still weaker than the schema's word (M4a, open)

The one gap the refactor did **not** close, and deliberately out of its scope.
`Warm.ready` (`tddy-index-daemon/src/operations.rs:65`) means "a live language server holds this
root's index", not "the crate graph is loaded and queryable". The probe that decides it properly is
`RustBackend::ensure_indexed` — private, and URI-scoped — and the server's own `$/progress` is
reachable only through `LspClient::drain_notifications`, which **consumes** the queue a concurrent
operation on the same root is folding. Forwarding phases from there would steal them from an apply.

Closing it needs `drain_notifications` to become non-destructive (a broadcast or watch rather than a
drain), which is a real design change in `tddy-lsp` and not something to fold into a feature. It is
already recorded as the third item in
[`2026-09-15-rpcservice-has-no-cancellation-surface.md`](../todo/2026-09-15-rpcservice-has-no-cancellation-surface.md)'s
neighbourhood; give it its own entry at wrap if still open.

### My own test fixture was wrong, and the implementer proved it rather than working around it

`a_check_returns_the_findings_it_made` was written with a **symbol**-anchored `extract_module` and a
comment claiming it was "wrong in a way the static tier can see". False: `RustBackend::check`
(`backends/rust.rs:1096-1099`) opens with `let Anchor::Range { .. } = &op.anchor else { return
Ok(Vec::new()) }` — the static tier judges range anchors only, because both of its rules need the
seam's extent. Undeclared-symbol detection is `locate_symbol` → `textDocument/documentSymbol`, a
language-server answer reachable only through `check --deep`.

So no finding existed to collect, before or after the refactor. The implementer left the test
failing, said so, and proved the collection path worked by other means rather than inventing a
text-heuristic tier to satisfy a bad premise. Fixture corrected to a range anchor colliding with a
`mod grouped;` the file already binds — a name collision being the lexical fact a static tier can
actually see.

### An unclassified error is `Internal`, on purpose (M7)

`status_of_analysis` maps `AnalysisError::Message` to `Internal` rather than guessing a better code,
and the "no Cargo.toml at …" case that used to be a `Message` became its own `NotACrate` variant
mapping to `InvalidArgument`. That is this changeset's own complaint — *"`plan is malformed` is the
wrong error class … the advice 'fix your plan' is actively misleading"* — applied to the analysis
side: an unclassified refusal genuinely is one a caller cannot act on, and the fix is a variant, not
a cleverer default.

### `fake_lsp` cannot resolve an assist, so two planned tests are not yet deliverable (M4a)

The fixture's `codeAction` always answers `ContentModified`, by design — it models the "ask again"
failure mode. That means no plan can *succeed* against it, so
`applies_a_plan_and_streams_one_event_per_operation` and
`queues_a_second_request_for_one_root_instead_of_colliding_on_its_journal` (both named in
`## Acceptance Tests`) cannot be written yet. The implementer declined to claim them, which is the
right call.

What is covered instead: the terminal `RunOutcome` event (exact struct equality), the apply path
reaching the backend registry (`NoBackend` → `InvalidArgument`), and two unit tests on the sink —
one pinning that a progress line crosses from the `spawn_blocking` thread into the request's stream,
one pinning the dropped-receiver → cancellation path. The **per-operation** event leg stays uncovered
until a fixture can resolve one assist. Third fidelity limit found in this fixture; the first two are
recorded below.

### `StatePaths`' fields are public, so the journal layout is now public surface (M3)

Promoting `StatePaths`, `open_run`, `restore_ledger` and `commit_operation` is what makes "a host can
drive the apply loop without re-deriving the `.restructure/` paths" true — this changeset's own
wording, which the implementer followed. The cost is that `<root>/.restructure/journal.jsonl` is now
part of this crate's public API, so the future change that re-keys the journal to a plan identity
(recorded in
[`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`](../todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md))
becomes breaking for any host reading those fields.

The alternative is keeping the fields private and exposing only `StatePaths::under` plus
`open_run`/`restore_ledger`. That serves the apply loop but leaves a host unable to read the journal
for a status-style query — which M4's `PlanStatus` RPC needs. **Decide at M4**: if `PlanStatus` can
be served entirely through `runner::status`, the fields can go private again before anything depends
on them.

`open_run`'s new doc states the constraint rather than fixing it: it is the only concurrency gate
that exists, there is no lock file, `.restructure/` is keyed by root alone, and a host must therefore
serialize per root itself. No lock was added — deliberately, per this changeset's `## Prerequisites`.

### `fake_lsp` sent no `$/progress`, so a forwarding consumer had nothing to forward (M3)

The second fidelity gap in the same fixture, found the same way as the `positionEncoding` one. The
cold-hover mode answered `null` without reporting anything, so `ServerChatter` absorbed nothing and a
caller's sink came back empty — the fake being unfaithful, not the library being wrong: a real server
that cannot answer yet reports what it is doing instead. The mode now emits `begin` once and a rising
`report` per unanswered hover, matching the shape `ServerChatter::progress` folds (title on `begin`,
inherited by later reports).

Two fidelity gaps in one fixture, each found by a different milestone. Worth remembering before M4
writes a fake for the daemon: a fake that ignores a protocol its consumers depend on cannot be driven
past the point where they depend on it.

### `positionEncoding` was missing from `fake_lsp`, and the red tests hid it (M2)

`client_capabilities()` advertises `general.positionEncodings: ["utf-8"]` (`rust.rs:181`) and
`refuse_foreign_encoding` (`rust.rs:2293`) refuses any other negotiated encoding — a real
rust-analyzer honours the request, and a client that counts bytes against utf-16 columns would be
wrong on every line carrying a character outside the BMP.

`fake_lsp` advertised no `positionEncoding` at all, so the handshake settled on the LSP default of
utf-16 and `RustBackend::start()` refused before any wait was reached. The red-phase tests did not
expose it, because `with_cancellation`'s `unimplemented!()` panicked during construction — earlier
than `start()`. The red signal was therefore correct but shallower than it looked: three of the four
tests would have failed at the handshake the moment cancellation was implemented.

The M2 implementer diagnosed it, declined to weaken the encoding refusal, declined to edit
`tddy-lsp` while another agent held it, and proved the fix against a patched copy out of tree.
Fixture fixed in `initialize_result()`. Worth remembering as a fixture-fidelity lesson: a fake that
ignores a negotiation its consumers depend on cannot be driven past a handshake by those consumers.

### Two cancellation tests overlap (M2)

`a_cancelled_wait_stops_and_reports_how_far_the_index_got` and
`cancelling_stops_the_blocking_wait_rather_than_only_dropping_its_future` both cancel a wait and
assert it returns; they differ only in whether they assert the error shape or the elapsed time. The
second was passing for the wrong reason before the fixture fix (the handshake refusal also returned
fast), which is how the overlap came to light. Both are meaningful now, but `/validate-tests` should
decide whether the timing assertion earns its own test or belongs as a second assertion on the
first.

### stderr is drained in chunks, so a long line can span two log records (M1)

`LspServerBody` drains the server's stderr in 8 KiB chunks and logs each at `debug`, mirroring the
stdout drain. A line reader would read better in the log but buffers without bound on a server that
writes megabytes with no newline; the chunked form is bounded by construction. Accepted.

## Decisions & Trade-offs

**A separate process, not a service inside `tddy-daemon`.** `reusable-lsp.md` requirement 14 makes
`tddy-daemon` the owner of the `LspRegistry`. This change splits ownership of the *index* from
ownership of the *lifecycle*: the index lives in its own crash domain and its own memory, and
`tddy-daemon` decides when it runs. The deciding constraint is the handshake — `tddy-daemon`
registers `LspAllowList::rust_only()`, which advertises no client capabilities, and rust-analyzer
returns no code actions and no `$/progress` to such a client. The handshake is fixed at spawn, so
one registry entry cannot serve both consumers.

**One process for many worktrees.** `LspRegistry` is already a map keyed by `(root, language)`, so
multi-root is what it does; restricting to one worktree would add a limitation and then
re-implement per process what the registry provides. Memory is unaffected (rust-analyzer is a child
either way) and crash isolation is already handled by the terminal-task respawn at
`registry.rs:78-93`. Accepted cost: **a host restart loses every warm index at once**, roughly seven
minutes per root. Bounded by lazy re-warming and by the option of pointing a worktree at its own
socket, which needs no new code.

**How cancellation actually reaches the work.** Discovery found that `tddy_rpc::RpcService` has
**no cancellation surface at all** — no token, no deadline, no context (`tddy-rpc/src/bridge.rs:33-69`).
`ServerEngine::on_peer_disconnected` does abort a peer's in-flight forwards
(`server_engine.rs:212-235`), but two gaps make that insufficient here: it is not in the tonic or
Connect-HTTP path at all (`local_socket_server.rs` serves generated tonic servers directly), and
aborting a future only drops it at an await point — **the restructure backend is synchronous and
runs inside `spawn_blocking` with `std::thread::sleep` loops, which a dropped future does not
stop.**

So cancellation is explicit and checked inside those loops, and the token comes from a `TaskBody` in
the `TaskRegistry` — the established route, which already has the SIGTERM→SIGKILL escalation. The
streaming design earns its keep twice over here: a unary handler has no back-channel, so **a send
failing into the response stream's dropped receiver is what tells the daemon its client is gone**.

Rejected: adding a cancellation parameter to `RpcService`, which is a cross-cutting change to a
trait with many implementors and no precedent; and a handler-level `tokio::time::timeout`, which
would reintroduce the guessed deadline this change exists to remove. Recorded for the trait-level
option: [`2026-09-15-rpcservice-has-no-cancellation-surface.md`](../todo/2026-09-15-rpcservice-has-no-cancellation-surface.md).

**Managed by the daemon, not declared to the supervisor.** `tddy-supervisor`'s `ManagedService`
(`supervisor/src/config.rs:85-125`) would supply real backoff, restart and reap policy for free, but
its `services:` list is eager and fixed at config load, the supervisor is optional (absent in dev,
in the Tauri desktop host and in `tddy-sandbox-app`), and its declaration file is described in
`supervisor.yaml.production` as *"the ENTIRE privilege surface of the host"*. A lazily-started
developer-facing helper does not belong there. Documented non-goal, exactly as the sandbox runner
treats it.

**Cancellation instead of budgets.** A server should not invent a deadline its caller never stated.
The three wait loops end on readiness or on the caller going away. This is deliberately only the
*server* half of
[`2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md),
which states that a client-side policy must not ride along with a feature. Accepted cost:
`--indexing-budget` is withdrawn — a **breaking CLI change**, the only one here.

**Opt-in client wiring.** `TDDY_INDEX_SOCKET` unset means today's behaviour, so CI cannot be
affected by construction. Rejected: auto-spawn from the CLI, which gives better ergonomics at the
price of implicit process lifetime and a much harder test story.

**rust-analyzer's own file watching.** No `notify` dependency is added. The workspace has none, and
`tddy-core/src/usage_watcher.rs:14` records a deliberate decision to poll rather than take one.
`LspClient` already auto-acknowledges `client/registerCapability` (`client.rs:470-473`), so the
server registers and drives its own watching. Accepted cost: staleness is rust-analyzer's semantics
and is not directly observable from the daemon.

**A bare proto package.** `package code_index;`, not `tddy.code_index.v1`, per
[`2026-09-09-versioned-proto-package-names.md`](../todo/2026-09-09-versioned-proto-package-names.md).

**Landed as one PR in milestones**, not a stack. The milestones are ordered so each leaves the tree
green and each earlier one is independently valuable; reviewability comes from the commit
boundaries rather than from separate PRs.

**Two premises from the source analysis are not carried forward.** `restructure apply` already exits
1 on refusal (`tddy-tools/src/main.rs:157`); there is no `process::exit` in the crate. And the
"apply observability WIP" it treats as landed is not in this worktree — `git status` is clean.
Per-request progress is not a separate deliverable here: it falls out of the streaming RPCs, because
a stream is the only place progress can go once `println!` is off the table.

## Refactoring Needed

### From the green pass

- [ ] `SIGINT` handling in `restructure_cli.rs` replaces the signal's default disposition, so a run
      wedged inside a single in-flight request cannot be `^C`d again until that request's liveness
      bound expires. `SIGTERM` is deliberately untouched. A second-`^C`-hard-exit would mean
      `process::exit` in a library and new CLI surface, so it was not added — decide in M8 whether
      the CLI should own that.
- [ ] `ONE_REQUEST_LIVENESS` (600s) exists because `Handle::current().block_on` in
      `backends/lsp_bridge.rs:25` is not cancellation-aware: while one request is in flight no
      in-loop cancellation check runs, so a finite per-request bound is what returns control to the
      retry loop. If the bridge ever becomes cancellation-aware, this bound can go.
- [ ] `packages/tddy-code-restructuring/README.md`, `docs/ft/coder/rust-code-restructuring.md` § CLI
      and the `code-restructuring` agent skill all still document `--indexing-budget`. Assigned to
      M8; package docs are changeset-gated, so they were deliberately not edited in M2.
- [ ] `docs/dev/todo/2026-09-10-…entangled-cluster.md` § 2 and
      `2026-09-09-restructure-defects-from-the-first-cross-crate-move.md` § node 6 are now answered
      by code but not yet edited to say so. Also M8.

### From the acceptance-test pass

- [ ] `a_warm_shared_client()` (`tddy-lsp/tests/warm_host_document_sync_test.rs`) and
      `a_backend_over_a_server_that_never_finishes_indexing()`
      (`tddy-code-restructuring/tests/cancellation_acceptance.rs`) build the same
      allow-list-then-registry-then-client chain. A shared fixture would need a test-only crate,
      which is more structure than two call sites earn — revisit if a third appears.
- [ ] `a_server_that_will_not_settle_is_refused_as_unsettled_not_as_a_malformed_plan` builds its
      allow-list inline because it needs a *different* launch spec from the suite helper's.
      Extracting `a_backend_over(spec)` would let both share one chain.
- [ ] `fake_lsp` is now declared as a `[[bin]]` in three manifests (`tddy-lsp`,
      `tddy-lsp-executor`, `tddy-code-restructuring`), each with its own comment saying why. A
      fourth consumer is the point at which it should become a real test-fixture crate instead.
- [ ] The `--cold-hovers` mode is driven by setting `LaunchSpec.args` directly, since `LaunchSpec`
      has builders for capabilities and options but not for args or env. A `with_args` builder
      would make the three call sites read the same way.

### From /validate-changes

*(populated during validation)*

### From /validate-tests

*(populated during validation)*

### From /validate-prod-ready

*(populated during validation)*

## Validation Results

### /validate-changes

**Risk summary: 0 critical · 2 warning · 3 info.** Ordinary branch, base `master`; all work is
uncommitted working tree, so the diff is `git diff master` plus untracked files.

| Package | Build | Tests | Clippy `-D warnings` |
|---|---|---|---|
| `tddy-lsp` | ✅ | 39 | ✅ clean |
| `tddy-code-restructuring` | ✅ | 308 | ✅ clean |
| `tddy-code-analysis` | ✅ | 39 | ✅ clean |
| `tddy-index-daemon` | ✅ | 62 | ✅ clean |
| `tddy-daemon` | ✅ | 27 *(scoped)* | ✅ clean |
| `tddy-daemon-kernel` | ✅ | 82 | ✅ clean |
| `tddy-tools` | ✅ | 25 *(scoped)* | ✅ clean |

`cargo fmt --all -- --check` clean. `bash -n` clean on `run-index-daemon`, `install`, `release`,
`publish.sh`. `tddy-daemon` and `tddy-tools` are scoped runs — their full suites are long and carry
pre-existing failures, so whole-workspace green is CI's answer, not claimed here.

**What the risk sweep found clean:**

- **No test-environment branches** anywhere in new production code (`cfg!(test)`, `TDDY_TEST`,
  `is_test`).
- **No `unwrap()` outside a `lock()`** in any new production module.
- **No `unimplemented!()` or `todo!()` left** — every red-phase stub (`did_change`, `did_close`,
  `with_cancellation`, the seven service methods, `main`) is implemented.
- **No hardcoded secrets**, no unsafe blocks added.
- **No stdout writes** in library code; `tddy-code-restructuring` now has an invariant test enforcing
  it, and `tddy-index-daemon/src/` has no print macro outside one doc comment.
- **No fallbacks**: a set-but-unreachable `TDDY_INDEX_SOCKET` fails, a configured-but-unspawnable
  index daemon fails, a relative workspace root is refused rather than resolved.

**⚠ WARNING — `packages/tddy-code-restructuring/src/runner.rs`: 1,018 production lines.** Already
over the guideline at `master` (1,261 total) and grew by 142. Recorded rather than fixed, with the
four seams named, in
[`2026-09-16-runner-rs-is-1000-production-lines-…`](../todo/2026-09-16-runner-rs-is-1000-production-lines-and-this-change-grew-it.md)
— splitting it on top of 550 changed lines would bury this diff under a mechanical one, which
`CLAUDE.md` forbids.

**⚠ WARNING — 5 `TODO` markers in new production code**, all naming the package that owns the gap and
all recorded in `## Technical Debt`: `main.rs:48` (reaper ownership), `analyze.rs:48` (a capture
cannot be cancelled), `operations.rs:65` (`Warm.ready`), `index_console.rs:15` (the renderer),
`complexity_cache.rs:79` (unbounded cache). None is a placeholder for missing behaviour; each is a
named boundary this change deliberately did not cross.

**ℹ INFO** — `tddy-index-daemon/src/cli.rs` is 538 lines but only 290 production (tests from `:291`),
so it is within budget. `operations.rs` (419) and `index_daemon.rs` (479) are near it; the latter's
author noted the error type should move out next.

**ℹ INFO** — nine untracked files are **not** this change's: `.claude/CLAUDE.md`, `.codex/config.toml`,
`.gemini/settings.json`, `.mcp.json`, `.vscode/mcp.json`, `GEMINI.md`, `opencode.jsonc`. They appeared
on disk during the session as harness configuration. `AGENTS.md` is likewise partly changed by
something other than this work (a CodeGraph section). **They must not be committed with this change.**

**ℹ INFO** — nothing is committed yet. 38 tracked files changed (+1,828 / −768) and 35 new files are
this change's.

### /validate-tests

**10 test files analysed, 582 tests. 1 critical (fixed during validation) · 0 warning.**

**[CRITICAL — fixed] The claim the whole change exists for had no test.** This document's
`## Acceptance Tests` § Production tier planned
`packages/tddy-index-daemon/tests/warm_index_production.rs` with
`a_second_apply_against_a_warm_index_does_not_reload_the_crate_graph`, annotated *"The claim the
whole change exists for."* It was never written. Every other suite runs against `fake_lsp`, which by
construction cannot demonstrate index reuse — a fake has no crate graph to reload. So 582 passing
tests covered the transports, the cancellation, the routing, the error classes and the lifecycle, and
**nothing covered the benefit**.

Written during this validation: two `#[ignore]` production tests over a real rust-analyzer and a real
three-crate cargo workspace —
`a_second_request_against_a_warm_root_does_not_load_the_crate_graph_again` and
`two_roots_in_one_process_each_get_their_own_index`. The assertion is an **absolute** budget on the
second request (3s), not a ratio against the first: a ratio only means something when the first run
is slow, and how slow a cold index is depends on the machine. This direction cannot pass by accident,
because reuse answers from an in-memory map in microseconds while re-loading even a trivial graph is
seconds.

**The evidence the change works, measured.** `Anchors` is the probe, not `Warm`: an anchor goes
through `RustBackend::ensure_indexed`, which polls until the server can actually answer, so the first
one on a root pays the graph load. On a three-crate workspace and a real rust-analyzer:

| | cold | warm | ratio |
|---|---:|---:|---:|
| same root, back to back | 2.15 s | **865 µs** | ≈2,500× |
| first root, after a second root's graph loaded in between | 2.10 s | **880 ms** | 2.4× |

**⚠ Two resident rust-analyzers contend enough to invert the measurement.** Re-asking the first root
after a second root's graph loaded ranged from **880 ms to 3.6 s** across runs — the upper end
*exceeding* that root's own 2.19 s cold load. Three consecutive runs failed a ratio assertion on it.

This is a real operational property of the "one process, many worktrees" decision, not a test defect:
the hosts are cheap but the rust-analyzers are not, and two resident on one laptop compete for CPU.
It does not undermine the change — reuse on a single root is ~2,500× — but it does mean **a developer
warming several worktrees at once will not see per-root reuse at the single-root ratio**, and the PRD's
"memory is unaffected because rust-analyzer is a child either way" was about memory and says nothing
about CPU. Worth a sentence in the feature doc.

The test now asserts this semantically — both roots still answer, and both appear in `Workspaces` —
with the timing claim removed and the reason written into the test. Four consecutive runs green.

The second row is the interesting one and explains the assertion's shape. Reuse holds, but the gap is
1,000× smaller than the first row because **each request builds a fresh `RustBackend` whose `indexed`
flag starts false**, so the readiness probe is re-paid even against a warm server — exactly what
`## Initial Discovery` predicted ("keeping a warm `BackendRegistry` is what actually amortizes the
index — not keeping the client alone"). Worth closing: the daemon holds the registry per root but
rebuilds the backend per request.

**My first draft of this test was wrong twice, and both are worth recording.** It first probed
`Warm`, which returns ready as soon as `get_or_spawn` yields a live server without waiting for the
graph — so it passed in 0.34 s having proven nothing. Switched to `Anchors`, it then asserted an
absolute 3-second budget, which a 2.1-second *cold* load would also have passed. It now asserts a
ratio, which works here because the test constructs the cold run itself, so "the first one was slow"
is guaranteed rather than hoped for, and it is machine-independent.

**Clean across all ten files:**

- **No `#[ignore]` without justification** — the only two carry a reason string naming the cost.
- **No conditionals in any test body.** The `match`/`if` hits are all in drivers and fixtures: the
  `unary_at`/`stream_at` dispatch helpers (wire handling in a driver is where fluent-tests says it
  belongs), two polling helpers, a `/bin/sh` fixture script, and Rust source *strings* whose `if`
  exists so the scored function has a complexity above 1.
- **No try/catch-shaped error swallowing**, no tautologies, no assertion-free tests.
- **No loose matchers where an exact one would do.** M4b's suite added a `rendered()` helper that
  strips the log prefix specifically so each test can assert the **whole** rendered account with
  `assert_eq!` rather than a `contains`.
- **Semantic fixture data** — `grouped`, `src/big.rs`, `foo`, real digests, real commits. No `"foo"`
  standing in for a value that matters.
- **Given/When/Then** throughout, and names that read as behaviour.

**Deliberate limits the suites state in their own comments rather than hiding:**

- `fake_lsp`'s `codeAction` always answers `ContentModified`, so no plan can *succeed* against it.
  That is why `applies_a_plan_and_streams_one_event_per_operation` and
  `queues_a_second_request_for_one_root…` are still unwritten, and why two of M4b's subcommand tests
  pin refusals rather than answers. Both files say so in block comments.
- `IndexDaemonRegistry::connect`'s success leg is untested for want of a fixture that can bind a real
  gRPC-over-UDS socket; its failure leg is tested, and the dialable path is covered from both ends.
- The cold-path tests in `index_daemon_client_acceptance.rs` set `PATH` to an empty directory so a
  cold `anchors` run fails fast instead of indexing this repo for minutes. That is controlling the
  environment, not branching on it, and the helper explains it.

### /validate-prod-ready

**Fit to ship, with 5 named deferrals.**

| Check | Result |
|---|---|
| Mock or fake code in production paths | ✅ none — every fake is a test fixture (`fake_lsp`, three `/bin/sh` scripts, `RecordingCache`) |
| Test-environment branches in production | ✅ none (`cfg!(test)`, `TDDY_TEST`, `is_test` all absent) |
| `unimplemented!()` / `todo!()` | ✅ none — every red-phase stub implemented |
| `unwrap()` outside `lock()` in new code | ✅ none |
| Fallbacks without consent | ✅ none — unreachable socket fails, unspawnable daemon fails, relative root refused |
| Hardcoded secrets | ✅ none |
| Direct stdout in library code | ✅ none, and enforced by a test |
| New external dependencies | ✅ none — `tokio-util`, `futures-util`, `md5`, `tonic` all already in `Cargo.lock`; two were feature additions to existing deps |
| `TODO`/`FIXME` annotations on temporary code | ✅ 5, each naming its owning package and each recorded above |
| Debug output left behind | ✅ none |

The 5 `TODO`s are deferrals with reasons, not placeholders: a capture cannot be cancelled,
`Warm.ready` is weaker than the schema, the complexity cache is unbounded, the renderer is restated
three times, and the reaper's owner is undecided. Each is in `## Technical Debt` with what closing it
would take.

### /analyze-clean-code

| Metric | Result |
|---|---|
| Modules over ~500 production lines | ⚠ 1 — `runner.rs` at 1,018 (already over at `master`; [entry filed](../todo/2026-09-16-runner-rs-is-1000-production-lines-and-this-change-grew-it.md)) |
| New modules within budget | ✅ all — largest new production module is `index_daemon.rs` at 479; `cli.rs` is 538 total but 290 production |
| Duplication | ⚠ the console rendering is stated 3× (recorded above, with a test guarding drift) |
| Magic values | ✅ named constants throughout, each with a doc comment giving its reason |
| Deep nesting | ✅ none flagged by clippy at `-D warnings` across all seven packages |

Both warnings are deliberate: splitting `runner.rs` on top of 550 changed lines would bury a
reviewable diff under a mechanical one, and collapsing the rendering needs a published renderer in a
crate this change already modifies heavily.

## TODO

- [x] Record initial discovery (`2026-09-15-warm-code-intelligence-daemon-initial-discovery.md`)
- [x] Cross-check `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test -p <pkg>` per touched package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
