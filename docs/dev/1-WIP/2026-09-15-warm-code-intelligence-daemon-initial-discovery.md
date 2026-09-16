# Initial Discovery: Warm Code-Intelligence Daemon

**Changeset**: [2026-09-15-warm-code-intelligence-daemon.md](./2026-09-15-warm-code-intelligence-daemon.md)
**Date**: 2026-09-15
**Passes**: 5

## Combined Conclusions

*(rewritten after each pass — see `## Exploration N` at the tail for the evidence)*

### What already exists, and is more than the source MD assumed

A long-lived, reused rust-analyzer **is already built**. `LspRegistry`
(`packages/tddy-lsp/src/registry.rs:48`) is `Clone` over `Arc` interior state, keyed by
`(workspace root, language)`, with idle tracking, pull-based `reap_idle`, `shutdown_all` and
crash-respawn — and `tddy_lsp_executor::register` (`packages/tddy-lsp-executor/src/lib.rs:23`) is
the process-global host pattern, already driven by a 60s reaper in both `tddy-daemon`
(`runtime.rs:874`, `:304`) and `tddy-sandbox-app` (`main.rs:593`).
`docs/ft/coder/reusable-lsp.md` requirements 5–8 and 14 specify exactly this.

So this change is **not** "build a warm-index server". It is: give the restructure/analyze **library
path** a host, because `restructure_cli.rs:97-131` constructs a fresh `TaskRegistry` + `LspRegistry`
per invocation and drops both when the process exits.

The dual-transport half needs no invention either. `packages/tddy-terminal-rpc` is the worked
example of one crate owning, serving and dual-transporting its own proto: a two-pass `build.rs`
(prost + `TddyServiceGenerator`, then `tonic_build` with `extern_path`), one impl of the generated
`tddy-rpc` trait, and gRPC free via the generated `<Service>TonicAdapter`. `--grpc` and `--stdio`
concurrently from one process is already proven and already has an acceptance test to model
(`packages/tddy-e2e/tests/stdio_remote_control_acceptance.rs:141`).

**The generated trait is the in-process seam.** Its methods take and return prost message structs
inside `tddy_rpc::Request`/`Response`, so a same-process caller passes Rust values with no
encode/decode. Single-shot and daemon modes therefore share one code path by construction — no
serialization tax on single-shot, no second implementation to keep in step.

### What has to be fixed before a warm index is correct

Seven defects, none of which bite the current CLI hard because it exits after one request. Each
becomes structural the moment the process survives:

1. **`settle = warmup / 20`** (`backends/rust.rs:454`). `resolution_budget()` (`:2043`) switches
   from `warmup` to `settle` the instant `indexed` flips true. In a warm daemon `indexed` is true
   from request #2 onward — *that is the feature* — so **every** daemon request runs on the settle
   path. `--indexing-budget 900` yields a 45s ceiling, which is the "46s" refusal recorded in
   [`2026-09-10-move-module-to-crate-…`](../todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md).
   The authors documented the failure mode at `rust.rs:463`. ⛔ **Blocking.**
2. **`did_change` and its version counter live in `RustBackend`** (`backends/rust.rs:941`), not in
   `LspClient`, and die with each backend. A second warm request restarts versions at 1 against a
   server that already saw version 40. `LspClient` has only `did_open`, hard-coded to `"version": 1`
   (`client.rs:191`), with no per-URI version state and no open-document set.
3. **Seven `current_dir()` call sites** (`runner.rs:226`, `:321`, `:350`, `:419`, `:454`;
   `restructure_cli.rs:101`). Nothing takes a workspace root, so one process cannot serve two roots.
4. **`progress: fn(&str)`** — bare function pointers that `println!` (`runner.rs:846`). They cannot
   capture a per-request channel. `rust.rs:529` anticipates this exact change: *"anything that speaks
   a protocol on stdout, a persistent server most obviously, would have its stream corrupted."*
5. **`.restructure/` is keyed by root, with no lock and no plan identity** (`runner.rs:797`), and
   `open_run` refuses a second plan with `JournalExists`. Two concurrent daemon clients on one root
   collide.
6. **The idle timer is refreshed only by `get_or_spawn`** (`registry.rs:88`), never by client use.
   A host running a reaper can reap rust-analyzer mid-restructure.
7. **`get_or_spawn` releases the map lock before spawning** (`registry.rs:95`), so concurrent cold
   requests double-spawn and orphan a live task that is never cancelled.

Secondary but real: `drain_notifications` is destructive and single-consumer (`client.rs:341`), so
concurrent requests plus a progress stream steal each other's `$/progress`; `stderr` is piped but
never drained (`server_body.rs:61`), a blocking risk that grows with process lifetime; the stdout
`broadcast` drops frames silently on `Lagged` (`client.rs:435`); and both production hosts pass
`LspAllowList::rust_only()`, which advertises **no** capabilities — so a daemon-spawned
rust-analyzer would return no code actions and no `$/progress` unless it uses
`restructure_allow_list()` (`restructure_cli.rs:199`), and would also lose the toolchain pinning that
exists only on `RustBackend`'s self-spawned path (`rust.rs:754`).

### Two premises in the source MD that the code contradicts

- **"CLI printed `=== exit 0 ===` despite `Error:`"** — false. There is no `process::exit` in the
  crate; refusals propagate through `main() -> Result<()>` and exit **1**
  (`tddy-tools/src/main.rs:157`, `runner.rs:404`). The real defect the TODO backlog names is the
  *error class*: an indexing timeout is reported as `plan is malformed`, which in a daemon decides
  the gRPC status code.
- **"Local WIP: apply observability"** — not in this worktree. `git status` is clean. Any
  observability this change relies on is written fresh.

Also worth correcting: `docs/ft/coder/rust-code-restructuring.md` § LSP integration claims the crate
uses `LspRegistry::rust_only()` and spawns no private rust-analyzer. Both halves are wrong —
it uses `restructure_allow_list()`, and `RustBackend` retains a self-spawning transport
(`rust.rs:770`). **State A is written from the code, not the docs.**

### What the analyze half looks like

`tddy-code-analysis` is already daemon-shaped and has **no caching at all**: synchronous, no tokio,
no LSP, every entry point path-taking and cwd-free, and `capture_coverage` already takes a real
closure sink (`coverage.rs:128`). `complexity::file_complexity(source)` is pure over a `&str` and
caches trivially by content hash. It also has **no `tests/` directory**, so every acceptance test for
this half is a new file. Its long operations are genuinely long — 55.8 min for a `tddy-daemon`
coverage capture, ~22 min for duplicate-tests — which makes them server-streaming jobs, not unary
calls.

### How the daemon hosts it, and why cancellation is the hard part

`LspRegistry` is the template one level up: the daemon already runs a lazily-spawned,
workspace-root-keyed, idle-reaped, `TaskRegistry`-backed rust-analyzer, and `registry.rs:1-6` states
the layering rule — `TaskRegistry` owns process lifetime and the SIGTERM→SIGKILL net, a
purpose-built registry owns identity and connection. `LspServerBody` is the child body to copy: it
registers the child pid, selects on `child.wait()` so an exit is observed rather than discovered,
and shuts down gracefully before killing. The sandbox-runner path does neither, and that is now
recorded as a backlog defect rather than copied.

The surprise is cancellation. `tddy_rpc::RpcService` passes a handler no token, no deadline and no
context (`bridge.rs:33-69`). `ServerEngine` does abort a disconnected peer's forwards
(`server_engine.rs:212-235`) — but it is absent from the tonic and Connect-HTTP paths entirely, and
**a dropped future stops nothing that is blocking**, while the restructure backend runs inside
`spawn_blocking` with `std::thread::sleep` poll loops.

So removing the budgets is not a deletion. The token has to come from a `TaskBody` in a
`TaskRegistry` and be checked *inside* those loops, and the only disconnect signal a handler can get
is a send failing into the response stream's dropped receiver — which is what makes the
server-streaming shape load-bearing rather than merely nice for progress. `tokio-util` is per-crate
rather than workspace-level, and `tddy-rpc` has none, which is precisely why a trait-level fix is a
cross-cutting change and is recorded for later instead.

### Consequences for the test strategy

A test that boots real rust-analyzer costs minutes and is, by
[`docs/dev/guides/testing.md`](../guides/testing.md) § Production Tests, an `#[ignore]` production
test excluded from CI. The acceptance gate must therefore run against **`fake_lsp`**
(`packages/tddy-lsp/tests/bin/fake_lsp.rs`), the deterministic fake every existing `tddy-lsp` test
already uses. The transport half is covered by the two established harnesses: in-process dispatch at
the literal registered coordinate, and two `StdioEndpoint::from_duplex` halves over
`tokio::io::duplex`. Only the "one real index, two warm requests" claim needs a real server, and
that is an `#[ignore]` production test.

Note finally that `./test` exits with `tail`'s status, not cargo's
([`2026-09-09-the-test-script-…`](../todo/2026-09-09-the-test-script-reports-green-on-a-red-suite.md)),
so verification for this change reads `.verify-result.txt` **content**, never the exit code.

## Exploration 1 — parent-driven orientation (Grep / Glob / Read)

Run by the planning session itself, before any Explore agent.

### Sequence

1. `cat CLAUDE.md` — repo conventions, verification rules, judgment boundaries.
2. `cat .agents/skills/planning/references/planning-phase.md` — the canonical planning steps.
3. `ls packages/` — 84 workspace packages; confirmed `tddy-code-analysis`,
   `tddy-code-restructuring`, `tddy-lsp`, `tddy-lsp-executor`, `tddy-tools` all exist as separate
   crates.
4. `git status --short` — **clean.**
5. `find . -name 'restructure_cli.rs'` → `packages/tddy-code-restructuring/src/restructure_cli.rs`.
6. `find packages/tddy-code-restructuring packages/tddy-code-analysis packages/tddy-lsp -name '*.rs'`
   — module inventories (recorded below).
7. `ls docs/dev/todo/ | sort` — 130 backlog entries; then the Step 2b greps (Exploration 1b).
8. `grep -nE '^#{1,3} ' docs/ft/coder/{rust-code-restructuring,rust-code-analysis,reusable-lsp}.md`
   then `sed -n` over the load-bearing sections.
9. `sed -n '1,60p' Cargo.toml` — workspace member list, for adding a new crate.
10. `cat run-livekit-testkit-server` — the env-var handshake this plan was asked to mirror.
11. `cat .agents/skills/fluent-tests/references/generic-guidelines.md` and `.../rust/std-test.md`.
12. `grep -nE '^#{1,3} ' docs/dev/guides/testing.md` + the Production Tests section.

### Inspected files

**`git status` is clean.** The source MD this plan derives from describes "Local WIP: apply
observability — uncommitted on `feature/carve/restructure-moves`" across `runner.rs`,
`restructure_cli.rs` and `backends/rust.rs`. **None of it is in this worktree.** Any observability
this change relies on is written fresh, not merged. Recorded because the source MD's sequencing
("Priority 1 — land observability WIP") assumes a diff that does not exist here.

`packages/tddy-code-restructuring/src/` — `edit.rs`, `ledger.rs`, `registry.rs`, `journal.rs`,
`lib.rs`, `overlay.rs`, `runner.rs`, `plan.rs`, `apply.rs`, `restructure_cli.rs`, `verify.rs`,
`crate_move.rs`, `backends/{mod,rust,lsp_bridge}.rs`. Tests:
`tests/move_module_to_crate_acceptance.rs`, `tests/rename_cross_file_acceptance.rs`,
`tests/harness/mod.rs`.

`packages/tddy-code-analysis/src/` — `coverage.rs`, `crap.rs`, `complexity.rs`, `error.rs`,
`analyze_cli.rs`, `lib.rs`, `report.rs`, `duplicate_tests.rs`. **No `tests/` directory.**

`packages/tddy-lsp/src/` — `registry.rs`, `protocol.rs`, `client.rs`, `error.rs`, `lib.rs`,
`allowlist.rs`, `server_body.rs`. Tests: `client_roundtrip_test.rs`, `registry_reuse_test.rs`,
`server_body_test.rs`, and **`tests/bin/fake_lsp.rs`** — a deterministic fake language server
declared as a `[[bin]]` so `CARGO_BIN_EXE_fake_lsp` reaches the test targets. This is the single
most important fixture for this change's test strategy: every existing `tddy-lsp` test runs against
it rather than a real rust-analyzer.

`docs/ft/coder/1-WIP/` holds only `archived/` (four 2026-03 PRDs), so the PRD for this change is the
only live one in the Coder area. `docs/ft/1-WIP/` is **empty** — `prd-doc.mdc` names it as the
location but `planning-phase.md` and every existing PRD in the tree use
`docs/ft/{area}/1-WIP/`. Followed the tree.

`docs/dev/1-WIP/` holds three unrelated active changesets — `2026-08-31-split-sandbox-orchestration`,
`2026-08-31-split-sandbox-resume`, `2026-09-09-livekit-rooms-panel-every-connection`. **No conflict**
with this change's packages.

`plans/` holds three grill-me briefs, none related.

#### `docs/ft/coder/reusable-lsp.md` — the existing product contract, and the tension with it

Requirements 4–8 and 14–15 already specify long-lived, reused servers:

> 5. Servers are keyed by **(workspace root, language)**. Two requests with the same key
>    return the **same** running server (one task).
> 6. Servers are lazily started on first use (get-or-spawn) and **torn down after an idle
>    timeout**. Activity resets the idle timer.
> 14. **`tddy-daemon`** is the primary owner: it holds the registry beside the shared
>     `TaskRegistry`, runs the idle-reaper loop, and serves LSP tool calls.

And § Future Considerations explicitly defers:

> - Sharing one server across distinct sessions in the same workspace (initial slice
>   reuses across targets within an owner; cross-session sharing is a follow-up).

So "a long-lived process holding a warm rust-analyzer" is **already built and already owned by
`tddy-daemon`**. What is *not* built is a long-lived host that the **restructure/analyze CLI path**
can reach: `tddy-tools restructure` builds its own registry per invocation. The new binary is
therefore a *second* host of an existing capability, and the plan must say why it is not requirement
14's daemon. See the changeset's `## Decisions & trade-offs`.

§ Restructuring bridge also records what the assist consumer needs from the transport — the caller
chooses the handshake, a JSON-RPC `error` reaches the caller as `LspError::Server { code, message }`
(and `ContentModified` (-32801) means "ask again"), and `drain_notifications` is the only account of
a load that answers no requests.

#### `docs/ft/coder/rust-code-restructuring.md` — CLI surface and the budget contract

Documented CLI:

```text
tddy-tools restructure apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]
                                       [--indexing-budget SECONDS]
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check <plan.jsonl> [--deep] [--budget LINES] [--indexing-budget SECONDS]
tddy-tools restructure anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]
tddy-tools restructure verify --against <git-ref>
```

Eight operations: `extract_method`, `extract_variable`, `rename_symbol`, `extract_module`,
`extract_module_to_file`, `extract_trait`, `inline_method`, `move_module_to_crate`.

§ Budgets states the contract this change depends on:

> `--indexing-budget SECONDS` governs every wait the run makes: the one-time crate-graph warm-up,
> the per-request timeout on the shared client, and the per-operation settle budget once the first
> index has succeeded (a twentieth of the run budget, floored at 30s). An indexing timeout is not a
> plan defect, and the message says so along with **how far the index got**.

`docs/dev/todo/2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md` § 2 reports
that this **is not true of the code**. Doc and implementation disagree; see Exploration 1b.

§ LSP integration states the crate does not spawn its own server:

> Restructuring uses the existing long-running rust-analyzer task (`LspRegistry::rust_only()`). The
> restructure crate does **not** spawn a private rust-analyzer.

Exploration 2 found this is **false in two ways**: the allow-list used is `restructure_allow_list()`
(not `rust_only()`), and `RustBackend` retains a self-spawning path that bypasses `LspRegistry`
entirely. The doc describes an intent, not the code.

§ Known limitations records that `extract_method` needs type inference where `extract_module` needs
only the syntax tree — so index readiness is operation-dependent, which matters for what a warm
daemon can promise.

#### `docs/dev/guides/testing.md` — what this change may and may not do in a test

- § Mandatory Test Style: `fluent-tests` — non-negotiable.
- § Anti-Patterns names, among others, Conditional Test Skipping, Try/Catch Workarounds,
  Conditional Logic in Tests, Fallback Assertions, **Environment Detection in Tests**, "TODO" Test
  Placeholders, Multiple Code Paths in One Test.
- § Production Tests — `#[ignore]`, 30s–4min each, excluded from CI.

Combined with Exploration 2's finding that `fake_lsp` exists, this settles the test strategy: a test
that boots **real** rust-analyzer is a production test (minutes, `#[ignore]`) and cannot be the
acceptance gate. The acceptance tests must run against `fake_lsp`.

Note also `docs/dev/todo/2026-09-09-the-test-script-reports-green-on-a-red-suite.md`: `./test` exits
with `tail`'s status, not cargo's, so **no `./test` result in this repo can be trusted** until that
lands. Verification for this change reads `.verify-result.txt` content, not the exit code.

#### `Cargo.toml` — workspace members

`[workspace] resolver = "2"` with an explicit `members` list. `tddy-lsp`, `tddy-code-analysis`,
`tddy-code-restructuring`, `tddy-lsp-executor`, `tddy-codegen`, `tddy-rpc`, `tddy-stdio`,
`tddy-service`, `tddy-connectrpc` are all listed. A new crate must be added to this list explicitly
— there is no glob.

#### `run-livekit-testkit-server` — the handshake to mirror

Reuses a fixed-name container, discovers its mapped port, and prints to **stdout**:

```bash
echo "export LIVEKIT_TESTKIT_WS_URL=${ws_url}"
```

with all human-facing chatter on **stderr** (`>&2`), so `eval $(./run-livekit-testkit-server | grep '^export ')`
works. Tests read the env var and fall back to starting their own container when it is absent. This
is exactly the shape the opt-in client wiring was chosen to copy — and the stdout/stderr split is
the same discipline the `--stdio` transport needs for a different reason.

### Findings

1. The "warm long-lived rust-analyzer" capability is **already specified and already hosted** by
   `tddy-daemon` (`reusable-lsp.md` req. 14). The gap is that the restructure/analyze **CLI** path
   does not use it.
2. The observability work the source MD treats as a prerequisite **does not exist in this tree**.
3. `tddy-code-analysis` has **no `tests/` directory at all** — every acceptance test for the
   analyze half of this change is a new file.
4. `fake_lsp` makes fast, deterministic acceptance tests possible; real rust-analyzer tests are
   `#[ignore]` production tests by this repo's own definition.
5. Two feature docs describe behaviour the code does not have (`--indexing-budget` honoured;
   `rust_only()` used; no private spawn). State A must be written from the code, not the docs.

## Exploration 1b — `docs/dev/todo/` cross-check (Step 2b)

### Grep / glob

```bash
grep -rl -iE 'restructure|rust-analyzer|rust_analyzer|lsp|analyz|crap|coverage|duplicate-test|proto|codegen|tonic|grpc|stdio' docs/dev/todo/
```
→ 58 of 130 entries. Read the body of each plausible hit; the ones that survived are below.

### Inspected files and the excerpts that decided each verdict

**`2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md` § 2 — the blocking one.**

> ```
> tddy-tools restructure apply --dry-run --indexing-budget 900
> ```
> ran roughly 20 minutes of rust-analyzer indexing, reached `working (100%)`, then failed with
> **"rust-analyzer had not finished indexing after 46s"**. The 900-second budget was accepted on the
> command line and then measured against what looks like a fixed 46-second ceiling. On a workspace
> this size the flag exists precisely for this case, so as it stands the operation cannot be applied
> to `tddy-daemon` at all.

Recorded rather than fixed because node 3's `## Boundaries` assigned the operation to node 1.

**`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`** — four items land on this
change:

> - **The journal is repo-scoped, not plan-scoped** (`.restructure/journal.jsonl`). A *completed*
>   plan blocks the next one with *"a journal already exists for this plan — pass `--resume`"*, and
>   `--resume` would resume the wrong plan.

and, from § `#unbundle` node 6:

> **`apply` of the same 3-module layer-1 plan then refused**, after roughly 35 minutes of indexing:
>
>     Error: plan is malformed: rust-analyzer never settled enough to answer textDocument/references
>
> - **`plan is malformed` is the wrong error class.** The plan was not malformed; the indexer did not
>   settle. A caller cannot distinguish a genuine schema problem from an indexing timeout… This is
>   the same shape as the budget-overrun exiting zero, recorded above: the operation reports the
>   wrong thing about its own failure.

**`2026-09-09-the-test-script-reports-green-on-a-red-suite.md`**

> - **The exit code is `tail`'s, not cargo's.** `./test` pipes cargo through `tail` to write
>   `.verify-result.txt`, so the script exits 0 whatever cargo did. … Anything trusting that status
>   — an agent, a hook, a CI step — reads a red suite as passing.

Same defect class as the one this change fixes in `restructure apply`, in a different script.

**`2026-09-09-keeping-target-from-overhogging-the-disk.md`**

> during the `connection_service.rs` split this machine hit **1.5 GiB free / 100% full** three
> times, and `rm -rf target` was used each time. That reclaimed 28–38 GiB but cost a full cold
> rebuild *and* a full cold rust-analyzer index (~7–10 minutes each) every single time.

| Path | Size |
|---|---:|
| `target/debug/incremental` | **3.1 G** |
| `target/debug/deps` | 1.9 G |
| **total** | **5.5 G** |

**`2026-09-09-versioned-proto-package-names.md`**

> `connection.proto` declares a bare `package connection;` … Nothing in the workspace uses a
> versioned package name (`tddy.host.v1`), so nothing can be evolved behind one. … Worth doing as
> its own change, across all protos at once, after the `#unbundle` stack lands — a partial rename is
> worse than none, because it makes the convention unreadable.

**`2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md`**

> Node 6's `generate_tonic_adapter` emits handler bodies that call `tddy_service::to_tonic_status`,
> by design … Node 7's two adapters are the first generated ones to land in **`tddy-service`'s own**
> `OUT_DIR`. Inside that crate `tddy_service::…` does not resolve.

Fix applied there was `extern crate self as tddy_service;` at `packages/tddy-service/src/lib.rs:11`.

**Analyze-side entries** (in scope because the analyze surface is):

- `2026-09-09-duplicate-tests-is-quadratic-over-per-test-signatures.md` — "Detecting subset and
  identical signatures over **2,159** captured tests took **~22 minutes**".
- `2026-09-09-coverage-capture-writes-its-denominator-only-at-the-end.md` — "A failure at test 2,100
  of 2,159 therefore loses `rust-coverage-final.json` entirely"; 55.8 min for `tddy-daemon`.
- `2026-09-09-crap-scores-coverage-as-a-boolean-not-a-ratio.md` — `crap.rs:66-69` sets
  `covered: record.count > 0`, so CRAP collapses to `complexity` for anything executed once.
- `2026-09-09-coverage-rs-module-hygiene.md` — `normalize_export` is 93 lines at depth 4; file over
  500 lines.
- `2026-09-09-llvm-cov-ignore-filename-regex-does-not-filter-functions.md` — recorded so nobody
  "fixes" the wrong thing.

**Read and ruled unrelated** (same area, different concern): `2026-09-09-macro-expansion-as-a-restructure-operation.md`
(research, recommends *not* building it), `2026-09-09-restructure-defects-from-the-connection-service-split.md`
(D6–D9 marked fixed), `2026-08-15-echoes-a-message-over-sandbox-service-served-over-stdio-is-skipped-in.md`
(a cgroup permission failure, not a stdio-transport defect),
`2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md`
(a `tddy-tools` manifest concern — but see the changeset: this change adds a binary and must not
repeat it), `2026-09-09-tddy-daemon-untested-complexity-hotspots.md`,
`2026-09-12-the-two-new-service-rs-files-are-over-budget.md`.

### Findings

The scan produced **one ⛔ blocking item** (`--indexing-budget` is not honoured — a warm-index
daemon cannot be built on a code path that gives up after 46s regardless of the budget), and several
⚠ during items, of which the repo-scoped journal is the one that constrains the daemon's concurrency
model. Verdicts and links are carried in the changeset's `## Prerequisites`.

## Exploration 2 — `tddy-lsp` / `tddy-lsp-executor` lifecycle (Explore agent)

### Sequence

Glob/read over `packages/tddy-lsp/src/*.rs` and `packages/tddy-lsp-executor/src/*.rs` and their
`tests/`; then repo-wide greps for `LspRegistry` / `get_or_spawn` / `tddy_lsp`, for `Send + Sync`
assertions, and for `notify::` / `RecommendedWatcher` / `inotify` / `FsEventWatcher` /
`notify_debouncer` across both `*.rs` and `Cargo.toml`.

### Inspected files and excerpts

#### `packages/tddy-lsp/src/registry.rs` — reuse, idle, reap, respawn all already exist

`registry.rs:48-68`:

```rust
/// Per-`(root, language)` registry with lazy get-or-spawn and idle teardown.
#[derive(Clone)]
pub struct LspRegistry {
    allow: LspAllowList,
    task_registry: TaskRegistry,
    /// Live services keyed by workspace+language, each with its idle-timer.
    services: Arc<Mutex<HashMap<LspKey, ServiceEntry>>>,
    idle_timeout: Duration,
}
```

`registry.rs:24-46`:

```rust
type ServiceEntry = (Arc<LspService>, IdleTimeoutTracker);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LspKey {
    pub root: PathBuf,
    pub language: Language,
}

pub struct LspService {
    pub task_id: TaskId,
    pub client: Arc<LspClient>,
}
```

It is `Clone` over `Arc` interior state, not held as `Arc<LspRegistry>`. `Send + Sync` holds
structurally — `LspAllowList` is a `HashMap` (`allowlist.rs:74-77`), `TaskRegistry` is `Clone` over
`Arc<RwLock<…>>` + `Arc<broadcast::Sender<…>>` (`packages/tddy-task/src/registry.rs:41-45`),
`IdleTimeoutTracker` is `std::sync::Mutex<Instant>` + `Duration` (`packages/tddy-task/src/idle.rs:8-12`).
**No `assert_send`/`Send + Sync` assertion exists anywhere in either crate.** De-facto proof: it is
already moved across `tokio::spawn` in the daemon reaper (`runtime.rs:304-313`).

`get_or_spawn` (`registry.rs:70-144`), the reuse half:

```rust
    pub async fn get_or_spawn(&self, key: LspKey) -> Result<Arc<LspService>, LspError> {
        if !self.allow.is_allowed(key.language) {
            return Err(LspError::LanguageNotAllowed(key.language.id().to_string()));
        }

        {
            let mut services = self.services.lock().await;
            if let Some(existing) = services.get(&key).map(|(svc, _)| Arc::clone(svc)) {
                let alive = match self.task_registry.get(&existing.task_id).await {
                    Some(handle) => !handle.status().is_terminal(),
                    None => false,
                };
                if alive {
                    // Reuse: refresh the idle timer and hand back the same server.
                    if let Some((_, tracker)) = services.get(&key) {
                        tracker.record_activity();
                    }
                    return Ok(existing);
                }
                // The task died — drop the stale entry and spawn a fresh server.
                services.remove(&key);
            }
        }
```

`SPAWN_TIMEOUT` is a hard 30s (`registry.rs:21-22`, applied `registry.rs:120-130`).

**Thundering herd.** The `services` lock is released at `registry.rs:95`, *before* the spawn. Two
concurrent `get_or_spawn` calls on the same cold key each spawn a rust-analyzer; the second `insert`
at `registry.rs:136` overwrites the first, orphaning a live task that is never cancelled — it stays
in `TaskRegistry` but is unreachable for reaping. There is no in-flight placeholder.

`reap_idle` (`registry.rs:162-180`) — pull-based, a caller must drive it:

```rust
    pub async fn reap_idle(&self) -> Vec<LspKey> {
        let mut services = self.services.lock().await;
        let expired: Vec<LspKey> = services
            .iter()
            .filter(|(_, (_, tracker))| tracker.should_shutdown())
            .map(|(key, _)| key.clone())
            .collect();

        let mut reaped = Vec::new();
        for key in expired {
            if let Some((service, _)) = services.remove(&key) {
                self.task_registry.cancel_task(&service.task_id).await;
                reaped.push(key);
            }
        }
        reaped
    }
```

Also `shutdown_all` (`registry.rs:191-200`), `get(&key)` (peek, no spawn, **no** idle refresh —
`registry.rs:182-189`), `bind_target` = get_or_spawn + `did_open` per source
(`registry.rs:146-160`), and `workspace_root_for` picking the **outermost** `Cargo.toml` ancestor
(`registry.rs:206-216`).

**The idle timer is refreshed only by `get_or_spawn`** (`record_activity`, `registry.rs:88`).
Direct use of `service.client` — which is exactly what `tddy-code-restructuring` does, for minutes
at a time — does not touch the tracker.

Crash recovery is the `alive`/`is_terminal` check only; a wedged-but-alive rust-analyzer counts as
healthy.

`tests/registry_reuse_test.rs` — six tests, all against `fake_lsp`:

| line | test |
|---|---|
| 42 | `two_targets_in_one_workspace_reuse_a_single_language_server` |
| 64 | `different_workspace_roots_get_separate_language_servers` |
| 85 | `requesting_a_disallowed_language_returns_an_error_and_spawns_no_server` |
| 99 | `an_idle_language_server_is_torn_down_after_the_timeout` |
| 119 | `activity_keeps_a_language_server_alive_past_the_idle_timeout` |
| 144 | `a_crashed_language_server_is_respawned_on_the_next_request` |

Not covered: concurrent `get_or_spawn` on a cold key, `shutdown_all`, reuse across a source-file
change, reuse spanning more than two calls.

#### `packages/tddy-lsp/src/client.rs` — progress is there, document sync is not

`client.rs:100-124`:

```rust
pub struct LspClient {
    /// Outbound byte stream to the server's stdin (framed JSON-RPC).
    stdin: mpsc::UnboundedSender<Bytes>,
    /// Monotonic source of request ids.
    next_id: AtomicI64,
    /// In-flight requests awaiting their response.
    pending: Pending,
    /// Latest published diagnostics per document.
    diagnostics: DiagnosticsCache,
    /// The background reader draining the server's stdout; aborted on drop.
    reader: JoinHandle<()>,
    request_timeout_ms: AtomicU64,
    handshake: Mutex<Value>,
    notifications: Notifications,
}
```

Inbound is a `broadcast::Receiver<Bytes>` (`client.rs:141-142`, `read_loop` `client.rs:419-439`),
and `RecvError::Lagged(_) => continue` (`client.rs:435`) **silently drops frames** if the reader
falls behind — which can drop a *response* and strand a request until its timeout.

Correlation: `AtomicI64` → `HashMap<i64, oneshot::Sender<Response>>` (`client.rs:86-87`,
`client.rs:367-402`); covered by `correlates_concurrent_requests_by_id`
(`tests/client_roundtrip_test.rs:216`). `response_payload` (`client.rs:496-509`) splits errors from
results.

Timeouts: `DEFAULT_REQUEST_TIMEOUT = 10s` (`client.rs:25`), mutable through `&self` via
`set_request_timeout` (`client.rs:356`) / `request_timeout` (`client.rs:362`). The doc comment
(`client.rs:19-24`) says 10s is "far too short for a code-action request against a cold index, which
is why it is a default rather than a cap."

`$/progress` **is** captured — `client.rs:91-98` and `client.rs:118-123`:

```rust
type Notifications = Arc<Mutex<VecDeque<Value>>>;

/// How many undrained notifications to keep before dropping the oldest.
const NOTIFICATION_BACKLOG: usize = 256;
```

```rust
    /// Server notifications this client does not consume itself, kept for a caller to drain.
    ///
    /// `$/progress` and `experimental/serverStatus` are the two that matter: they are the only
    /// account of what a server is doing during a load that answers no requests, and dropping
    /// them left every such wait silent and every timeout unable to say where the server got to.
    notifications: Notifications,
```

but only through a **destructive, single-consumer** drain (`client.rs:341-344`):

```rust
    /// Take every server notification received since the last drain, oldest first.
    pub fn drain_notifications(&self) -> Vec<Value> {
        self.notifications.lock().unwrap().drain(..).collect()
    }
```

Two concurrent observers steal each other's notifications. There is no broadcast or watch of
progress, and the backlog silently drops the oldest past 256 (`client.rs:478-482`, asserted at
`client.rs:654`).

Inbound dispatch is three-way (`client.rs:443-494`): `publishDiagnostics` → cache; a `method` **with**
an `id` → auto-acknowledged server→client request (`client/registerCapability`,
`window/workDoneProgress/create`, `workspace/configuration` — `server_request_reply`,
`client.rs:514-527`); everything else → the retained ring buffer.

**Document sync — what exists and what does not:**

| notification | in `LspClient`? |
|---|---|
| `textDocument/didOpen` | ✅ `did_open` — `client.rs:183-196` |
| `textDocument/didChange` | ❌ absent |
| `textDocument/didSave` | ❌ absent |
| `textDocument/didClose` | ❌ absent |
| `workspace/didChangeWatchedFiles` | ❌ absent |
| `workspace/didChangeConfiguration` | ❌ absent |
| `rust-analyzer/reloadWorkspace` | ❌ absent |

`did_open` **hard-codes `"version": 1`** (`client.rs:191`) and the client keeps no per-URI version
counter and no open-document set.

`did_change` **is** implemented — in the wrong crate, with per-process state.
`packages/tddy-code-restructuring/src/backends/rust.rs:941-953`:

```rust
    fn did_change(&mut self, uri: &str, text: &str) -> Result<()> {
        self.doc_version += 1;
        let version = self.doc_version;
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }]
            }),
        )
    }
```

Full-text sync, counter owned by `RustBackend`, reaching the shared client through
`LspClientBridge::notify` → `notify_raw` (`backends/lsp_bridge.rs:41-45`). **The counter dies with
the `RustBackend`**, so a second request in a warm daemon restarts versions at 1 against a server
that already saw version 40.

Escape hatch (`client.rs:331-339`): `request_raw` / `notify_raw`. Other ops: `diagnostics`
(cache-then-pull, `client.rs:198-211`), `workspace_diagnostics` (`:213-231`), `definition`,
`references`, `hover`, `symbols`, `workspace_symbols`, `shutdown` (`:233-329`), `handshake()`
(`:347-349`). `Drop` aborts the reader (`:126-130`).

#### `packages/tddy-lsp/src/protocol.rs` — framing only

62 lines, **no message types**: `encode_message` (`:9-14`), `FrameReader::{new,push,next_message}`
(`:18-46`), private `find_subsequence` / `parse_content_length`. Header (`:1-4`): *"Minimal LSP
JSON-RPC framing (`Content-Length` headers) over byte streams. This is a deterministic codec — the
transport itself is a `tddy-task` channel."*

All messages are untyped `serde_json::Value`. `lsp-types = "0.94"` is declared in `Cargo.toml` and
**imported nowhere** — a dead dependency. Methods named as string literals in `client.rs`:
`initialize` (:176), `textDocument/diagnostic` (:206), `workspace/diagnostic` (:217),
`textDocument/definition` (:236), `textDocument/references` (:245), `textDocument/hover` (:252),
`textDocument/documentSymbol` (:264), `workspace/symbol` (:298), `shutdown` (:326); notifications
sent `initialized` (:178), `textDocument/didOpen` (:186), `exit` (:327).

#### `packages/tddy-lsp/src/allowlist.rs` — the capability gate

`Language` has **only `Rust`** (`:9-22`). `LaunchSpec` (`:24-45`) carries `program`, `args`, `env`,
`capabilities`, `initialization_options`, with the doc note (`:33-39`) that *"rust-analyzer in
particular returns no code actions at all to a client that advertised no `codeAction` support…
Empty by default."* Builders `with_capabilities` / `with_initialization_options` (`:59-69`).
`LspAllowList` (`:72-107`) is a `HashMap<Language, LaunchSpec>`; **`rust_only()` =
`LaunchSpec::new("rust-analyzer")` with empty capabilities** (`:102-106`).
`language_for_target_type` (`:112-117`) maps `"rust_binary" | "rust_library"` → `Rust`.

The gate runs before spawn (`registry.rs:74-76`, again at `:97-101`).

#### `packages/tddy-lsp/src/server_body.rs` — tddy-lsp is a client/host, not a server

`LspServerBody` is a `tddy_task::TaskBody` owning one rust-analyzer child (`:27-36`). No
`tower-lsp`/`lsp-server` dependency; nothing implements `initialize` on the receiving side.

`run` (`:40-174`): `Command::new(&spec.program)` with args/env, `current_dir` only if the root exists
(`:48-61`), all three streams `Stdio::piped()`; `ctx.register_child_pid(pid)` (`:75-77`) for the
SIGTERM→SIGKILL net; stdin bridge from `mpsc::UnboundedReceiver<Bytes>` (`:90-104`); stdout fan-out
in 8 KiB reads into the broadcast (`:107-121`, `STDOUT_CHUNK = 8192` at `:25`); handshake then
`client_tx.send` (`:124-145`); run until cancel or exit (`:148-164`); bounded shutdown with
`GRACEFUL_SHUTDOWN = 500ms` (`:22`), `client.shutdown()`, `start_kill`, `wait` (`:168-173`).

**`stderr` is piped but never read** (`:61`) — rust-analyzer's own log output is discarded and can
fill the pipe buffer and block the server. `tests/server_body_test.rs` covers only
`a_language_server_task_stays_running_until_it_is_cancelled` (:37) and
`an_unresponsive_language_server_is_killed_after_the_grace_period` (:67).

#### `packages/tddy-lsp-executor` — the existing long-lived-registry precedent

`src/lsp_tools.rs` is a pure catalog (130 lines). `:16-23`:

```rust
/// The five language-agnostic tool names, in catalog order.
pub const LSP_TOOL_NAMES: [&str; 5] = [
    "LspDiagnostics",
    "LspDefinition",
    "LspReferences",
    "LspHover",
    "LspSymbols",
];
```

Gated by env, not a live probe (`:12-14`, `:67-73`):

```rust
pub const LSP_TOOLS_ENV: &str = "TDDY_LSP_TOOLS";

pub fn lsp_tools_enabled() -> bool {
    std::env::var(LSP_TOOLS_ENV).is_ok_and(|value| !value.trim().is_empty())
}
```

MCP shape applied in `tddy-tools`: `lsp_tool_defs()` at `packages/tddy-tools/src/server.rs:1596-1610`,
merged at `server.rs:342-344`. **No restructure tool and no analyze tool in this set.**

`src/lib.rs:23-35` is the pattern to copy:

```rust
/// Register a process-global executor over `allow`, sharing `task_registry` (so LSP
/// servers appear as ordinary tasks) and reaping servers idle past `idle_timeout`.
/// Returns the underlying [`LspRegistry`] so the caller can drive an idle-reaper loop.
pub fn register(
    task_registry: TaskRegistry,
    allow: LspAllowList,
    idle_timeout: Duration,
) -> LspRegistry {
    let executor = TddyLspExecutor::new(allow, task_registry, idle_timeout);
    let registry = executor.registry();
    register_lsp_executor(Arc::new(executor));
    registry
}
```

Registration is a process-global **first-write-wins `OnceLock`**
(`packages/tddy-core/src/toolcall/lsp.rs:57-67`):

```rust
static REGISTERED: OnceLock<Arc<dyn LspExecutor>> = OnceLock::new();

/// Register the process-wide LSP executor. The first registration wins.
pub fn register_lsp_executor(executor: Arc<dyn LspExecutor>) {
    let _ = REGISTERED.set(executor);
}
```

So the executor can never be replaced or reconfigured after first registration, and cannot be
un-registered for shutdown.

Per-call flow (`lib.rs:95-115`): resolve target → language + `LspKey{workspace_root_for(repo_dir),
language}` → read file from disk → `registry.bind_target(key, &srcs)` → `did_open`. **Every tool
call re-`did_open`s the file with `"version": 1`** — that is how staleness is papered over today.
The trait is synchronous; each method wraps `block_on` (`lib.rs:118-123`), so callers must be inside
`spawn_blocking`. `is_available` calls `build_list_json` (reads `BUILD.yaml`) on **every**
invocation with no caching (`lib.rs:69-78`, `:126-128`).

Cross-call sharing is proven by `two_targets_in_one_workspace_share_one_server_and_resolve_queries`
(`tests/e2e_test.rs:57`).

#### Every user of `LspRegistry` / `tddy_lsp::`

**A. `tddy-daemon`** — `packages/tddy-daemon/src/runtime.rs:874-881`:

```rust
        // Reusable-LSP executor: a Rust-only executor sharing this daemon's task registry,
        // so `Lsp*` tool calls (relayed through tddy-tool-engine) resolve to a real, reused
        // language server; the loop that reaps servers left idle is the host's to start.
        tasks.lsp_idle_reaper = Some(tddy_lsp_executor::register(
            task_registry.clone(),
            tddy_lsp::LspAllowList::rust_only(),
            Duration::from_secs(300),
        ));
```

Field `lsp_idle_reaper: Option<tddy_lsp::LspRegistry>` (`runtime.rs:174`, default `None` at `:582`);
the reaper loop (`runtime.rs:304-313`):

```rust
        if let Some(reaper) = self.lsp_idle_reaper {
            handles.push(tokio::spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(60));
                ticker.tick().await; // consume the immediate first tick
                loop {
                    ticker.tick().await;
                    reaper.reap_idle().await;
                }
            }));
        }
```

The field is **moved** into the loop, so the daemon keeps no other handle on the registry.

**B. `tddy-sandbox-app`** — `packages/tddy-sandbox-app/src/main.rs:593-610`, same shape but with its
own `TaskRegistry::new()` and the registry scoped inside a block, surviving only in the spawned
reaper. Both hosts: **300s idle timeout, 60s reap tick, `rust_only()` (no capabilities).**

**C. `tddy-tools restructure`** — `packages/tddy-code-restructuring/src/restructure_cli.rs:98-125`,
reached from `packages/tddy-tools/src/main.rs:96` and dispatched at `main.rs:157`:

```rust
        let root = std::env::current_dir().context("current_dir")?;
        let task_registry = TaskRegistry::new();
        let lsp_registry = LspRegistry::new(
            restructure_allow_list(),
            task_registry,
            Duration::from_secs(600),
        );
        let key = LspKey { root, language: Language::Rust };
        let service = lsp_registry
            .get_or_spawn(key)
            .await
            .context("rust-analyzer LSP")?;
        service.client.set_request_timeout(request_timeout(&options));
        Some(Arc::clone(&service.client))
```

A process-local variable that dies with the CLI — a fresh rust-analyzer and a fully cold index on
every run. It is also the **only** caller that configures capabilities
(`restructure_cli.rs:199-208`):

```rust
fn restructure_allow_list() -> LspAllowList {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new("rust-analyzer")
            .with_capabilities(crate::client_capabilities())
            .with_initialization_options(crate::server_settings()),
    );
    allow
}
```

`client_capabilities()` (`packages/tddy-code-restructuring/src/backends/rust.rs:141-181`) requests
`workspace.workspaceEdit`, `textDocument.codeAction` + `resolveSupport`, `rename`, hierarchical
`documentSymbol`, `semanticTokens`, and:

```rust
        "window": { "workDoneProgress": true },
        "experimental": { "snippetTextEdit": false, "serverStatusNotification": true },
        …
        "general": { "positionEncodings": [BYTE_ENCODING] }
```

with the rationale at `rust.rs:130-134`: *"Work-done progress and rust-analyzer's `serverStatus`
extension are both requested so the warm-up has something to show and something to stop on; without
the first the server sends no `$/progress` at all."* `DEFAULT_INDEXING_BUDGET_SECONDS = 600`
(`restructure_cli.rs:224`); registry idle timeout 600s.

**D/E. Consumers, not spawners** — `backends/lsp_bridge.rs:13-16` (`LspClientBridge { client:
Arc<LspClient> }`, blocking `request`/`notify`, plus `map_lsp_error` at `:56-69` turning `Timeout`
and `ContentModified` (-32801) into `ServerCatchingUp`); `backends/rust.rs:17` +
`runner.rs:17` (`RustBackend::from_lsp_client`, `runner::registry_for(client, indexing_budget,
progress)` at `runner.rs:207-220`).

**F. The bypass.** `RustBackend` has a second, self-spawned transport that does **not** go through
`LspRegistry` (`backends/rust.rs:770-800`): `Command::new(&self.binary)` with its own
`stdin`/`BufReader<stdout>`, `stderr(Stdio::null())`, and pinned toolchain env
(`CARGO_HOME`/`RUSTUP_HOME`/`RUSTUP_TOOLCHAIN`/`CARGO`/`RUSTC`). Selected by `if let Some(bridge) =
&self.bridge { … }` in `start` (`rust.rs:735-748`) — bridge wins when present, else self-spawn.
`runner.rs:197-204` uses the self-spawned form with a hard-coded `"/usr/bin/rust-analyzer"` for
static-only checks.

Two consequences: the **toolchain pinning** at `rust.rs:754-768` — with the explicit warning
*"Pinning is mandatory, not best-effort… the rustup proxy would channel-sync… a candidate for the
600s stall at `discovering sysroot`"* — exists **only** on the self-spawned path, and
`LspAllowList::rust_only()` sets no env at all; and the bridged path must fold progress in manually
because it never sees the stream (`rust.rs:823-829`):

```rust
            // The self-spawned transport folds progress in as it reads the stream; a bridged one
            // never sees the stream, so it collects what arrived and folds it in here. Without
            // this the whole load is silent and a timeout cannot say where the server got to.
            for notification in bridge.drain_notifications() {
                if let Some(line) = self.chatter.absorb(&notification) {
                    (self.progress)(&line);
                }
            }
```

`ServerChatter::absorb` (`rust.rs:344-357`) already folds `$/progress` +
`experimental/serverStatus`, tracking `quiescent`, `furthest` percentage and per-token titles
(`rust.rs:318-337`).

**Test-only spawners**: `packages/tddy-code-restructuring/tests/harness/mod.rs:230-258`
(`a_rust_analyzer_rooted_at`, *"launched the way `tddy-tools restructure` launches it"* — real
rust-analyzer); `packages/tddy-lsp/tests/client_roundtrip_test.rs:19-25`, `:308` (60s idle,
`fake_lsp`); `registry_reuse_test.rs`; `packages/tddy-lsp-executor/tests/e2e_test.rs:12-13`.

Advertisement-guard / doc references only, no spawn:
`packages/tddy-tools/tests/lsp_tool_advertisement_acceptance.rs:12`,
`packages/tddy-tool-engine/src/lib.rs:708`, `packages/tddy-core/src/session_actions/tool_gate.rs:15`.

#### Filesystem watching — nothing in the workspace

```bash
grep -rn "notify::\|RecommendedWatcher\|inotify\|FsEventWatcher\|notify_debouncer" --include="*.rs" packages/
```

**Exactly one hit, and it is a comment explaining the deliberate absence** —
`packages/tddy-core/src/usage_watcher.rs:14`:

```
//!   from disk each tick. Polling (not `notify`/inotify) keeps this dependency-free and robust to
```

```bash
grep -rn "notify\|notify-debouncer\|watchexec" --include="Cargo.toml" packages/
```
→ **zero hits.** The `notify` crate is not a dependency of this workspace at any version.

Existing precedent for the polling alternative: `usage_watcher.rs` (re-reads from disk each tick)
and the two 60s `tokio::time::interval` reaper loops.

#### `packages/tddy-lsp/Cargo.toml`

```toml
[dependencies]
tddy-task = { path = "../tddy-task" }
async-trait = "0.1"
bytes = "1"
log = "0.4"
lsp-types = "0.94"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tokio = { version = "1", features = ["rt", "sync", "time", "process", "io-util", "macros"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt", "rt-multi-thread", "macros", "time", "process", "sync", "io-util"] }
```

Plus `[[bin]] fake_lsp` at `tests/bin/fake_lsp.rs`, declared so `CARGO_BIN_EXE_fake_lsp` reaches the
`tests/` targets (re-declared in `tddy-lsp-executor`'s manifest to reuse it). `lsp-types` and `log`
are declared but unused in `src/`. No dependency on `tddy-build` or `tddy-core`, by design
(`lib.rs:7-9`).

### Findings

**Already built:** a stable-key reuse registry, `Clone` + `Send` + `Sync` over `Arc` interior state;
idle tracking, pull-based reaping, `shutdown_all`, crash respawn; a proven process-global
long-lived-registry pattern (`tddy_lsp_executor::register`) driven by a 60s reaper in both
`tddy-daemon` and `tddy-sandbox-app`; `$/progress` + `experimental/serverStatus` capture with a
256-entry backlog and a `ServerChatter` folder that already computes phase, furthest percentage and
quiescence; per-client adjustable request timeouts; a capable `client_capabilities()`.

**Missing or defective for a warm daemon:**

1. **No `didChange`/`didSave`/`didClose`/`didChangeWatchedFiles` on `LspClient`** — only `did_open`,
   hard-coded to `"version": 1`. The working `did_change` and its version counter live in
   `RustBackend` and die with each CLI process; they must move into `LspClient` with per-URI version
   state and an open-document set.
2. **No filesystem watcher and no `notify` dependency in the workspace at all.**
3. `drain_notifications` is destructive and single-consumer — concurrent requests plus a progress
   stream steal each other's notifications.
4. `get_or_spawn` releases the map lock before spawning ⇒ concurrent cold requests double-spawn and
   orphan a task.
5. The idle timer is refreshed only by `get_or_spawn`, not by client use — a long restructure can be
   reaped mid-flight by a host that runs a reaper.
6. Both production hosts pass `LspAllowList::rust_only()` (no capabilities ⇒ no code actions and
   **no `$/progress`**) and no toolchain-pinning env; the pinning that prevents the documented 600s
   `discovering sysroot` stall exists only on the self-spawned path.
7. `stderr` is piped but never drained (`server_body.rs:61`) — a blocking risk that grows with
   process lifetime.
8. `register_lsp_executor` is first-write-wins `OnceLock` — no replace, no unregister.
9. The stdout `broadcast` with `Lagged(_) => continue` can silently drop response frames under load.

## Exploration 3 — restructure + analysis CLI architecture (Explore agent)

### Sequence

Read `restructure_cli.rs`, `runner.rs`, `backends/{rust,lsp_bridge}.rs`, then every state-holding
module of `tddy-code-restructuring`; `analyze_cli.rs` and the `tddy-code-analysis` modules;
`tddy-tools/src/main.rs` for dispatch; both `lib.rs` files for the public surface; all three
`Cargo.toml`s. Repo-wide greps for `linkedProjects`, `process::exit`, `current_dir`, and for any
other crate depending on either library.

### Inspected files and excerpts

#### `restructure_cli.rs` (361 lines, 127 of them tests)

Subcommands (`:21`–`:95`), `RestructureArgs { command: RestructureCommand }`:

| Variant | Args struct | Fields |
|---|---|---|
| `Apply` (`:31`) | `RestructurePlanArgs` (`:42`) | positional `plan: PathBuf`, `--dry-run`, `--resume`, `--from <usize>`, `--stop-after <usize>`, `--indexing-budget <u64>` |
| `Status` (`:33`) | same | only `plan` is read (`:149`) |
| `Check` (`:35`) | `RestructureCheckArgs` (`:63`) | positional `plan`, `--deep`, `--budget <usize>`, `--indexing-budget` |
| `Anchors` (`:37`) | `RestructureAnchorsArgs` (`:80`) | positional `file`, `--items` (`value_delimiter = ','`), `--indexing-budget` |
| `Verify` (`:39`) | `RestructureVerifyArgs` (`:91`) | `--against <String>` only |

The whole per-invocation setup, `restructure_cli.rs:97-131`:

```rust
pub async fn run(args: RestructureArgs) -> Result<()> {
    let options = options_for(args);

    let client = if needs_lsp_client(&options) {
        let root = std::env::current_dir().context("current_dir")?;
        let task_registry = TaskRegistry::new();
        let lsp_registry = LspRegistry::new(
            restructure_allow_list(),
            task_registry,
            Duration::from_secs(600),
        );
        let key = LspKey { root, language: Language::Rust };
        let service = lsp_registry.get_or_spawn(key).await.context("rust-analyzer LSP")?;
        service.client.set_request_timeout(request_timeout(&options));
        Some(Arc::clone(&service.client))
    } else { None };

    tokio::task::spawn_blocking(move || crate::runner::dispatch(options, client))
        .await
        .context("restructure task join")?
        .map_err(anyhow::Error::msg)
}
```

- A brand-new `TaskRegistry` **and** `LspRegistry` per invocation (`:102`–`:107`); both are locals
  that drop at the end of `run`. The 600s idle timeout never matters because the process exits first.
- `LspKey` is `(current_dir, Language::Rust)` — **process cwd, not a parameter**.
- `needs_lsp_client` (`:226`–`:232`): `Apply | Anchors => true`, `Check => options.deep`,
  `Status | Verify => false`.
- `DEFAULT_INDEXING_BUDGET_SECONDS = 600` (`:224`).
- The runner runs inside `spawn_blocking` because the backend is synchronous and reaches the client
  through `Handle::current().block_on` (`lsp_bridge.rs:25`, `:42`). **A daemon inherits this
  constraint**: the multi-thread runtime must keep driving while a blocking thread calls `block_on`.

**Exit codes — the source MD's claim is false.** There is no `process::exit` anywhere in
`tddy-code-restructuring`. `runner::check` returns `Err(MalformedPlan)` on findings
(`runner.rs:404`–`:408`); `runner::verify` likewise (`runner.rs:481`–`:485`);
`restructure_cli.rs:130` flattens with `.map_err(anyhow::Error::msg)`;
`packages/tddy-tools/src/main.rs:156`–`:158` propagates with `?` into `main() -> Result<()>`
(`main.rs:127`), so `Termination` prints the error and **exits 1**. The only explicit code is
`main.rs:161`–`:162` → `exit(2)` for a missing subcommand. Existing acceptance tests pin
`failure()`/`success()` but not specific codes
(`packages/tddy-tools/tests/restructure_cli_acceptance.rs:30`, `:51`, `:91`).

Note also that `--budget` is deliberately **not** a gate: `runner.rs:398`–`:402` prints the report
before the findings check.

#### `runner.rs` (1261 lines, tests from `:876`)

Two entry points: `run(args: &[String], client)` (`:96`, legacy argv path, still public) and
**`dispatch(options: Options, client: Option<Arc<LspClient>>)`** (`:110`–`:118`) — the parsed seam
clap uses. `Options` (`:54`–`:73`) with `Default` (`:75`–`:91`) is the whole input surface:
`command, target, dry_run, resume, from, stop_after, indexing_budget, deep, budget, items, against`.

`registry_for` is **public** (`:207`–`:220`) — the single most useful existing entry point for a
daemon, since it builds a `BackendRegistry` that can stay warm across requests:

```rust
pub fn registry_for(
    client: Arc<LspClient>,
    indexing_budget: Option<u64>,
    progress: fn(&str),
) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    let mut rust = RustBackend::from_lsp_client(client, indexing_budget, progress);
    if wants_trace(std::env::var_os(TRACE_VARIABLE).as_deref()) {
        rust = rust.with_trace(report_trace);
    }
    registry.register(Box::new(rust));
    registry
}
```

`registry_for_static` (`:197`–`:204`) hard-codes `"/usr/bin/rust-analyzer", "/tmp", "/tmp"` — never
started, only `check()`/`supports()`/`module_references()` are called on it.

**`RESTRUCTURE_TRACE` does exist** (`:854`–`:862`), read once at registry construction (`:215`):

```rust
const TRACE_VARIABLE: &str = "RESTRUCTURE_TRACE";

fn wants_trace(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

fn report_trace(line: &str) { eprintln!("   trace: {line}"); }
```

The progress sinks are **bare `fn` pointers that print** (`:846`–`:852`):

```rust
fn report_progress(line: &str) { println!("   indexing: {line}"); }
fn report_progress_aside(line: &str) { eprintln!("   indexing: {line}"); }
```

`apply`/`check` use the stdout one; `anchors` uses stderr so stdout stays pure JSON (`:428`). Being
`fn` pointers rather than closures, **they cannot capture a per-request channel without a signature
change** — which is the concrete blocker for per-request progress streaming.

The apply loop (`:223`–`:296`) — per op: `--stop-after` → `break` with a message, not an error
(`:242`–`:248`); `ledger.translate_anchor(&op.anchor)?` (`:250`); `registry.backend_for(...)?
.resolve(&op.with_anchor(anchor), &Workspace { root: &root, overlay: &overlay })?` (`:251`–`:259`);
`report_visibility` (`:261`); dry run records into ledger + overlay and `continue`s without touching
disk (`:264`–`:273`); real run calls `commit_operation` (`:275`–`:283`). `commit_operation`
(`:488`–`:520`) is the write-ahead sequence: hash → journal `in_flight` → `apply_workspace_edit` →
hash → journal `completed` → `ledger.record` → `LedgerCheckpoint::write`.

**Everything is fatal and short-circuits with `?`** — `crate::Result<T> = Result<T,
RestructureError>` (`lib.rs:79`), no per-op error capture, no partial-failure summary.

**cwd is consumed at seven sites**: `runner.rs:226` (apply), `:321` (status), `:350` (check), `:419`
(anchors), `:454` (verify), plus `restructure_cli.rs:101`. **Nothing takes a root parameter.** This
is the single largest structural blocker to a multi-workspace daemon.

#### `backends/rust.rs` (6434 lines) — budgets and the `indexed` flag

`:451`–`:470`:

```rust
const WARMUP_BUDGET: Duration = Duration::from_secs(600);
const SETTLE_BUDGET: Duration = Duration::from_secs(30);

fn settle_budget_for(warmup: Duration) -> Duration {
    std::cmp::max(SETTLE_BUDGET, warmup / 20)
}

const INDEXING_POLL: Duration = Duration::from_secs(2);
const SETTLE_POLL: Duration = Duration::from_millis(200);
const CONTENT_MODIFIED: i64 = -32801;
const CONTENT_MODIFIED_RETRIES: u32 = 30;
```

**There is no 900s constant** — `900` appears only as a user-supplied `--indexing-budget` in tests
and in the TODO entry. The cascade: 600s warm-up → `settle = max(30s, 600/20) = 30s`. A caller
passing `--indexing-budget 900` gets `settle = 45s`, which is the observed "46s" refusal.

The `indexed` flag (`:544`–`:546`):

```rust
    /// Whether the crate graph has been observed loaded. Set once, and never unset: the graph does
    /// not unload, so every later wait is a settle rather than an index.
    indexed: bool,
```

Read by `resolution_budget` (`:2043`–`:2049`):

```rust
    fn resolution_budget(&self) -> Duration {
        if self.indexed { self.settle } else { self.warmup }
    }
```

Set `false` in `new` (`:668`) and `from_lsp_client` (`:717`); set `true` in exactly two places —
`ensure_indexed` (`:2023`) and `wait_until_resolved` (`:2066`). **Per-process, per-`RustBackend`.**

`ensure_indexed` (`:2002`–`:2035`) is the hover-probe warm-up: request `documentSymbol`, take the
first symbol's position, then loop `textDocument/hover` at `INDEXING_POLL` until the hover is
non-null **or `self.chatter.quiescent`**, bounded by `self.warmup`; on expiry it returns
`RestructureError::IndexingIncomplete { environment, seconds, last: chatter.how_far() }`. Blocking
`std::thread::sleep` inside the `spawn_blocking` thread. Called from `anchor_for` (`:1126`),
`resolve` (`:1185`), `outside_references` (`:1242`).

`request_settled` (`:860`–`:871`) retries `ServerCatchingUp` 30 × 200 ms = 6s, and
`lsp_bridge.rs:66` maps `LspError::Timeout` into `ServerCatchingUp` — so a whole
`set_request_timeout` expiry consumes one of the 30 retries.

The authors documented the failure mode themselves at `rust.rs:463`:

> "Which is seconds" holds for a file of ordinary size and fails badly on a very large one: an edit
> to an 18,000-line module in a workspace this size takes rust-analyzer well past thirty seconds to
> re-resolve, and the run then reports an incomplete index for a server that was working normally.
> So this is the *default*, and [`settle_budget_for`] scales it when a caller has said how long it
> is willing to wait.

`server_settings()` (`:208`–`:215`) is the entire server config — four import lines:

```rust
pub fn server_settings() -> Value {
    json!({
        "imports": {
            "granularity": { "group": "crate", "enforce": true },
            "merge": { "glob": false }
        }
    })
}
```

**`linkedProjects` appears nowhere in the repo** (grepped `*.rs`, `*.toml`, `*.md`, `*.json` — zero
occurrences). Nothing sets `cargo.targetDir`, `checkOnSave`, `procMacro`, `cachePriming` or
`numThreads` either.

The progress-sink doc comment (`rust.rs:529`–`:535`) **anticipates this exact change**:

```rust
    /// Where a progress line goes. Every other consequence of an operation travels back to the
    /// caller inside a [`Resolution`], but progress happens *while* a call is in flight and has
    /// nowhere to wait — so it needs a sink rather than a return value. It stays a sink rather than
    /// a `println!` because this library is not the only possible front end: anything that speaks a
    /// protocol on stdout, a persistent server most obviously, would have its stream corrupted by an
    /// engine writing progress into it. Silent by default, and the binary is what makes it visible.
    progress: fn(&str),
```

`ServerChatter` (`:316`–`:449`) holds `titles`, `last`, `shown`, `quiescent`, `furthest`; `absorb`
(`:345`) folds `$/progress` and `experimental/serverStatus`; `how_far()` (`:418`) builds the
`last; furthest <phase> N%` string `IndexingIncomplete` reports.

#### The state-holding modules

| Module | Responsibility | Mutable state | Lives where |
|---|---|---|---|
| `plan.rs` (653) | Immutable command log. `Plan::parse` (`:176`) reads the JSONL snapshot header + ops; refuses schema mismatch and any op carrying `text`/`code`/`content`. `verify_snapshot` (`:199`–`:211`) re-hashes and errors `SnapshotMismatch`. 14 `RefactorKind` variants (`:45`–`:97`), 8 `SUPPORTED` (`rust.rs:38`–`:47`) | **none** | disk, caller path |
| `journal.rs` (594) | Append-only event log; `append` (`:130`–`:153`) does `writeln!` + `flush()` + **`sync_all()`**; `fold`/`fold_through` (`:157`, `:162`); `next_op` (`:184`); `resume_decision` (`:192`–`:214`) → `ReRun`/`MarkCompleted`/`Abort` | `Journal { pub records: Vec<JournalRecord> }` (`:100`) | `.restructure/journal.jsonl` |
| `ledger.rs` (489) | Snapshot coords → on-disk coords, repo-wide. `translate` (`:73`), `record` (`:96`), `rename` (`:110`), `translate_anchor` (`:119`), `current_path` (`:139`); `LedgerCheckpoint` (`:35`) cross-checked against `fold_through` → `CheckpointDivergence` | `PositionLedger { files: BTreeMap<..>, renames: BTreeMap<..> }` (`:17`–`:20`) | RAM + `.restructure/ledger.json` |
| `overlay.rs` (212) | What the tree *would* hold during a no-write run. `read` (`:34`) falls through to disk; `record` (`:46`) applies an edit in memory | `Overlay { files: BTreeMap<PathBuf, String> }` (`:19`–`:21`) | **RAM only, no disk form** |
| `apply.rs` (378) | The **only** module touching the filesystem. `apply_workspace_edit` (`:18`): Creates (`git add -N`) → Changes → Renames via **`git mv`**, no `fs::rename` fallback. `ensure_git_worktree` (`:38`), `hash_touched_files`/`hash_file` (`:56`, `:66`), `edited` (`:108`) | none | working tree + git |
| `verify.rs` (193) | Statement-multiset comparison against a git ref. `statements` (`:43`), `compare` (`:71`) → `Comparison { before, after, missing, added }` | none (pure) | — |
| `registry.rs` (285) | Language dispatch + the `LanguageBackend` trait (`:35`–`:96`). `Workspace<'a> { root, overlay }` (`:23`). `backend_for` (`:115`–`:141`) takes **`&mut self`** and hard-errors `UnsupportedOp` | `BackendRegistry { backends: Vec<Box<dyn LanguageBackend>> }` (`:98`) — transitively owns **all live LSP session state** | RAM only |
| `edit.rs` (87) | Edit vocabulary: `Position` (1-based), `Range`, `TextEdit`, `FileEdit::{Change,Create,Rename}`, `WorkspaceEdit`, `VisibilityChange`, `Resolution` | none | — |
| `crate_move.rs` (1608) | The one cross-crate op, engine-*informed* not engine-performed. `ModuleReferences` trait (`:69`), `Destination::read` (`:120`), `survey` (`:205`), `resolve` (`:236`), `facade_line` (`:999`) | none persistent | — |

`.restructure/` state paths, `runner.rs:797`–`:820`:

```rust
struct StatePaths { journal: PathBuf, ledger: PathBuf }

impl StatePaths {
    fn under(root: &Path) -> Self {
        let dir = root.join(".restructure");
        Self { journal: dir.join("journal.jsonl"), ledger: dir.join("ledger.json") }
    }

    fn ensure_self_ignoring(&self) -> Result<()> {
        let dir = self.journal.parent().expect("state paths live in a directory");
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(".gitignore"), "*\n")?;
        Ok(())
    }
}
```

`open_run` (`:771`–`:788`) is the only concurrency gate that exists: `ensure_git_worktree`,
`ensure_self_ignoring`, `Journal::load`, then **`JournalExists` unless `--resume`/`--from`**. One
journal per root, **no lock file and no plan identity in the path** — two daemon clients applying
different plans to the same root collide on `.restructure/journal.jsonl`.

**Private, so a daemon reimplementing the apply loop must promote them**: `StatePaths` (`:797`),
`open_run` (`:771`), `restore_ledger` (`:790`), `commit_operation` (`:488`), `Rehearsal` (`:522`),
`registry_for_static` (`:197`), `files_named_by`/`measured`/`over_budget`/`budget_report`,
`survey_lines`.

#### `analyze_cli.rs` (307 lines, tests from `:219`)

| Variant | Args | Fields |
|---|---|---|
| `Coverage` (`:25`) | `AnalyzeCoverageArgs` (`:32`) | `--path` (required), `--coverage-dir` (default `./coverage`) |
| `Report` (`:27`) | `AnalyzeReportArgs` (`:43`) | `--path`, `--coverage-dir`, both required |
| `DuplicateTests` (`:29`) | `AnalyzeDuplicateTestsArgs` (`:54`) | `--coverage-dir`, `--out` (default `<coverage-dir>/duplicate-tests`), `--min-signature` (5), `--subset-ratio` (0.5), `--include-test-sources` |

`pub fn run(args: AnalyzeArgs) -> Result<()>` (`:74`) is **synchronous — no `async`, no tokio**.
Three thin delegations to `coverage::capture_coverage`, `report::generate_report`,
`report::generate_duplicate_tests_report`.

The only state is `ProgressRenderer` (`:100`–`:152`) — `{ started: Instant, interactive: bool,
painted: usize }`, a stderr renderer. **The library itself never prints**, and `capture_coverage`
takes `&mut dyn FnMut(CaptureProgress<'_>)` (`coverage.rs:128`–`:132`) — **a real closure sink,
unlike the restructure backend's `fn` pointers, so it is already daemon-friendly.**

**No LSP, no tokio** — `tddy-code-analysis`'s deps are `anyhow, clap, md5, proc-macro2, serde,
serde_json, syn, thiserror, walkdir, which`. Grepping `lsp|Lsp` across `src/*.rs` returns nothing.
It is purely `syn`-based plus subprocess-driven (`cargo test --no-run`, `llvm-profdata`, `llvm-cov`).

**No in-process cache of any kind.** The only reuse is incidental and on disk:
`instrumented_build_dir(&crate_name)` + `CARGO_TARGET_DIR` (`coverage.rs:418`–`:426`), so cargo's
own incrementality is the whole "cache". `capture_coverage` always re-runs the full build and every
test — no `exists()` short-circuit, no mtime check, no skip logic. Per-test `.profraw`/`.profdata`
go to `std::env::temp_dir()` keyed by artifact id (`:192`–`:195`).

`complexity::file_complexity(source) -> Vec<FunctionComplexity>` is **pure over a `&str`** — it
caches trivially by content hash.

#### `tddy-tools` dispatch

`packages/tddy-tools/src/main.rs:92`–`:96`:

```rust
    /// Rust code analysis: coverage, CRAP report, duplicate-tests.
    Analyze(tddy_code_analysis::analyze_cli::AnalyzeArgs),

    /// Plan-driven Rust refactoring via rust-analyzer (through tddy-lsp).
    Restructure(tddy_code_restructuring::restructure_cli::RestructureArgs),
```

`:155` → `Some(Subcommand::Analyze(s)) => tddy_code_analysis::analyze_cli::run(s)?,`
`:156`–`:158` → `Some(Subcommand::Restructure(s)) => { tddy_code_restructuring::restructure_cli::run(s).await? }`

`#[tokio::main] async fn main() -> Result<()>` at `:126`–`:127`; 24 sibling subcommands.
**Neither restructure nor analyze is exposed through the MCP server** (`server.rs`, 3369 lines) —
they are CLI-only. `tddy-tools` and the crates themselves are the only workspace consumers of
either library.

#### Public library surface

`tddy-code-restructuring/src/lib.rs` (79 lines) — **every module is `pub`**; nothing is hidden.
Re-exports `client_capabilities`, `server_settings`, the `crate_move` types, the whole `edit`
vocabulary, `Journal`/`JournalRecord`/`OpStatus`, `LedgerCheckpoint`/`PositionLedger`, `Overlay`,
`Anchor`/`Plan`/`Reexport`/`RefactorKind`/`RefactorOp`, `BackendRegistry`/`LanguageBackend`.
`RestructureError` (`:31`–`:77`) has 12 variants: `MalformedPlan`, `CodeTextInPlan{field}`,
`SnapshotMismatch{path,expected,actual}`, `AnchorInvalidated{path}`, `NoBackend{extension}`,
`UnsupportedOp{backend,op}`, `NotAGitWorktree{path}`, `IndeterminateJournal{op}`,
`CheckpointDivergence{op}`, `JournalExists`, `ServerCatchingUp`,
`IndexingIncomplete{seconds,last,environment}`, `Io`.

`tddy-code-analysis/src/lib.rs` (11 lines) — all modules `pub`, every entry point cwd-free and
path-taking: `coverage::capture_coverage(crate_path, coverage_dir, &mut progress)`,
`complexity::file_complexity(source)`, `crap::{crap_score, join_rust_function_metrics}`,
`duplicate_tests::{analyze_duplicates, analyze_coverage_dir, signature_for_rust_test}`,
`report::{generate_report, generate_duplicate_tests_report}`.

#### Cargo manifests

`tddy-code-restructuring`: `anyhow, clap(derive), serde(derive), serde_json, sha2, thiserror 2,
tddy-lsp, tokio{rt,sync,time,macros}, tddy-task`; dev-deps add `tempfile, pretty_assertions, rstest`
and tokio `rt-multi-thread` — needed because `Handle::current().block_on` only progresses from a
blocking thread while another thread drives the runtime. **A daemon inherits that constraint.**

`tddy-code-analysis`: `anyhow, clap(derive), md5, proc-macro2(span-locations), serde(derive),
serde_json, syn 2{full,visit}, thiserror 2, walkdir, which`. No tokio, no tddy-lsp, no tddy-task.

`tddy-tools`: 30+ deps including `tddy-rpc`, `tddy-stdio`, `tddy-terminal-rpc`, `tddy-service`,
`prost`, `rmcp`, `tokio{full}`. **It does not depend on `tddy-lsp` directly** — only transitively
through `tddy-code-restructuring` and `tddy-lsp-executor`. It declares two `[[bin]]` targets, the
second being the test fixture recorded in
[`2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md`](../todo/2026-09-10-the-execute-tool-stdio-fixture-bin-forces-three-dev-deps-into-dependencies.md).

### Findings

1. **cwd, seven times.** Nothing in the runner takes a workspace root; a daemon serving two roots
   cannot use `runner::dispatch` as-is.
2. **`progress: fn(&str)` cannot carry per-request state**, and the doc comment says a persistent
   server is exactly the case it was left a sink for.
3. **`.restructure/` is keyed by root with no lock and no plan identity**, and `open_run` refuses a
   second plan with `JournalExists`.
4. **`indexed` is per-`RustBackend`**, so keeping a warm `BackendRegistry` (via the already-public
   `runner::registry_for`) is what actually amortizes the index — not keeping the client alone.
5. **The budget defect is `settle_budget_for` returning `warmup/20`**, and a warm daemon runs every
   request on that path.
6. **The exit-code premise in the source MD is false** — refusals already exit 1.
7. **`tddy-code-analysis` is already daemon-shaped** (sync, path-taking, closure progress sink) and
   has no caching at all, so its warm-state opportunity is entirely greenfield.

## Exploration 4 — gRPC + stdio dual-transport house pattern (Explore agent)

### Sequence

`find . -name '*.proto'`; read `tddy-codegen`'s three source files and `docs/tonic-adapter.md`;
read `docs/ft/coder/{rpc-multi-transport,grpc-remote-control}.md` in full; read `tddy-rpc`,
`tddy-stdio`, `tddy-connectrpc`, `tddy-connectrpc-testkit`, `tddy-terminal-rpc`, `tddy-service`;
trace `tddy.v1.TddyRemote` and `sandbox.SandboxService` end to end; read `tddy-daemon`'s `main.rs`,
`runtime.rs`, `local_socket_server.rs`, `tddy-supervisor`'s `lib.rs`/`server.rs`; read `install`,
`run-livekit-testkit-server`, `packages/tddy-livekit-testkit`.

### Inspected files and excerpts

#### Proto layout — 64 files across 7 roots

| Root | Contents |
|---|---|
| `packages/tddy-service/proto/` | 40 protos — the shared service catalog |
| `packages/tddy-terminal-rpc/proto/terminal_session.proto` | a crate that **owns and serves** its own proto |
| `packages/tddy-supervisor/proto/supervisor.proto` | the privileged surface, deliberately isolated |
| `packages/tddy-rpc/proto/rpc_envelope.proto` | the wire envelope shared by every transport |
| `packages/tddy-livekit/proto/` | envelope/reflection/echo copies |
| `packages/tddy-build/proto/tddy/build/v1/` | messages only |
| `packages/tddy-workflow-recipes/proto/` | `package tddy.workflow`, messages only |

**The house rule for a new crate-owned service is `tddy-terminal-rpc`, not `tddy-service`.**
`packages/tddy-terminal-rpc/src/service.rs:528`–`:534`:

```rust
pub const TERMINAL_SESSION_SERVICE: &str = "terminal_session.TerminalSessionService";
```

> "It lives in this crate rather than in `tddy-service` … because this crate owns
> `terminal_session.proto`, is the only crate that *serves* it … and is already a dependency of
> every caller that addresses it."

`packages/tddy-service/src/service_coordinates.rs:1`–`:18`: only coordinates whose *served* and
*addressed* ends sit in **different** crates get published there.

#### Proto package naming — bare snake, not versioned

Two families exist and the versioned one is the minority: bare lower-snake (`package host;`,
`package worktree;`, `package session;`, `package terminal_session;`, `package supervisor;`) versus
`tddy.v1`, `tddy.acp.v1`, `tddy.build.v1`. Coordinate = `<package>.<Service>`.

[`2026-09-09-versioned-proto-package-names.md`](../todo/2026-09-09-versioned-proto-package-names.md)
forbids introducing one now:

> Worth doing as its own change, across all protos at once, after the `#unbundle` stack lands — **a
> partial rename is worse than none, because it makes the convention unreadable.**

→ A new proto uses a **bare snake package**.

#### `packages/tddy-codegen` — a `prost_build::ServiceGenerator`, not a macro

Three files, 1404 lines. `src/lib.rs` is 7 lines. **No generator binary and no
`generate_tonic_adapter!` macro** — `packages/tddy-codegen/docs/tonic-adapter.md` explains:

> "`#[tonic::async_trait]` rewrites the signatures of the trait it is applied to. A declarative
> macro cannot see through that rewrite to produce the bodies, so there is no `macro_rules!` that
> stands in for this."

`src/config.rs:9`–`:26` — the four knobs:

```rust
pub struct TddyServiceGenerator {
    /// Generate the RpcService server struct with per-method handler structs.
    pub generate_rpc_server: bool,
    /// Generate the tonic adapter that wraps an implementation of the generated trait.
    pub generate_tonic_adapter: bool,
    /// Crate path for RPC types (e.g. `"tddy_rpc"`).
    pub rpc_crate_path: String,
    /// Module path of the tonic server trait the adapter should implement, as tonic-build emitted
    /// it — e.g. `"crate::tonic_worktree::worktree_service_server"`.
    pub tonic_trait_path: Option<String>,
}
```

What it emits (`generator.rs:8`–`:51`):

1. Always: `#[async_trait] pub trait <Service>` with tonic-mirrored signatures but
   `tddy_rpc::{Request,Response,Streaming,Status}` wrappers (`:85`–`:135`).
   **This trait is the in-process seam** — its methods take and return prost message structs inside
   `tddy_rpc::Request`/`Response`, so a caller in the same process passes Rust values with **no
   encode/decode at all**.
2. `generate_rpc_server` → `<Service>Server<T: <Service>> { inner: Arc<T> }` with
   `pub const NAME: &'static str = "<proto.package>.<Service>"` (`:218`–`:223`), `new`, and
   `from_arc` (`:229`–`:241`, doc: *"Build from an already-shared handle, so the same implementation
   instance can be served over more than one transport"*), plus
   `impl RpcService for <Service>Server<T>` whose `handle_rpc` answers
   `Status::not_found("Unknown service: {}")` on a coordinate mismatch (`:537`–`:589`) and an
   `is_bidi_stream` that `matches!` only the bidi method names (`:554`–`:573`).
3. `generate_tonic_adapter` **and** `tonic_trait_path` → `<Service>TonicAdapter<T>` holding `Arc<T>`
   plus a delegating `impl <service>_server::<Service>`. Refusals convert through
   `tddy_service::to_tonic_status` / `to_rpc_status` — **not configurable, deliberately**, because
   `tddy-rpc` pins tonic 0.11 against everything else's 0.12. This is the property recorded in
   [`2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md`](../todo/2026-09-12-generate-tonic-adapter-hardcodes-its-status-conversion-path.md);
   it only bites a crate emitting into `tddy-service`'s own `OUT_DIR`.

#### The canonical `build.rs` — `packages/tddy-terminal-rpc/build.rs`, in full

`tonic-adapter.md` calls it "the worked example of both passes over one proto":

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // RpcService-flavored pass: async trait + RpcService server for LiveKit/tddy-rpc.
    prost_build::Config::new()
        .out_dir(std::env::var("OUT_DIR")?)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: true,
            rpc_crate_path: "tddy_rpc".to_string(),
            tonic_trait_path: Some(
                "crate::proto::tonic_terminal_session::terminal_session_service_server".to_string(),
            ),
        }))
        .compile_protos(&["proto/terminal_session.proto"], &["proto"])?;

    // Tonic gRPC server/client, reusing the prost message types above so both `TerminalSessionService`
    // trait impls (tonic and RpcService) operate on identical Rust types. `src/lib.rs` includes
    // this output — without that include the adapter's `tonic_trait_path` would not resolve and the
    // gRPC server trait would be generated but unreachable.
    let tonic_dir = format!("{}/tonic_terminal_session", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&tonic_dir)?;
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&tonic_dir)
        .extern_path(".terminal_session", "crate::proto::terminal_session")
        .compile_protos(&["proto/terminal_session.proto"], &["proto"])?;

    let descriptor_path = format!("{}/terminal_session_descriptors.bin", std::env::var("OUT_DIR")?);
    let descriptor_scratch = format!("{}/descriptor_set_only", std::env::var("OUT_DIR")?);
    std::fs::create_dir_all(&descriptor_scratch)?;
    prost_build::Config::new()
        .file_descriptor_set_path(&descriptor_path)
        .out_dir(&descriptor_scratch)
        .compile_protos(&["proto/terminal_session.proto"], &["proto"])?;

    Ok(())
}
```

`[build-dependencies]` (`packages/tddy-terminal-rpc/Cargo.toml:48`–`:51`):

```toml
prost-build = "0.13"
tddy-codegen = { path = "../tddy-codegen", features = ["tonic"] }
tonic-build = "0.12"
```

`src/lib.rs:17`–`:42` includes both modules (the tonic one under
`#![allow(unused_imports, clippy::all)]`), and `:59`–`:63`:

```rust
pub static TERMINAL_SESSION_DESCRIPTOR_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/terminal_session_descriptors.bin"));
```

`packages/tddy-service/build.rs` (724 lines) is the same pattern ×30, including **stdio-only**
variants with no tonic pass (`:105` remote_git — *"Bidi-only… No tonic pass — git never reaches the
daemon over gRPC"*; `:543` daemon_config — *"No tonic pass — the daemon's own settings are never
served over gRPC"*).

#### The multi-transport contract

`docs/ft/coder/rpc-multi-transport.md` (✅ Implemented 2026-07-01), § Requirements:

> 1. **Same user code, either transport.** Code that calls or serves RPCs depends on
>    transport-agnostic types (`RpcService` for serving; a client trait for calling) — not on
>    `livekit::Room` or any stdio-specific type.
> 2. **Multiplexed.** Many concurrent RPC calls (unary, server-streaming, client-streaming, bidi)
>    run over one stdio pipe pair, correlated by request id…
> 3. **Bidirectional.** Either peer on the stdio pipe can be the caller…

Two behavioural rules a new server must honour: **real-time streaming** (items forwarded
immediately, never peeked one ahead — *"a real-time bidi producer may only emit its next item after
the peer reacts to the current one, so peeking ahead would deadlock"*), and **backpressure, not
drop** (`on_response` must `.send().await`, not `try_send`; regression test
`packages/tddy-rpc/src/client_engine.rs::delivers_every_stream_item_even_when_the_consumer_drains_after_a_large_burst`).

The doc also states the repo's no-dual-path rule: *"per this repo's convention there is no dual-path
fallback — each call site's old transport is deleted once it moves to stdio."*

`docs/ft/coder/grpc-remote-control.md` (Stable) — the flag contract:

> `--stdio` runs *concurrently* with `--grpc` when both are passed (they're independent transports
> onto the same `PresenterHandle`) but is exclusive with local TUI/plain-mode dispatch, since fd 1
> can't be shared between RPC framing and terminal rendering.

and the three requirements a `--stdio` binary must meet:

> **Stdio-safe core** (`tddy_core::stdio_safety`): before serving, force-overrides any
> `LogOutput::Stdout` logger destination to stderr and redirects real stderr to a log file
> (`redirect_fd_to_file`) — stdin/stdout stay live for RPC framing.

Note the tension worth deciding: `grpc-remote-control.md` records *"duplicated, not delegated"* as
the established pattern — but that is the **hand-written** one. `tddy-codegen`'s generated adapter
**does** delegate, and `tonic-adapter.md` names the two remaining hand-written adapters as debt.
A new service uses the generated adapter.

#### Crate responsibilities

| Crate | "Serve this service over…" |
|---|---|
| `tddy-rpc` | nothing — it *defines* `RpcService`/`ServiceEntry`/`RpcBridge`/`RpcClientTransport`, the envelope and the frame codec |
| `tddy-stdio` | **stdio / any duplex byte channel** — `StdioEndpoint::{from_process_stdio, from_duplex}`, `spawn_child_endpoint` |
| `tddy-connectrpc` | **HTTP/Connect** at `POST /rpc/{service}/{method}` — `connect_router` |
| `tddy-livekit` | **LiveKit data channels** |
| tonic directly + `<Service>TonicAdapter` | **gRPC** — there is no `tddy-grpc` crate; a host calls `tonic::transport::Server::builder().add_service(...)` itself |
| `tddy-terminal-rpc` | one *service*, over all of the above, from one `Arc` |
| `tddy-service` | the protos, the impls, the refusal conversion, reflection |
| `tddy-connectrpc-testkit` | **TypeScript only** — in-memory Connect backend for bun/Cypress |

The serving trait, `packages/tddy-rpc/src/bridge.rs:31`–`:69`:

```rust
#[async_trait]
pub trait RpcService: Send + Sync + 'static {
    fn is_bidi_stream(&self, _service: &str, _method: &str) -> bool { false }
    async fn handle_rpc(&self, service: &str, method: &str, message: &RpcMessage) -> RpcResult;
    async fn handle_rpc_stream(&self, service: &str, method: &str, messages: &[RpcMessage]) -> RpcResult { ... }
    async fn start_bidi_stream(&self, _service: &str, _method: &str, _input_rx: mpsc::Receiver<RpcMessage>)
        -> Result<BidiStreamOutput, Status> { Err(Status::unimplemented("bidi streaming not supported")) }
}
```

The registration unit, `bridge.rs:71`–`:81`:

```rust
pub struct ServiceEntry {
    pub name: &'static str,
    pub service: Arc<dyn RpcService>,
}

pub struct MultiRpcService { entries: Vec<ServiceEntry> }
```

The stdio serving API, `packages/tddy-stdio/src/endpoint.rs`:

```rust
pub struct StdioEndpoint<S: RpcService> { ... }                                   // :30
    pub fn from_process_stdio(service: S) -> (Arc<StdioRpcClient>, Self)          // :76
    pub fn from_duplex(reader: impl AsyncRead…, writer: impl AsyncWrite…, service: S)
        -> (Arc<StdioRpcClient>, Self)                                            // :91
    pub async fn run(self)                                                        // :109
pub async fn spawn_child_endpoint<S: RpcService>(command: Command, service: S)
    -> std::io::Result<ChildEndpoint>                                             // :204
```

One writer task owns the write half and **flushes after every frame** (`:135`–`:148`, because *"a
peer's own read loop may itself be blocked awaiting a response this write carries"*);
`OUTGOING_QUEUE_CAPACITY = 256` *"so a slow peer applies backpressure"*.

**There is no `tddy-*-testkit` crate for testing a Rust-served service** — `tddy-connectrpc-testkit`
is TypeScript. The four Rust patterns are: in-process dispatch at the registered coordinate; two
`StdioEndpoint::from_duplex` halves over `tokio::io::duplex`; spawning the real binary with
`spawn_child_endpoint`; and tonic over an ephemeral port
(`packages/tddy-service/src/lib.rs:589`–`:620`, `mod test_util { pub async fn spawn_server(router) }`
binding `[::1]:0`).

#### The end-to-end worked example — `tddy.v1.TddyRemote` on `tddy-coder`

The gRPC branch, `packages/tddy-coder/src/run.rs:3456`–`:3486`:

```rust
if let Some(port) = args.grpc {
    let handle = tddy_core::PresenterHandle { event_tx: event_tx.clone(), intent_tx: intent_tx.clone() };
    let service = tddy_service::TddyRemoteService::new(handle);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
        let result: anyhow::Result<()> = rt.block_on(async move {
            let addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
            let listener = tokio::net::TcpListener::bind(addr).await?;
            log::info!("gRPC server listening on port {}", port);
            tonic::transport::Server::builder()
                .add_service(tddy_service::gen::tddy_remote_server::TddyRemoteServer::new(service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await.map_err(anyhow::Error::from)
        });
        result.expect("gRPC server failed")
    });
}
```

The stdio branch, `run.rs:3774`–`:3800`:

```rust
if args.stdio {
    // `--stdio` dedicates this process's real stdin/stdout to RPC framing (see
    // `tddy_core::stdio_safety`) — serve the same remote-control surface as `--grpc`, but
    // never touch physical fd 1 for TUI rendering (no `run_event_loop`/crossterm here).
    let service = tddy_service::proto::remote::TddyRemoteServer::new(
        tddy_service::TddyRemoteService::with_view_factory(view_factory.clone()),
    );
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    rt.block_on(async move {
        let (_client, endpoint) = tddy_stdio::StdioEndpoint::from_process_stdio(service);
        endpoint.run().await;
    });
    shutdown.store(true, Ordering::Relaxed);
}
```

The stdio-safety preamble, `run.rs:348`–`:354` and `:374`–`:381`:

```rust
if args.stdio || args.acp {
    // --stdio and --acp both dedicate fd 1 to RPC framing (line-delimited RPC / ACP JSON-RPC);
    // a misconfigured `output: stdout` logger would corrupt the stream. Must run before
    // init_tddy_logger — log::set_logger only succeeds once.
    tddy_core::stdio_safety::enforce_stdio_safe_log_output(&mut log_config);
}
...
#[cfg(unix)]
if args.stdio {
    let _ = tddy_core::stdio_safety::redirect_fd_to_file(libc::STDERR_FILENO, &logs.join("stdio_stderr.log"));
}
```

Flags, `run.rs:625`–`:638`:

```rust
/// Start gRPC server alongside TUI for programmatic remote control (e.g. --grpc 50052)
#[arg(long, value_name = "PORT", default_missing_value = "50051")]
pub grpc: Option<u16>,

/// Serve the remote-control surface over stdin/stdout (RPC over stdio) instead of --grpc's
/// TCP socket. No local TUI is rendered.
#[arg(long)]
pub stdio: bool,
```

**The acceptance test is the template to copy** —
`packages/tddy-e2e/tests/stdio_remote_control_acceptance.rs` (233 lines, 2 tests):

- `const CALL_TIMEOUT: Duration = Duration::from_secs(8);` with its rationale (`:21`–`:24`).
- `struct NoCallbackService` implementing `RpcService` with
  `Status::unimplemented(format!("test process hosts no callback service, got {service}/{method}"))`
  (`:26`–`:37`) — *"any inbound request here would be a bug, so it fails loudly rather than silently
  no-op'ing."* Copy-pasted in ≥6 suites.
- `fn tddy_coder_exe_path()` with the `CARGO_BIN_EXE_*`-then-`current_exe()` fallback (`:39`–`:50`).
- `submits_a_feature_input_over_stdio_and_receives_a_goal_started_event` (`:88`–`:139`).
- **`serves_grpc_and_stdio_concurrently_from_the_same_process`** (`:141`–`:233`) — subscribes both
  channels first *"so both observe the same broadcast GoalStarted event rather than racing a late
  subscriber against it"*, submits over stdio only, asserts the event arrives on both; retries the
  tonic connect for a bounded window because *"the gRPC listener binds on its own background
  thread."*

The `tokio::io::duplex` harness, `packages/tddy-tools/tests/session_tool_streaming_dispatch.rs:64`–`:78`:

```rust
let (client_side, server_side) = tokio::io::duplex(256 * 1024);
let (server_read, server_write) = tokio::io::split(server_side);
let (_unused, server_endpoint) = tddy_stdio::StdioEndpoint::from_duplex(server_read, server_write, FramePlayingExecuteTool { frames });
tokio::spawn(server_endpoint.run());
let (client_read, client_write) = tokio::io::split(client_side);
let (client, client_endpoint) = tddy_stdio::StdioEndpoint::from_duplex(client_read, client_write, NoCallbackService);
tokio::spawn(client_endpoint.run());
client
```

#### The coordinate-integrity trio

`packages/tddy-terminal-rpc/tests/terminal_session_service_acceptance.rs:1`–`:80`:

```rust
//! Every test dispatches at the *registered coordinate* —
//! `entry.service.handle_rpc("terminal_session.TerminalSessionService", "<Method>", …)` with an
//! encoded request, decoding the answer back — rather than calling the handler behind it. The proto
//! was extracted into this crate and then never served: nine methods that compile prove nothing
//! about whether a client can reach any of them.
```

1. `names_the_service_the_wiring_layer_registers` — literal string equality.
2. `registers_at_the_coordinate_its_generated_server_answers_to` — against `ServedCoordinate::NAME`.
3. `publishes_the_coordinate_its_schema_declares` — reads the `.proto` off disk, extracts `package`
   + `service`, asserts `format!("{package}.{service}")` matches the published constant.

And the registration function, `packages/tddy-terminal-rpc/src/service.rs:536`–`:548`:

```rust
#[must_use]
pub fn build_terminal_session_entry(ports: TerminalSessionPorts) -> tddy_rpc::ServiceEntry {
    let server = TerminalSessionServiceServer::new(TerminalSessionServiceImpl::new(ports));
    tddy_rpc::ServiceEntry {
        name: TERMINAL_SESSION_SERVICE,
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}
```

#### Daemon conventions

`packages/tddy-daemon/src/main.rs` (189 lines). Header (`:1`–`:9`): *"The bootstrap itself lives in
`tddy_daemon::runtime`: this binary is one of its hosts… What is left here is what only a binary
owns — the argument parsing, the pre-tokio fork of the spawn worker, the HTTP listener, and the
signals a service manager sends it."*

Ordered startup (`:31`–`:105`): ignore `SIGPIPE`; `Args::parse()` (only `--config`, with
`env = "TDDY_DAEMON_CONFIG"`, and `--relay`); `DaemonConfig::load(config_path)?` — **`--config` is
required**; `init_tddy_logger` from the config's own `log:` block; fork the spawn worker *before*
tokio; `startup_config_check` (*"Validated before anything is assembled, so a missing port or web
bundle fails at startup rather than after the roster is built"*); build the multi-thread runtime;
`tasks.spawn()`; a dedicated SIGTERM task; `server::run_server(...)`, then `kill_all` + `abort_all`.

Socket resolution, `packages/tddy-daemon-kernel/src/config.rs:1071`–`:1083`: explicit
`local.socket_path`, else `${XDG_RUNTIME_DIR}/tddy-daemon.sock`, else `/run/tddy-daemon.sock`.

Systemd activation then self-bind, `packages/tddy-daemon/src/local_socket_server.rs:70`–`:104`:

```rust
pub const SD_LISTEN_FDS_START: RawFd = 3;

#[derive(Debug, PartialEq)]
pub enum SocketSource { Activated(RawFd), SelfBind(PathBuf) }

pub fn resolve_socket_source(my_pid: u32, listen_pid: Option<&str>, listen_fds: Option<&str>,
                             fallback_path: &Path) -> SocketSource { … }
```

Graceful shutdown is `select!(ctrl_c, SIGTERM)` handed to `serve_*_with_shutdown`
(`server.rs:90`–`:132`, `runtime.rs:316`–`:340`). The supervisor's version
(`packages/tddy-supervisor/src/lib.rs:71`–`:133`) is the tightest reference, with two rules worth
copying: *"Signal streams first: a service that dies the instant it is exec'd would otherwise raise
its `SIGCHLD` before anything was listening for it"* and *"Bind before the first service starts…
binding first means a failure to bind cannot leave orphaned services behind."*

Its accept loop is the per-connection stdio-RPC pattern with a cap
(`packages/tddy-supervisor/src/server.rs:586`–`:618`):

```rust
let (reader, writer) = stream.into_split();
let service = SupervisorServiceServer::new(SupervisorServiceImpl::new(Arc::clone(&surface), peer));
// The supervisor never calls back into a caller, so the client half of the endpoint is
// dropped; the connection is request/response in one direction only.
let (_client, endpoint) = StdioEndpoint::from_duplex(reader, writer, service);
tokio::spawn(async move { endpoint.run().await; drop(slot); });
```
with `const MAX_CONCURRENT_CONNECTIONS: usize = 64;`.

The sandbox-runner's fail-fast rule for no transport (`runner.rs:2098`–`:2117`) is the one to mirror:

> "Neither flag is a default for the other: started with no transport at all, the jail would serve
> nobody, and failing fast beats a process that comes up and waits forever."

#### Logging — `log`, not `tracing`

**No `tracing` anywhere in the workspace** — zero `tracing`/`tracing-subscriber` dependencies. The
stack is the `log` facade over `tddy-core`'s YAML-configured backend
(`packages/tddy-core/src/log_backend.rs:24`–`:95`): `LogConfig { loggers, default, policies,
rotation }`, `LogOutput::{Stderr, Stdout, File, Buffer, Mute, Many}`,
`LogSelector::{Target, ModulePath, Heuristic}`. Entry points `tddy_core::init_tddy_logger(config)`
(`:638`) and `tddy_core::default_log_config(...)` (`:326`). **Target naming is
`crate_name::module`** with the underscored crate name — `target: "tddy_daemon::local_socket_server"`.

#### Install

`install` (1009 lines). Binary list, `install:554`–`:557`:

```bash
INSTALLED_BINARIES=(tddy-daemon tddy-coder tddy-tools tddy-remote-git-repo tddy-session-sync tddy-sandbox-runner)
if "$want_supervisor"; then
  INSTALLED_BINARIES=(tddy-supervisor "${INSTALLED_BINARIES[@]}")
fi
```

→ **a new binary must be added to this array**; `:559`–`:566` pre-flights that each release binary
exists and `:692`–`:695` installs it. Config templates are rendered only when absent.

#### Registering a new workspace member

1. `Cargo.toml` `[workspace] members` — add `"packages/tddy-<name>"` (no glob).
2. `packages/tddy-<name>/BUILD.yaml` — every package has one:
   ```yaml
   schema_version: 1
   targets:
     - id: "tddy-livekit-testkit:lib"
       name: "tddy-livekit-testkit"
       config:
         type: rust_library
         package: tddy-livekit-testkit
         srcs:
           - "packages/tddy-livekit-testkit/src/**/*.rs"
           - "packages/tddy-livekit-testkit/Cargo.toml"
   ```
3. `[lib] name = "tddy_<name>"` + `[[bin]] name = "tddy-<name>"`.
4. `packages/tddy-<name>/docs/` with `changesets/README.md`; feature docs in `docs/ft/<area>/`.
5. `scripts/generated-code.manifest` only if a **TypeScript** client is needed — Rust bindings live
   in `OUT_DIR` and *"therefore cannot drift"*.

#### The testkit handshake

`packages/tddy-livekit-testkit/src/livekit_testkit.rs:45`–`:93` — the five properties to copy:

```rust
/// Env var to reuse an existing LiveKit server instead of starting a container.
pub const LIVEKIT_TESTKIT_WS_URL_ENV: &str = "LIVEKIT_TESTKIT_WS_URL";

pub struct LiveKitTestkit {
    _container: Option<testcontainers::ContainerAsync<GenericImage>>,
    ws_url: String,
}

pub async fn start() -> Result<Self> {
    if let Ok(ws_url) = std::env::var(LIVEKIT_TESTKIT_WS_URL_ENV) {
        let ws_url = ws_url.trim().to_string();
        if !ws_url.is_empty() {
            let (host, port) = parse_ws_url(&ws_url)?;
            let http_url = format!("http://{}:{}", host, port);
            Self::wait_for_api_url_async(&http_url).await?;
            return Ok(Self { _container: None, ws_url });
        }
    }
    // …else start a container…
```

1. **Same type, same `start()`, same readiness gate** either way; only `_container: None` vs `Some`
   differs, so `Drop` reaps only what the testkit owns.
2. **Empty string = unset** — `packages/tddy-vm-testkit/src/env_file.rs:81` cites this as precedent.
3. **Readiness proven at the application layer**, not the socket layer — `wait_for_api_url_async`
   polls `RoomClient::list_rooms` under 15s at 200ms (`:153`–`:182`).
4. The URL is parsed by hand, failing with a named error.
5. The script prints **narration on stderr and a single `export …` line on stdout**, which is what
   makes `eval $(… | grep '^export ')` work.

### Findings

1. **The dual-transport recipe is fully established and generated** — one proto, a two-pass
   `build.rs`, one impl of the generated `tddy-rpc` trait, and gRPC comes free via
   `<Service>TonicAdapter`. Nothing here needs inventing.
2. **The generated trait is the in-process seam.** Its methods take and return prost message structs
   wrapped in `tddy_rpc::Request`/`Response` — a same-process caller passes Rust values with no
   encode/decode, which is exactly the unified single-shot design asked for.
3. **`--grpc` + `--stdio` concurrently is already proven** and already has a named acceptance test to
   model (`serves_grpc_and_stdio_concurrently_from_the_same_process`).
4. **Stdout discipline has a named helper** — `tddy_core::stdio_safety::{enforce_stdio_safe_log_output,
   redirect_fd_to_file}` — which must run *before* `init_tddy_logger`, since `log::set_logger`
   succeeds once.
5. **The proto package must be bare snake**, not versioned, per the TODO backlog.
6. **A new binary is not free**: workspace member, `BUILD.yaml`, `INSTALLED_BINARIES`, docs.
7. **No `tracing`** — `log` with `target: "tddy_<crate>::<module>"`.


## Exploration 5 — daemon-managed child-process lifecycle, and cancellation (Explore agent)

Run after the decisions "separate process, lifecycle managed by the daemon", "no budgets" and
"one process, many worktrees" were taken, to find the existing mechanism rather than invent one.

### Sequence

Enumerate every long-lived child `tddy-daemon` manages; read `packages/tddy-spawn` in full; read
`tddy-task`'s `TaskBody`/`TaskRegistry`/`IdleTimeoutTracker`; trace `tddy-sandbox-runner` end to end
from the daemon side; look for an existing get-or-spawn-a-process registry; read
`tddy-supervisor`'s `ManagedService` and both supervisor YAMLs; grep `tddy-rpc` and every transport
for cancellation, deadlines and `Drop`; grep the workspace for `tokio-util` and `CancellationToken`.

### Inspected files and excerpts

#### There is no single child supervisor in the daemon — there are five mechanisms

| Mechanism | Where | Transport | Who kills it |
|---|---|---|---|
| Forked spawn worker | `main.rs:57-66`, `tddy-spawn/src/spawn_worker.rs:126-278` | two `pipe(2)`s, newline-delimited JSON | nobody — dies when its request pipe closes |
| `tddy-sandbox-runner` (session) | `tddy-session-lifecycle/src/connection_service/svc_start_sandboxed_claude_cli_session.rs:535` | piped stdio, or gRPC over UDS/TCP | `SandboxSessionState::stop()`, from three call sites, none of them shutdown |
| `tddy-sandbox-runner` (workspace tools) | `tddy-daemon-sandbox/src/workspace_tool_sandbox.rs:274` | in-jail tool IPC | a `Drop` impl (`:458-462`) — the only child in the daemon with one |
| PTY CLI sessions | `tddy-session-lifecycle/src/cli_session_manager.rs`, `tddy-pty/src/runtime.rs` | PTY master fds | `kill_all()`, wired to SIGTERM at `main.rs:145-168` |
| Screen-sharing VNC/RDP bridges | `tddy-screen-sharing/src/screen_sharing_service.rs:305-372` | config on stdin, then closed | `terminate_bridge(&key)`; pids in `Mutex<HashMap<String, u32>>` |

Verified negatives: **`tddy-session-sync` and `tddy-remote-git-repo` are not daemon-spawned** —
the first is a standalone shipped binary, the second is exec'd by `git` as `GIT_SSH_COMMAND`.

#### `tddy-lsp`'s registry already is the pattern, and already runs rust-analyzer in the daemon

`packages/tddy-lsp/src/registry.rs:1-6` states exactly what it adds over `TaskRegistry`:

```
//! This adds the lookup-or-spawn-by-stable-key layer that [`tddy_task::TaskRegistry`]
//! lacks (it keys by generated UUID and evicts terminal tasks). Two requests with the
//! same [`LspKey`] return the same running server; idle servers are reaped; a server
//! whose task has become terminal is re-spawned on the next request.
```

That settles the layering question: `TaskRegistry` is the right holder for **process lifetime and
the cancel/kill net**, and is the wrong holder for **identity and connection** — it keys by
generated UUID and evicts terminal tasks (`TERMINAL_TASK_CAP = 200`, `TERMINAL_TASK_TTL = 5 min`,
`registry.rs:11-14`). Every long-lived child in the tree layers a purpose-built registry on top:
`LspRegistry`, `SandboxSessionManager`, `WorkspaceSandboxRegistry`.

Readiness in `get_or_spawn` is a `oneshot` plus `tokio::time::timeout(SPAWN_TIMEOUT)` — the cleaner
of the two idioms in the tree, but unavailable across a process boundary.

#### `LspServerBody` is the child body to copy; the sandbox path is the one to avoid

`packages/tddy-lsp/src/server_body.rs:75-77` registers the pid for the escalation net:

```rust
        if let Some(pid) = child.id() { ctx.register_child_pid(pid); }
```

and `:148-173` **observes** death rather than discovering it, then shuts down gracefully before
killing:

```rust
        let cancel = ctx.cancel_token();
        tokio::select! {
            _ = cancel.cancelled() => {}
            result = child.wait() => {
                stdin_task.abort(); stdout_task.abort();
                if ctx.is_cancelled() { return TaskStatus::Cancelled; }
                return match result { Ok(exit) => TaskStatus::Completed { exit_code: exit.code() }, … };
            }
        }
        // Cancellation requested: attempt graceful shutdown (bounded so a wedged
        // server can't stall us), then ensure the child is gone.
        let _ = tokio::time::timeout(GRACEFUL_SHUTDOWN, client.shutdown()).await;
        let _ = child.start_kill();
        let _ = child.wait().await;
```

The sandbox path does neither. `dial_and_bridge` **discards the relay's `JoinHandle`**
(`tddy-daemon-sandbox/src/sandbox_session.rs:418-426`), so nothing watches the runner; and shutdown
kills only `cli_sessions` (`main.rs:145-177`), so a runner outlives the daemon. Recorded as
[`2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md).

#### The reusable pieces for spawning a helper binary

`wait_for_sandbox_ready` (`tddy-daemon-sandbox/src/sandbox_session.rs:118-157`) — 50 ms poll, a
**child-death check every tick** so a child that dies before signalling fails fast with a decoded
reason, and diagnostics folded into the error message. The runner's half of the contract is
**bind, then write the marker** (`tddy-sandbox-runner/src/runner.rs:2479-2482` for UDS,
`:2060` for stdio), which is what makes the marker a readiness rather than a liveness signal.

`connect_uds_channel` (`tddy-sandbox-runner/src/runner.rs:2610-2624`) is the shared gRPC-over-UDS
connector, documented as *"Reused by any AF_UNIX tonic client … so the connector pattern lives in
one place."*

`resolve_sandbox_runner_path` (`sandbox_session.rs:557-581`) is the three-tier binary search:
`CARGO_BIN_EXE_*` → sibling of `current_exe()` (with a `deps/` hop for integration tests) → bare
name on `PATH`.

`StartupWatch` (`tddy-spawn/src/spawner.rs:502-554`) is the shared "did it survive its own argument
parsing" abstraction, `grace` + `poll`, serialisable across the worker's JSON IPC.

`terminate_sandbox_process` (`sandbox_session.rs:687-706`) SIGTERMs the process group, waits 200 ms,
then SIGKILLs both pid and group.

#### `tddy-supervisor` could declare it, and should not

`packages/tddy-supervisor/src/config.rs:85-125` — `ManagedService { name, exec_start, args, user,
group, working_dir, env, restart, socket }` with `RestartPolicy { max_retries, initial_backoff_ms,
max_backoff_ms, stability_threshold_ms }`, and a real `Starting → Running → Backoff → GaveUp`
machine (`services.rs:27-111`), backoff (`restart.rs`), a `SIGCHLD` drain guarded by a `spawn_gate`
(`supervisor.rs:484-510`) and a graceful shutdown with SIGKILL escalation (`:517-558`).

Three reasons it is the wrong home: `services:` is **eager and fixed at config load**
(`start_declared_services`, `supervisor.rs:333`) with no spawn-on-demand; the supervisor is
**optional** (`spawn_backend_choice` returns `ForkedWorker` when absent,
`supervisor_client.rs:32-38`; dev and `tddy-sandbox-app` run none); and its file is described in
`supervisor.yaml.production` as *"the ENTIRE privilege surface of the host"*, root-owned with
`deny_unknown_fields`.

Incidental correction: the comment in `dev.supervisor.yaml` claiming no daemon code path routes
spawning through the supervisor is **stale** — `supervisor_spawn` has five live call sites. And
[`2026-09-10-supervisor-spawn-delegation-…`](../todo/2026-09-10-supervisor-spawn-delegation-keeps-tddy-supervisor-in-the-daemon.md)
turns out to be about splitting a **test file**, not about architectural delegation; it carries no
implication for this change.

#### Cancellation — the finding that changed the design

`tddy_rpc::RpcService` (`packages/tddy-rpc/src/bridge.rs:33-69`) passes a handler the service name,
the method name and the message. **No token, no deadline, no context.** The only "deadline" in the
crate is `Status::deadline_exceeded` (`status.rs:55`).

`ServerEngine` does abort a disconnected peer's in-flight forwards
(`server_engine.rs:42-98`, `:212-235`), and `bridge.handle_messages` runs inside those forwards, so
the handler future is dropped. But:

- **It is not in every path.** `ServerEngine` backs LiveKit and the Tauri bridge. The local UDS path
  serves generated tonic servers directly (`tddy-daemon/src/local_socket_server.rs`), and
  `tddy-connectrpc` has no abort/disconnect/`Drop` handling — grepping its `src/` finds only the
  `Code::Aborted` string mapping (`error.rs:30`).
- **A dropped future stops nothing that is blocking.** The restructure backend is synchronous, driven
  inside `spawn_blocking`, and polls with `std::thread::sleep`. The blocking closure runs to
  completion regardless.

The pinned contract confirms the narrow intent
(`packages/tddy-rpc/tests/server_engine_peer_disconnect.rs:13`):

> The contract these tests pin: **when `on_peer_disconnected` returns, nothing further will be
> published for that peer**

— about not publishing to a dead peer, not about stopping work.

`tokio-util` is **not** a workspace dependency. Root `Cargo.toml:89-92`'s
`[workspace.dependencies]` holds only `rstest`, `pretty_assertions`, `sysinfo`. It is declared per
crate at `0.7`: `tddy-task:17` and `tddy-vm:11` and `tddy-actions:21` with `features = ["rt"]`,
`tddy-daemon-sandbox:19` bare, and `tddy-core`/`tddy-coder`/`tddy-acp-stub`/`tddy-integration-tests`
with `features = ["compat"]`. **`tddy-rpc` has none.** Non-test `CancellationToken` appears in four
files only: `tddy-task/src/task.rs`, `tddy-task/src/registry.rs` (doc comments),
`tddy-daemon-sandbox/src/sandbox_action.rs`, `tddy-vm/src/build.rs`.

`TaskContext` already exposes what is needed (`tddy-task/src/task.rs:357-405`): `cancel_token()`,
`is_cancelled()`, `register_child_pid()`, and the house rule at `:410` — *"Await
`ctx.cancel_token().cancelled()` in each `tokio::select!` wait."* `cancel_task`
(`registry.rs:195-207`) fires the token and spawns `escalation_safety_net` (`:289-329`): 5 s grace,
then SIGTERM every registered pid, 2 s, then SIGKILL.

### Findings

1. **`LspRegistry` is the template**, and the daemon already runs a lazily-spawned,
   workspace-root-keyed, idle-reaped, `TaskRegistry`-backed rust-analyzer. The index daemon is that
   shape one level up. No get-or-spawn-a-process pattern exists in `tddy-worktree-service`,
   `tddy-projects` or `tddy-session-lifecycle` to compete with it.
2. **`LspServerBody` is the child body to copy**; the sandbox runner's is the one to avoid, and its
   two defects are now recorded in the backlog.
3. **Cancellation is not free.** `RpcService` offers none, `ServerEngine`'s abort is absent from the
   tonic path, and a dropped future does not stop `spawn_blocking` work. The token must come from a
   `TaskBody` and be checked *inside* the synchronous loops — which makes the streaming design
   load-bearing, since a send failing into a dropped receiver is the only disconnect signal a
   handler can get.
4. **The supervisor is a documented non-goal**, for three concrete reasons.
5. **`tokio-util` is per-crate, not workspace-level**, and `tddy-rpc` has none — which is what makes
   a trait-level cancellation parameter a genuinely cross-cutting change rather than a small one.
