# Changeset: a run that waits for the index says, on a fixed heartbeat, what it is waiting for

**Date**: 2026-10-05
**Status**: 🚧 In Progress
**Type**: Feature (diagnosability of a wait; **adds no deadline**)
**Stack**: `#sharpen` 4/8, branch `feature/sharpen/apply-heartbeat`, PR title
`feat(code-restructuring,lsp,index-daemon): a waiting run names its stage and the server it waits on (#sharpen 4/8)`.
**PR**: [#591](https://github.com/uppin/tddy-coder/pull/591) (draft).
Based on `feature/sharpen/spawn-record` (K=3) only because `gh stack` is linear; **no behavioural edge**
to any other node.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-05-sharpen-apply-heartbeat-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | What this change does about it |
|---|---|---|
| `2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check.md` — **exists only on `feature/carve/lifecycle-ports-agents` (PR #532); not on master, so it cannot be linked** | ⚠ **During** (diagnosability only) | The hang itself is **not reproduced**; the cause stays a hypothesis (two, now: a build script that never finished, and an earlier killed apply still holding the root's queue — see State A). This node delivers what the entry's "What capability would remove it" asks for **short of a deadline**: the run says the stage it waits on. It is **not** claimed ✅. The entry is deleted at wrap only if a reproduction shows the heartbeat naming the stuck stage; otherwise it is edited to say what the run now prints and that the cause is still unknown. This entry exists only on `feature/carve/lifecycle-ports-agents`; whichever of that PR and this node lands second handles it at wrap on that rule. |
| [`2026-10-02-a-server-that-never-sends-server-status-leaves-settled-outline-waiting.md`](../todo/2026-10-02-a-server-that-never-sends-server-status-leaves-settled-outline-waiting.md) | ℹ **Answered in part, stays open** | `settled_outline` gets the heartbeat (its loop is one of the five). The entry asks for a deadline or a silence-as-quiescent fallback; the developer decided **no deadline** (D1), so the question it asks is closed in the negative and **the defect stays recorded**: a server that never sends the status still waits until its caller stops. At wrap, edit the entry to say the wait is now narrated; do not delete. |
| [`2026-10-02-rust-backend-locate-symbol-waits-on-an-empty-outline-with-no-deadline.md`](../todo/2026-10-02-rust-backend-locate-symbol-waits-on-an-empty-outline-with-no-deadline.md) | ℹ **Answered in part, stays open** | Same as above for `locate_symbol`. ✅ only if the developer later chooses a bound. |
| `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md` | ⚠ **During** | (1) Its "If you are about to change this code" — *a run waits until the server is ready or until you stop it* — is **kept**: no wait here ends sooner. (2) Its mechanism 1 ("the warm latch is set before the graph finishes loading") is re-checked against master: the daemon has **no `indexed` field**; the latch is `graph.rs::GraphLoad`, set only on `quiescent: true`, never earlier; `RustBackend.indexed` is per request. So this node must not *introduce* a way for a failure or cancel path to set either, and a test pins it (D9). (3) At green, re-measure the record's three mechanisms and update it directly (code-issue records are the one place a changeset may edit `packages/*/docs/`): narrow or mark partly fixed what master already does. |
| `packages/tddy-index-daemon/docs/code-issues/complexity-warm-narrate-until-loaded.md` | ⚠ **During** | `warm.rs::narrate_until_loaded` (nesting 5) is **not touched**: adding a heartbeat arm to it would deepen the function the record asks to flatten. The warm stream therefore stays silent on a silent server; recorded as a follow-up (D7). |
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` (2,852 production lines) | ⚠ **During** | The wait logic goes in `backends/rust/wait.rs`. Net growth of `rust.rs`: **+10 lines** at most (a `Duration` field, a builder, five call-site edits, a `stage` argument on two constructors). Re-measure and append a history row at green. |
| [`2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md`](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **During** | Same constraint. |
| `packages/tddy-code-restructuring/docs/code-issues/dead-code-plan-filehint-modified.md`, `oversized-file-test-binary.md`, `complexity-rust-facade-lines.md`, `broken-restructure-anchors-empty-outline.md` | — | Not in this node's path; the last one's `Claimed by` is `none`, so there is no wait-or-proceed fork. |
| `packages/tddy-lsp/docs/code-issues/` | ℹ **Not analysed** | The directory does not exist (not the same as clean). This node edits only `tests/bin/fake_lsp.rs` there. |

No open issue is claimed (`Claimed by:`) in this node's path.

**Merge order.** Open PR #586 (`run-index-daemon`, `SKILL.md`) does not touch any file this node edits
except `SKILL.md`'s *Waiting* bullet at `:153`, which is a different paragraph from #586's; if both land
the hunks do not overlap. `spawn-record` (K=3, beneath this node) also edits `backends/rust.rs` and
`runner/options.rs` by a few wiring lines; take the later of the two at rebase.

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md) (line 20:
  the waiting sentence gains the heartbeat) —
  `backends/rust/wait.rs` (new), `chatter.rs`, `readiness.rs`, `rust.rs`, `lsp_bridge.rs`, `lib.rs`
  (`IndexingIncomplete` gains `stage`), `runner/entry_points.rs` (`registry_for_waiting`),
  `runner/options.rs` (`wait_heartbeat`), and, if D5, `runner/compile_gate.rs`.
  - [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md) — the
    *Readiness* section gains the heartbeat paragraph
- **`tddy-lsp`**: `tests/bin/fake_lsp.rs` only — three scripted modes (`--never-quiescent`,
  `--goes-busy-after-hovers N`, `--hover-never-answers`). A test double, "NOT part of the shipped
  product" by its own header; compiled as a `[[bin]]` by three crates
  (`tddy-lsp`, `tddy-code-restructuring`, `tddy-index-daemon`), so all three build it.
- **`tddy-index-daemon`**: [README.md](../../../packages/tddy-index-daemon/README.md) —
  `operations.rs` (the queue wait, D6), `service.rs` (`CodeIndexPorts::wait_heartbeat`), `apply.rs` and
  `queries.rs` (call `registry_for_waiting`); [code-index-service.md](../../../packages/tddy-index-daemon/docs/code-index-service.md).
- **`tddy-tools`**: **not edited.** It renders the daemon's `Indexing` events through
  `index_console::indexing`, which already stamps and prints any line; a heartbeat is one more line.
- **Agent docs**: `.agents/skills/code-restructuring/SKILL.md` *Waiting* bullet (`:153`).

## Related Feature Documentation

- [PRD-2026-10-05-sharpen-apply-heartbeat.md](../../ft/coder/1-WIP/PRD-2026-10-05-sharpen-apply-heartbeat.md)
- [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) — § Waiting (`:424`) gains the
  heartbeat and the stage in the cancel message.

## Summary

A restructure run that waits for rust-analyzer, which can be many minutes and has no budget, says nothing
once the server stops talking. This node makes every such wait print, on a fixed cadence, **which stage it
is in, how long, which server it is waiting on, and what that server last said and for how long it has said
nothing new** — and makes the "wait was cancelled" refusal name the stage. It adds **no deadline**: the
run still waits until the server is ready or its caller stops it.

## Background

`restructure apply` of a one-field rename sat at 0% CPU for over ten minutes after `check --deep` had
answered; its last lines were `waiting for type inference at the anchor` and `working: build script
num-bigint run` (see the todo in Prerequisites). The daemon stayed up and answered `--status`.

Verified in the code (State A): the throttled progress stream prints a phase once and then goes silent
while a server sits non-quiescent, and a wait that ends only by cancellation says nothing in between;
and a cancel arrives through a **failed send**, so a silent wait never learns its caller has gone. The
developer's decision (2026-10-05, D1) is a heartbeat with no deadline, which keeps the written design
rule (`packages/tddy-code-restructuring/README.md:20`: *"A run waits until the server is ready or until its
caller stops waiting; there is no budget flag."*).

## Responsibility

- Give every polling wait of the Rust backend a **stage**, a clock and a heartbeat: a line through the
  run's own `progress` sink every `WAIT_HEARTBEAT` while the wait lasts.
- Make a wait that is ended by cancellation say its stage (`IndexingIncomplete.stage`).
- Make the heartbeat reach a caller whose wait is **inside a request** to the shared language server
  (D4) and, in the daemon, a run **queued behind another** on the same root (D6).
- Teach `fake_lsp` to be busy for ever, so a test can wait on a server that never gets ready.
- Leave readiness state untouched on every failure and cancel path (D9).

## Boundaries

- **No deadline, no budget, no flag, no fallback.** Nothing in this node ends a wait. The README's rule
  stays and gains one sentence. (Developer decision D1.)
- **Does not change what "ready" means** (quiescent and healthy) or any readiness condition.
- **Does not name rust-analyzer's children.** The server's own child (a build script) is invisible to
  every site in this repository; the heartbeat can only quote the server's last progress text
  (`working: build script num-bigint run`), which is evidence, not identification.
- **Does not print.** The library returns lines through `ProgressSink`; the only printer stays
  `restructure_cli.rs` (the invariant `tests/library_returns_its_results.rs` pins). So nothing here can
  corrupt a ratatui screen or an RPC stream.
- **Does not touch `warm.rs`** (D7), `graph.rs`, the proto, or `tddy-tools`.
- **Does not make a request end sooner** or change `REQUEST_BOUND_ABOVE_ANY_COLD_INDEX`.
- **The cold, self-started server (`start`, `receive()`) is not covered while a request is in flight:**
  its read is a blocking `read_line` on the calling thread and cannot be interrupted for a heartbeat
  without a reader thread. The polling stages of the cold path **are** covered. Stated in the docs.
- **Does not edit `docs/ft/coder/1-OVERVIEW.md`** (shared append-point); the reference is a wrap TODO.
- **No new live rust-analyzer test binary**, so `.config/rust-e2e.filterset` and the `rust-analyzer` group in `.config/nextest.toml` are not edited (the new tests run over `fake_lsp`).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| none | No ancestor in the stack is consumed. `tidy-engine-files`, `move-fidelity` and `spawn-record` sit below it on the line only because `gh stack` needs one. (`spawn-record`'s record would help diagnose a stuck *process*; the heartbeat does not read it.) | — | touch `plan.rs`, `plan/codec.rs`, `item_anchor.rs`, `item_move/*`, `crate_move/*`, or read or write the spawn record |

Real edge list for this node: **empty** (it consumes nothing from another node, and nothing consumes it).

## Draft PR contract

The first push of this PR carries the surface below **with behaviour that waits as it does today** (no
heartbeat is emitted, `stage` is a constant string), plus the failing tests that specify it. Tests must
compile and fail by assertion.

**Owned API surface**

`tddy-lsp/tests/bin/fake_lsp.rs` — the **never-quiescent mode this node needs** (the existing
`--loads-crate-graph` only ever sends `quiescent: true`, `fake_lsp.rs:317`). Three flags, all off unless
asked for, so every other mode stays byte-identical (the file's own rule):

| Flag | Behaviour |
|---|---|
| `--never-quiescent` | On `initialized`: send `experimental/serverStatus {"health":"ok","quiescent":false}`, then a `$/progress` `begin` (`title: "Building compile-time-deps"`) and one `report` with `message: "build script num-bigint run"` and **no percentage**, and then **nothing, ever** — no `end`, no `quiescent: true`. Hover answers non-null (a real server answers hover while it runs build scripts — `readiness-and-gates.md`). Wins over `--loads-crate-graph`. |
| `--goes-busy-after-hovers N` | Behaves like `--loads-crate-graph` (graph loads, `quiescent: true`), and **after the Nth hover** sends `quiescent: false` and the same build-script progress, never ending. The shape of a tree that changed after a ready index. |
| `--hover-never-answers` | Receives `textDocument/hover` and sends no response (as `tddy/neverAnswers` does for its method); every other request is answered. |

`tddy-code-restructuring`:

```rust
pub const WAIT_HEARTBEAT: Duration = Duration::from_secs(30);          // backends::rust (re-exported)
impl RustBackend { pub fn with_wait_heartbeat(self, every: Duration) -> Self; }
pub fn registry_for_waiting(client, cancel, progress, trace, wait_heartbeat: Duration) -> BackendRegistry;
// registry_for(..) keeps its signature and delegates with WAIT_HEARTBEAT (seven callers unchanged)
// Options gains:  pub wait_heartbeat: Duration                  (default WAIT_HEARTBEAT)
RestructureError::IndexingIncomplete { seconds, last, environment, stage: String }   // new field
// new, pub(super): backends/rust/wait.rs  — WaitStage, Waiting, heartbeat_line(..) (pure)
// ServerChatter: pub fn quiet_for(&self, now: Instant) -> Duration  (time since the server last said anything new)
```

`tddy-index-daemon`: `CodeIndexPorts { servers, wait_heartbeat: Duration }` (the host sets it; production
passes `WAIT_HEARTBEAT`).

**Failing tests that specify it** are listed under "Acceptance tests", with what each fails on today.

## Green wave

**Wave:** 1 of 2.
**Greenable independently:** yes.
**Concurrent with:** `feature/sharpen/tidy-engine-files`, `feature/sharpen/move-fidelity`, `feature/sharpen/spawn-record`. No shared file with the first two; with `spawn-record`: `backends/rust.rs` and `runner/options.rs` (a few lines each; take the later one on rebase).
**Blocks:** none.
Real dependency edges (whole stack): `tidy-engine-files -> plan-header, retarget-impl, repoint-call, repoint-facade`; `move-fidelity -> repoint-facade`; `retarget-impl -> repoint-call`. Nothing else is an edge: `spawn-record` and `apply-heartbeat` consume nothing and nothing consumes them (`spawn-record` lands after open draft PR #586, a merge-order fact, not a stack edge).

## Scope

- [x] **`fake_lsp` modes** (the draft contract): `--never-quiescent`, `--goes-busy-after-hovers N`, `--hover-never-answers`, documented in the file header.
- [x] **`ServerChatter::quiet_for`**: when the server last said something *different* (a changed progress line, a quiescent or health flip).
- [x] **`backends/rust/wait.rs`**: `WaitStage` (its text), `Waiting` (start, last beat), pure `heartbeat_line`; `RustBackend::with_wait_heartbeat`; `WAIT_HEARTBEAT = 30 s`.
- [x] **The five polling waits** use it, and the heartbeat is emitted through `progress`: `readiness.rs` `ensure_indexed` (`:85`) and `await_answer` (`:190`); `rust.rs` the assist wait (`:967`), `settled_outline` (`:1568`), `locate_symbol` (`:2094`). `request_settled`'s `SETTLE_POLL` waits stay bounded by `CONTENT_MODIFIED_RETRIES` and are not narrated.
- [x] **A request in flight** on the shared client is narrated while it waits (D4): `LspClientBridge::request` waits in heartbeat-sized slices.
- [x] **The cancel message names the stage**: `IndexingIncomplete.stage` (and `incomplete_assist_index`); message reads `… after {n}s while {stage} (last progress: …)`.
- [x] **Options and registry**: `Options.wait_heartbeat`, `registry_for_waiting`; the daemon passes `CodeIndexPorts::wait_heartbeat`.
- [x] **Daemon queue (D6)**: a run waiting on `index.hold(root)` says it is queued behind another operation, and for how long; the same sends detect a caller that has hung up.
- [x] **Compile gate (D5)**: `cargo check`'s wait is narrated with its pid and `-p` list. *Cuttable to a follow-up if the node is large.*
- [x] **Readiness untouched on failure (D9)**: a test pins that a cancelled wait sets neither the backend's `indexed` nor the root's graph latch.
- [x] **No deadline guard**: a test pins that a busy server is still waited on after many heartbeats.
- [x] **Docs**: README `:20`, `readiness-and-gates.md`, feature doc § Waiting, `SKILL.md:153`, `code-index-service.md` — done. **Deferred to wrap:** the two code-issue records re-measured and the three `docs/dev/todo/` edits in `## Prerequisites` (both belong to the wrap step).
- [ ] **Re-measure** `oversized-file-backends-rust.md`; append a history row. **Deferred to wrap** — this node grew the file by +111 production lines (2864 → 2975); the split itself is deferred because the parent `spawn-record` edits the same file (see Technical Debt).
- [x] **Testing**: all acceptance tests passing (scoped).

## Technical changes

### State A (current)

Verified in the tree at `origin/master` `a77bca29`.

**The waits.** `backends/rust.rs:620 keep_waiting(poll)` sleeps in 100 ms slices (`CANCEL_CHECK`) and returns
`false` only when the run's `CancellationToken` is cancelled. Five loops use it with `INDEXING_POLL` (2 s)
and end only by readiness or cancellation:

| Loop | Where | `progress` text today |
|---|---|---|
| warm-up probe | `readiness.rs:59 ensure_indexed`, wait at `:85` | `warming crate index (until ready, or until you stop waiting)` (once) |
| type-inference wait | `readiness.rs:142 await_answer`, wait at `:190`; reached by `rename_symbol` (`rust.rs:2049`) through `wait_until_resolved`, and by the bounded probe | `waiting for type inference at the anchor` (once) |
| assist wait | `rust.rs:967` | none |
| outline wait | `rust.rs:1568 settled_outline` | none |
| symbol wait | `rust.rs:2094 locate_symbol` | none |

Each ends with `incomplete_index(started.elapsed())` (`rust.rs:638`) or `incomplete_assist_index`
(`:2363`, `:2377`), which build `RestructureError::IndexingIncomplete { seconds, last, environment }` — **no
stage**. Its text: *"rust-analyzer had not finished indexing after {seconds}s (last progress: {last}) and
the wait was cancelled. Toolchain it resolved with: {environment}"* (`lib.rs`). Consumers match it with
`{ .. }` (`tddy-index-daemon/src/status.rs:61`; `rust.rs:4358`, `:4384`;
`tests/cancellation_acceptance.rs:129`), so a new field breaks none of them.

**Why the stream goes silent.** `ServerChatter` (`chatter.rs`) deduplicates a phase on its percentage (or its
line when it has none) and shows at most one line per token per `PROGRESS_INTERVAL` (2 s). A server that is
non-quiescent (`loading()`, `chatter.rs:233`: `reported_status && !quiescent`) and has said its last
thing — `working: build script num-bigint run` — produces **no further line at all**, however long it
stays so. `how_far()` carries that last line into the cancel message, but only at the end.

**What the loops wait on.** For a self-started server, `self.server.process` (a pid the backend owns).
For a bridged one (every daemon request), a shared `LspClient`: the backend knows nothing about the process.
Each poll issues a hover through `LspClientBridge::request`, which **blocks the thread in
`Handle::block_on`** until the answer, the run's token (`request_abandonable`) or the client's request bound
(`REQUEST_BOUND_ABOVE_ANY_COLD_INDEX`, minutes): a request in flight cannot print.

**How a caller's disappearance is learned.** The daemon's `progress_into` (`operations.rs:308`) cancels the
run's token when `events.blocking_send` fails; **that is the only disconnect signal**
(`operations.rs:10-14`). A wait whose server is silent sends nothing, so it cannot learn its caller is
gone. Meanwhile `serve_apply` holds the per-root gate (`index.hold(&root)`, `operations.rs:187`) for the
whole run, and **every other apply, check, anchors, snapshot and analyze on that root queues behind it** —
silently: nothing is sent while queued (`operations.rs:77`, `:187`; `queries.rs`, `analyze.rs`). Two
hypotheses fit the incident and neither was reproduced: (H1) a build script blocked and rust-analyzer stayed
non-quiescent; (H2) the first, killed CLI left its apply holding the gate, and the *retry* queued behind
it. The todo's second attempt "ended at the timeout" is compatible with H2; the first stall needs H1.

**Printing.** The library prints nothing: `console.rs` returns lines, `ServerChatter::absorb` returns
`Option<String>`, and `tests/library_returns_its_results.rs` asserts `restructure_cli.rs` is the only
module under `src/` with a print macro. The cold CLI's sink is `a_stamped_sink("indexing", true)`
(`restructure_cli.rs:~153`), which `eprintln!`s `indexing (+2.0s): <line>`; the daemon's sink is
`progress_into`, an `Indexing` event that `tddy_tools::index_console::indexing` writes to stderr, stamped.

**Readiness state.** `RustBackend.indexed: bool` (`rust.rs:450`) is set in exactly two places —
`readiness.rs` after a ready hover (`:81`, in `ensure_indexed`) and in `await_answer` (`:158`) — and a backend is built per
request, so it dies with it. The daemon's per-root latch is `graph.rs::GraphLoad`: set by
`fold_until_the_graph_is_loaded` only when `chatter.quiescent()`, never unset while the server lives. **There
is no `indexed` anywhere in `tddy-index-daemon/src` or `tddy-lsp/src`.** `warm_workspaces()` reports
`ready: true` for any root with a live server, loaded or not (that is mechanism 2 of the poisoned-latch
record and is *not* touched here).

**Test double.** `fake_lsp.rs` can narrate a load (`--loads-crate-graph`: progress then
`quiescent: true`, `:317`), answer hovers `null` N times (`--cold-hovers N`, which sends no status and so
is never `loading()`), and swallow one custom method (`tddy/neverAnswers`). It cannot be non-quiescent, and
it cannot swallow a hover.

### State B (target)

- **Heartbeat.** Each of the five waits owns a `Waiting { stage, began, last_beat }`. After each poll it
  calls `waiting.beat(self)`; when `heartbeat` has passed since the last beat (or since `began`) it sends
  one line through `self.progress`. The line is formed by the pure `heartbeat_line`:

  ```
  still waiting (2m00s) — warming the crate index: rust-analyzer behind a shared client is still loading;
  its last words, unchanged for 1m58s: "working: build script num-bigint run"; furthest: Roots Scanned 100%
  ```
  Fields, in order: the **prefix** `still waiting` (greppable, and the thing tests match); **elapsed** in
  `console::step_delta`-style units since *this wait* began; the **stage**; **which server**: `rust-analyzer
  (pid N)` for a self-started one, `rust-analyzer behind a shared client` for a bridged one (the engine
  cannot know the daemon's pid); the server's **state** (`is still loading` when `chatter.loading()`,
  `has not said it is ready` otherwise); **its last words** (`chatter.how_far()`) with `quiet_for` —
  *the time since it last said anything new*, which is what separates a slow server from a stuck one; and
  `the server has said nothing yet` before the first line. The sink adds its own stamp
  (`indexing (+30.0s): …`).
- **Stages** (the `WaitStage` text; the file and line are the anchor's where there is one):
  `warming the crate index`, `type inference at <file>:<line>`, `an answer for the <assist> assist at
  <file>:<line>`, `the outline of <file>`, `locating <name> in <file>`, and for an in-flight request
  `a <method> request in flight`.
- **Cadence.** A constant, `WAIT_HEARTBEAT = 30 s`, first beat 30 s after a wait begins, then every 30 s
  while it lasts; **not reset by server activity** (a fixed heartbeat, as decided). A cold load of six to
  ten minutes prints twelve to twenty lines. Checked at each poll (2 s), so a beat is at most 2 s late.
  See D2 for why it is not a setting.
- **In-flight requests (D4).** `LspClientBridge::request` takes a ticker and waits in slices of the
  heartbeat: it pins the request future and loops `block_on(timeout(heartbeat, &mut request))`, calling
  the ticker **outside** the runtime context between slices — so the sink may `blocking_send` (a send inside
  `block_on` would panic).
- **Where it prints.** Nowhere new: only `progress`. CLI stderr via `restructure_cli.rs`; the daemon as an
  `Indexing` event rendered by `index_console` to stderr. A hung-up caller now makes the daemon's *next
  heartbeat send fail*, which cancels the run through the existing `progress_into` rule, so a silent wait is
  ended within one cadence (no deadline: the caller left).
- **Cancel message.** `IndexingIncomplete` gains `stage: String`; text: *"rust-analyzer had not finished
  indexing after {seconds}s while {stage} (last progress: {last}) and the wait was cancelled. Toolchain it
  resolved with: {environment}"*. `status.rs` maps it as before.
- **Daemon queue (D6).** `serve_apply` and `serve_check` wait for the gate with `select!` against a
  heartbeat interval, sending *"still waiting (Ns) — queued behind another operation on `<root>`"*; a failed
  send drops the request out of the queue (cancelling the pending lock) — it neither holds nor takes the
  gate.
- **Compile gate (D5).** `exit_or_kill` beats with `cargo check --all-targets -p <pkgs> (pid N), running for
  Ms; the run cannot see cargo's own children` and the existing kill-on-cancel stays.
- **Readiness untouched (D9).** The heartbeat reads `chatter`, `indexed` and the clock and **writes
  neither flag**. No cancel or failure path sets `indexed`; the daemon's `GraphLoad` is untouched.

### Delta (what's changing)

#### `tddy-code-restructuring`
- **Architecture**: `backends/rust/wait.rs` (new, ~130 lines): `WaitStage`, `Waiting`, `heartbeat_line`.
- **API**: as in the contract; `IndexingIncomplete.stage`; `Options.wait_heartbeat`; `registry_for_waiting`.
- **Implementation**: five waits; `lsp_bridge.rs` request slicing; `chatter.rs` `quiet_for`
  (`changed_at` set when the folded `last` differs, or `quiescent`/`health` flips).
- **Tests**: `tests/wait_heartbeat_acceptance.rs`; unit tests in `wait.rs` and `chatter.rs`.

#### `tddy-lsp`
- **Test double only**: three flags in `tests/bin/fake_lsp.rs`; `tests/fake_lsp_busy_modes_test.rs`.

#### `tddy-index-daemon`
- **API**: `CodeIndexPorts::wait_heartbeat`.
- **Implementation**: `operations.rs` queue wait; `apply.rs`, `queries.rs`, `plan_upkeep.rs` call
  `registry_for_waiting` where they own the cadence.
- **Tests**: `tests/wait_heartbeat_acceptance.rs`.

## Implementation milestones

- [x] **M1 — contract.** `fake_lsp` modes, `quiet_for`, `wait.rs` skeleton, `stage`, `Options.wait_heartbeat`,
  `registry_for_waiting`, `CodeIndexPorts::wait_heartbeat`, all compiling; tests exist and fail by
  assertion; `./test -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon` run scoped with the failing
  names listed here.
- [x] **M2 — the five polling waits beat.** Tests 1, 2, 4, 5 below green. `rust.rs` production-line count
  measured to the first real `mod tests` block: **2864 → 2975 (+111)**, well over this changeset's ≤10
  estimate — the estimate omitted the `beat`/`waited_on` adapter (it reads backend state, so it lives on
  the backend), `keep_waiting`'s rewrite, `request`'s in-flight narration and the doc comments this
  codebase requires. Not shuffled between modules to hit a number; see Technical Debt below.
- [x] **M3 — a request in flight beats** (D4). Test 8 green.
- [x] **M4 — the daemon.** Tests in `tddy-index-daemon/tests/wait_heartbeat_acceptance.rs` green, including
  the readiness-untouched test (D9).
- [x] **M5 — the compile gate beats** (D5), or the milestone is cut and a todo recorded.
- [x] **M6 — docs and records.** README, `readiness-and-gates.md`, feature doc, `SKILL.md`,
  `code-index-service.md`; poisoned-latch record re-measured and edited; three todos edited per
  Prerequisites; `oversized-file-backends-rust.md` history row. *(docs done; the code-issue re-measurements
  and todo edits are left to the wrap step — see the TODO below.)*
- [x] **M7 — gates, scoped.** `./test -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon`;
  `cargo clippy -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon --all-targets -- -D warnings`;
  `cargo fmt --check`. The whole workspace is CI's (`scripts/ci-status.sh`).

## Testing plan

### Testing strategy

**Primary test approach: integration against `fake_lsp`**, the repo's own double for "a server that is
loading" (`cancellation_acceptance.rs`, `wedged_request_acceptance.rs`). No real rust-analyzer: a real one
costs minutes and cannot be made to stay busy on demand. The three new fake modes are part of the draft
contract because every acceptance test below depends on them; the double itself gets a test so a mode that
stops working fails *there*, not as a mysterious timeout in a consumer.

Timing is made deterministic by **shortening the cadence, not by waiting**: tests set the heartbeat to
50–300 ms through `RustBackend::with_wait_heartbeat` / `Options.wait_heartbeat` /
`CodeIndexPorts::wait_heartbeat`, and assert on *counts and contents of lines*, never on wall-clock
windows tighter than a few cadences. The cadence is an injected collaborator, as `servers` is; production
passes the constant, and there is no test-only branch.

### Testing options analysis

#### Option 1 (chosen): fake server, short cadence, assert on lines
**Level**: Integration (backend over a real `LspRegistry` + `LspClient` + `fake_lsp` process) and service
(daemon over the same).
**Trade-offs**: each case costs 1–3 s; asserts on prose prefix `still waiting`, which is deliberate (a
reader greps for it) and means the wording is a contract.

#### Option 2: unit-test `heartbeat_line` only
Kept as a complement (pure, milliseconds), **not** a replacement: it cannot show that a loop calls it, which
is the defect.

#### Option 3 (rejected): a real rust-analyzer on a fixture with a blocked `build.rs`
It would reproduce the incident's shape, and it is the right *production* test — but it takes minutes,
needs the toolchain, and the hang is **not reproduced** on this machine. Recorded as a follow-up
(`#[ignore]`d, `./vm-tests`-style) if a reproduction is found; this is also what would justify promoting the
todo to ✅.

### Coverage requirements

- [x] Every one of the five waits names its stage (tests 1, 2, plus the unit table for the other three).
- [x] Heartbeats keep coming for as long as the wait lasts, with growing elapsed time.
- [x] The cancel message names the stage.
- [x] No deadline: still waiting after many beats.
- [x] Readiness flags unset after a cancel.
- [x] A caller that hangs up is noticed within a cadence; a queued run says so.
- [x] Nothing prints from the library (existing guard).

## Acceptance tests

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/wait_heartbeat_acceptance.rs`
Fixture style: `fake_lsp` through `LspRegistry` and `RustBackend::from_lsp_client(..)` exactly as
`cancellation_acceptance.rs`; a `Mutex<Vec<String>>` progress sink; a workspace holding `pub fn foo() -> u32`;
the operation is a `rename_symbol` of `foo` driven on `spawn_blocking`. **Needs `--never-quiescent` etc.
(the draft contract); no rust-analyzer.**
- [x] `a_wait_on_a_server_that_stays_busy_says_what_it_is_waiting_for_on_every_beat` —
  `--never-quiescent`, heartbeat 300 ms, 1.6 s of waiting: at least four lines begin `still waiting`;
  each contains `warming the crate index`, `still loading` and `build script num-bigint run`; the elapsed
  time in them never decreases. *Fails today:* no line begins `still waiting` (0 found; only the one-off
  `starting rust-analyzer session` / `warming crate index …` lines exist).
- [x] `a_wait_names_the_type_inference_stage_once_the_index_was_ready` — `--goes-busy-after-hovers 1`:
  the first beats name `type inference at src/lib.rs:1`, **not** the warm-up (the index was ready, then
  went busy: the shape of a tree that changed after a ready index). *Fails today:* no beat.
- [x] `a_beat_says_how_long_the_server_has_said_nothing_new` — across beats the `unchanged for` figure
  grows by about one cadence per beat. *Fails today:* no beat.
- [x] `a_cancelled_wait_names_its_stage_and_where_the_server_got_to` — cancel after the second beat: the
  refusal is `IndexingIncomplete` with `stage == "warming the crate index"`, `last` containing the
  build-script line, and its message containing `while warming the crate index`. *Fails today:* the
  variant has no `stage`, and the message names none.
- [x] `a_wait_has_no_deadline_however_many_beats_pass` — heartbeat 50 ms, 1.5 s (at least 25 beats): the
  operation is still running; then cancel ends it. **A guard** — green before and after; it exists because
  the developer decided *no deadline* (D1) and the cheapest way to break that is a "give up after N beats".
- [x] `a_request_in_flight_to_a_server_that_never_answers_is_narrated_while_it_waits` (D4) —
  `--hover-never-answers`, bridged backend: beats arrive while the hover is unanswered, naming `a
  textDocument/hover request in flight`; cancel still ends it within the existing 5 s unwind. *Fails today:*
  silence for the whole request.
- [x] `a_cargo_check_that_does_not_finish_is_narrated_with_its_pid` (D5, same file; fixture crate whose
  `build.rs` sleeps a minute; `Options.wait_heartbeat` 200 ms; real `cargo`; cancelled after two beats):
  beats contain `cargo check --all-targets -p origin` and `pid`; the run ends `CallerStopped`. *Fails
  today:* silence. **Skipped if D5 is cut.**

### `tddy-code-restructuring` — unit
- [x] `packages/tddy-code-restructuring/src/backends/rust/wait.rs` — `heartbeat_line` table: each of the six
  stage texts; `has said nothing yet`; `quiet_for` formatting at 0 s, 59 s, 1 m 02 s; self-started vs shared
  client wording.
- [x] `packages/tddy-code-restructuring/src/backends/rust/chatter.rs` — `quiet_for` resets on a *different*
  progress line, on a quiescent flip and on a health change, and **does not** reset on a repeat of the
  same line or on a notification the throttle suppressed with the same text.
- [x] `packages/tddy-code-restructuring/tests/library_returns_its_results.rs` — **unchanged, must stay
  green**: nothing the heartbeat adds prints.

### `tddy-lsp` — `packages/tddy-lsp/tests/fake_lsp_busy_modes_test.rs`
- [x] `a_server_started_never_quiescent_reports_loading_and_never_finishes` — through `LspClient`: a
  `serverStatus` with `quiescent: false` is observed; none with `true` within 1 s; hover answers non-null.
  *Fails today:* the flag is ignored, so no status is ever sent.
- [x] `a_server_that_goes_busy_after_its_first_hover_says_so_after_it` — `quiescent: true`, then
  `false` after one hover.
- [x] `a_server_that_never_answers_a_hover_still_answers_everything_else` — `documentSymbol` answers;
  hover does not.

### `tddy-index-daemon` — `packages/tddy-index-daemon/tests/wait_heartbeat_acceptance.rs`
Fixture style: `code_index_service_acceptance.rs` (service over an `LspRegistry` whose server is
`fake_lsp`), `CodeIndexPorts { wait_heartbeat: 300 ms, .. }`.
- [x] `a_busy_servers_beats_arrive_on_the_stream_as_indexing_events` — an `Apply` of a one-op plan over a
  `--never-quiescent` server: the stream carries `Indexing` events whose line begins `still waiting` and
  names the stage. *Fails today:* none.
- [x] `a_caller_that_hangs_up_during_a_silent_wait_releases_the_root_within_one_beat` — first `Apply`
  on the busy server; the test drops its stream after the first beat; a second `Apply` on the same root
  proceeds to its own wait within three cadences. *Fails today:* the first run is never cancelled (a silent
  server sends nothing, so nothing detects the hang-up) and the second never leaves the queue.
- [x] `a_run_queued_behind_another_on_its_root_says_so_and_for_how_long` (D6) — while the first is held,
  the second's stream carries `still waiting … queued behind another operation`. *Fails today:* silent.
- [x] `a_cancelled_wait_leaves_the_root_not_ready_and_the_next_warm_still_waits_for_the_graph` (D9) — after
  the first run is hung up on, a `Warm` on the same root streams no `ready: true` within three cadences;
  the observable is the public stream (`graph_load_of` is `pub(crate)`, and `RustBackend.indexed` is
  per-request, so the root-level effect is the only thing a test outside the crate can see). A guard:
  green before and after, red if a later change makes a failure path latch readiness.

## Technical Debt & Production Readiness

**Deferred, marked `TODO(apply-heartbeat)` in the tree:**

- `runner/tidy.rs:117` — the tidy's `cargo check` beats at the production default cadence, not the run's.
  `Tidying` carries the run's sink but not its cadence; adding the field would edit two tidy test
  literals, which the green brief forbade. A partial cut of D5, which this changeset marks cuttable.
- `runner/group_gate.rs:274` — the group gate's check is silent. `gate_group` reaches `failing_check`
  through `GroupRun::finish`, which carries no progress sink; threading one in would edit the
  `GroupGate` test literals. A discarding sink is passed meanwhile. The other partial cut of D5.

**Not done here (wrap step):** the `packages/*/docs/code-issues/` re-measurements (the poisoned-latch
record, and a history row for `oversized-file-backends-rust.md`) and the three `docs/dev/todo/` edits
named in `## Prerequisites`.

**`oversized-file-backends-rust.md` (~2,864 production lines).** This node grew `rust.rs` by **+111**
production lines (2864 → 2975, counted to the first real `mod tests`). Decomposition is **deferred, not
skipped**: the parent node `spawn-record` edits the same file, so a split here would cascade rename
conflicts through its diff — the `pr-stack` rule against splitting a file a parent or dependent PR
touches. Recorded for a follow-up branch after the stack lands.

**File-length gate (step 3.5), 2026-10-06.** Two files crossed the 500-production-line budget:
`rust.rs` (deferred above — parent overlap) and `tddy-index-daemon/src/index.rs`, which this node took
from **491 → 508** with the `wait_heartbeat` field and its accessors. No other `#sharpen` node's own
commits touch `index.rs`, so the stack-overlap stop does not apply; the developer chose to **defer with
a record** rather than expand this feature node's scope with an unplanned engine split. Recorded as
`packages/tddy-index-daemon/docs/code-issues/oversized-file-index.md` and
`docs/dev/todo/2026-10-06-split-tddy-index-daemon-index-rs.md`. All other touched source files are under
budget (`operations.rs` 459, `runner.rs` 437, `queries.rs` 446, …).

## Decisions & Trade-offs

Decisions already taken (the developer, 2026-10-05):
- **D1**: "heartbeat, no deadline." Quoted from the brief. Consequence: nothing ends a wait; the README's
  rule stays; two master todos ask the opposite and stay open.
- 8-node decomposition approved; log-history is PR #586, not this stack.

### D2 (OPEN): the cadence — constant, or a plan/daemon setting?

| Option | For | Against |
|---|---|---|
| **a. A constant, `WAIT_HEARTBEAT = 30 s`; an injected `Duration` for tests (`with_wait_heartbeat`, `Options.wait_heartbeat`, `CodeIndexPorts::wait_heartbeat`)** | No flag; matches the rule "prefer agent-driven config over flags"; the interval is not a budget so it needs no knob; tests shorten it without a branch | The knob exists in the library even though no front end exposes it |
| b. A plan-header field | Per-plan | A plan "holds intents only"; a header field is a new schema surface for a diagnostic |
| c. A daemon flag / env | Operator control | A flag for a diagnostic; every front end must pass it |

**Recommendation: a.** 30 s: below the 60 s at which a developer starts to suspect a hang, above the 2 s
at which a healthy load already narrates itself.

### D3 (settled by the code): content of the line

Stage, elapsed, which server, the server's last words and how long they have been unchanged. "Which child"
means the process **this repository can name**: the self-started rust-analyzer's pid, or "behind a shared
client"; for the compile gate cargo's pid. rust-analyzer's own children are named only by the server's
progress text. The `unchanged for` figure is the single most useful field: it is the only number that
separates a slow server from a stuck one.

### D4 (OPEN): narrate a request that is in flight?

The incident cannot say whether the wait was the 2 s poll loop (a hover answered non-null while the server
was loading) or a hover that never returned; both are silent today. **Recommendation: yes, for the bridged
(daemon) path** — it is a ~15-line change in `lsp_bridge.rs` and without it the heartbeat has a hole exactly
where the unreproduced hang may be. The cold self-started path stays uncovered (blocking `read_line`) and
the docs say so. Cut it and the node is smaller but only covers H1-while-polling.

### D5 (OPEN): the compile gate's `cargo check`

The same blocked-build-script signature produces a silent wait in `compile_gate::exit_or_kill`, and its
cancel (`CallerStopped`) names no stage. **Recommendation: include**, as the last milestone and cuttable:
`run_check` gains the progress sink (it has four callers: baseline, result check, tidy, group gate).

### D6 (OPEN): the daemon's queue

`serve_apply` holds the root gate for the whole run and every other request on the root waits behind it
silently; the only disconnect signal is a failed send (State A). **Recommendation: include** — about 25
lines in `operations.rs`, and the heartbeat's sends are what cancel a run whose caller has gone, which is
the most plausible reason the *retry* of the incident hung (H2). Not included: an `events.closed()`
watcher that cancels the instant a caller hangs up (immediate rather than within one cadence) — a separate
decision, and the heartbeat already bounds detection without it.

### D7 (OPEN): the `warm` stream

`warm.rs::narrate_until_loaded` forwards phases and sends nothing while the server is silent, the same hole.
**Recommendation: not in this node.** It is the function `complexity-warm-narrate-until-loaded.md` asks to
flatten (nesting 5); an arm added to it makes that worse. Record a todo: extract_method first, then add the
arm — its own node.

### D8 (settled): where it prints

Through `progress` only; see State A. Nothing in `console.rs` or `chatter.rs` prints; nothing here may.

### D9 (settled, pinned by a test): the readiness latch

The brief's concern — the failure path must not leave the `indexed` latch set — is, on master, an
**invariant to keep** rather than a bug to fix: `RustBackend.indexed` is per request and set only on
observed readiness; the daemon's `GraphLoad` is set only on `quiescent: true`. The heartbeat is read-only
with respect to both. The poisoned-latch record's mechanism 1 predates this shape (or is still true
somewhere this reading did not reach); **could not be reproduced from the code**, and the record is
re-measured at green rather than closed here.

### D10 (settled): the cancel message names the stage by a new field

`IndexingIncomplete.stage` rather than prose folded into `last`: the stage is data a front end may render
differently, and `last` already carries two different things (the server's words and, for the assist case,
a paragraph of explanation). All construction sites are in `rust.rs` (`:638`, `:2363`, `:2377`); every
consumer matches with `..`.

### Contract published (commit 2) — what exists, what refuses

Honest about what is not yet real, all marked `TODO(apply-heartbeat)`:

- **`fake_lsp` busy modes are implemented for real** (test-support code; the tests cannot exist without them).
  Verified: `tddy-lsp/tests/fake_lsp_busy_modes_test.rs` passes with them and fails (3 of 3, by assertion) against
  the old `fake_lsp.rs`.
- `wait.rs`: `WAIT_HEARTBEAT`, `WaitStage` (all six stage texts are real), `WaitedOn`, `Beat`, `Waiting::{begin,
  elapsed, beat_due}` and `heartbeat_line`. **`heartbeat_line` returns an empty string, `Waiting::beat_due` is
  never due, `ServerChatter::quiet_for` returns zero, and `IndexingIncomplete.stage` is the constant
  `waiting for the index`** — which is how every wait behaved before. Nothing calls any of it yet, so no wait
  emits a line. `RustBackend.wait_heartbeat`, `Options.wait_heartbeat`, `CodeIndexPorts.wait_heartbeat` and
  `WorkspaceIndex::with_wait_heartbeat` carry the cadence; the daemon's apply and anchors paths hand it to
  `registry_for_waiting`; the other `registry_for` callers still delegate with `WAIT_HEARTBEAT` (unchanged).
- `IndexingIncomplete.stage` is a field and the message already reads `… after {n}s while {stage} (last
  progress: …)`; the three construction sites in `rust.rs` pass the constant until each wait names its own.
- Delivered at green (the commit after this contract): the five waits calling `Waiting` and naming their
  stage, `LspClientBridge::request` slicing (D4), `exit_or_kill`'s beat (D5 — except the tidy cadence and
  the group gate; see Technical Debt), and the daemon queue wait (D6).

### Red-phase findings that change how green is written

1. **The beat must be checked inside `keep_waiting`'s 100 ms slices, not once per 2 s poll.** State B says "checked
   at each poll (2 s)". With the 300 ms and 100 ms cadences the tests inject, that would give a beat every 2 s
   at best. `keep_waiting` already sleeps in `CANCEL_CHECK` (100 ms) slices and looks at the token between them;
   the heartbeat belongs beside that look, so the cadence floor is 100 ms and a 30 s cadence is at most 100 ms
   late (not 2 s). Consequence for the tests: nothing here injects a cadence below 100 ms.
2. **A server must already be busy when the first question is asked.** `--never-quiescent` therefore sends its
   `serverStatus: false` and its build-script progress with the handshake, not after the narrator's 100 ms delay;
   a delayed one let `ensure_indexed`'s first hover see no status, call the index ready, and finish the rename.
   The bridged backend reads the client's retained status and backlog, so a notification sent before any
   subscriber attached is still folded in. (The delay stays for `--loads-crate-graph`, whose narration is the
   thing a subscriber is meant to watch.)
3. **`--goes-busy-after-hovers N` announces the busy state as the hover after the Nth arrives, before answering
   it** (the changeset said "after the Nth hover"). Sent right after the Nth answer, whether the client's drain
   after that answer already held it was a race, so a test could not say whether the wait was in
   `ensure_indexed` ("warming") or `await_answer` ("type inference"). With N = 1: hover 1 is answered by a ready
   server (warm-up ends), hover 2 is answered by one that has said it is busy.
4. **The rename is anchored by a range at `src/lib.rs:1`** (line 1, columns 8–11, the name `foo`), not by symbol:
   `fake_lsp`'s `documentSymbol` always places `foo` at line 10, so a symbol anchor would name `:11` and the
   changeset's `type inference at src/lib.rs:1` could not hold. The outline and symbol waits (`locate_symbol`,
   `settled_outline`) are therefore covered by the `heartbeat_line` unit table only, as the coverage list says.
5. Windows and cadences: test 1 waits for four beats (changeset: 1.6 s); the `unchanged for` test beats once a
   second and asserts each step grows by one or two whole seconds (the figure is whole seconds); the no-deadline
   guard beats every 100 ms for 2.5 s (changeset: 50 ms for 1.5 s, 25 beats — unreachable at a 100 ms floor, see
   finding 1) and, being a guard, does not assert on beats.
6. **Tests that already pass, by design:** `a_wait_has_no_deadline_however_many_beats_pass` and
   `a_cancelled_wait_leaves_the_root_not_ready_and_the_next_warm_still_waits_for_the_graph` (guards), and the
   unit test `a_wait_is_not_due_a_beat_before_the_heartbeat_has_passed`.
7. Fallout of two new fields: `CodeIndexPorts` gained `wait_heartbeat`, so every struct literal of it was edited —
   `tddy-index-daemon/src/main.rs` and five test files, **including the parent's
   `tests/spawn_record_acceptance.rs`** (one added line, no behaviour); a `status.rs` unit-test literal and a
   `rust.rs` unit-test pattern gained `stage` / `..`.

### Decisions taken at the contract (the open ones, per their recommendations)

- **D2** followed **a** (constant 30 s, injected `Duration`). **D4** followed **yes** (the in-flight request is narrated; test
  `a_request_in_flight…` pins it). **D5** followed **include** (test `a_cargo_check…` pins it; still cuttable). **D6** followed
  **include** (two daemon tests pin it). **D7** followed **not in this node** (no `warm` change; the guard test reads `Warm`
  only to check it never claims ready).

### Honest limits (also in the docs)

- The heartbeat **diagnoses, it does not cure**: a run stuck on a build script still waits until stopped.
- rust-analyzer's own children are invisible; the quoted progress text is evidence.
- The hang is **not reproduced**; a heartbeat that "names the stuck stage" is shown only against a fake.
- A cold, self-started server is narrated between requests but not during one.
- The warm stream stays silent on a silent server (D7).

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
*(empty)*

### From @red (TDD Red Phase)
*(empty)*

### From @validate-changes (Change Validation)
*(empty)*

### From @validate-tests (Test Quality)
*(empty)*

### From @prod-ready (Production Readiness)
*(empty)*

### From @analyze-clean-code (Code Quality)
*(empty)*

### From @refactor (Completed Refactorings)
*(empty)*

## Validation Results

### @validate-changes
**2026-10-06 — clean.** Stack gate: base `feature/sharpen/spawn-record`, branch already current,
leak check clean (`origin/<base>..HEAD` is this PR's four commits only). Build: the three touched
packages build clean. Boundaries held — no `warm.rs`, `graph.rs`, proto or `tddy-tools` change, no
deletions, nothing from `## Dependencies` implemented (the edge list is empty). Stubs: only the two
recorded `TODO(apply-heartbeat)` D5 partial cuts (`tidy.rs:117`, `group_gate.rs:274`). Changeset
synced: Scope, Coverage and Acceptance boxes ticked; the two record-remeasurement items annotated
deferred to wrap; `rust.rs` growth measured and recorded (2864 → 2975, +111).
Scoped tests: `wait.rs` 18/18, `chatter.rs` 22/22, `fake_lsp_busy_modes_test` 3/3,
`wait_heartbeat_acceptance` (code-restructuring) 7/7, `wait_heartbeat_acceptance` (daemon) 4/4,
`library_returns_its_results` 6/6; `cargo clippy … --all-targets -- -D warnings` clean; `cargo fmt
--check` clean.

### @validate-tests
**2026-10-06 — clean.** The acceptance and unit suites are unchanged from the red phase (fluent
Given/When/Then, named helpers, deterministic short cadences, no wall-clock assertions). The only
test edits are five mechanical call-site follow-ups in `rust.rs`'s own unit tests for the two
signatures the plan mandates — `keep_waiting(None, …)` ×3 and `incomplete_assist_index(…, &WaitStage,
…)` ×2 — with every assertion and match arm intact. No test weakened, no anti-pattern introduced.

### @prod-ready
**2026-10-06 — clean, two recorded deferrals.** No mock code, no test-only branches, no debug output:
the library prints nothing (only `restructure_cli.rs` does, pinned by `library_returns_its_results`),
the daemon streams through its own sinks, and `fake_lsp` is a documented `tests/bin` double. The only
`TODO` markers this node added are the two D5 partial cuts (`tidy.rs:117`, `group_gate.rs:274`),
recorded in Technical Debt above. The stale `options.rs` TODO ("none does yet") was corrected.

### @analyze-clean-code
**2026-10-06 — clean.** The new functions are small and shallow: `heartbeat_line` (~25 lines, no
nesting), `Waiting::beat_due` (5), `keep_waiting` (a loop with one `if`), `beat`/`waited_on` (~18),
`LspClientBridge::request_narrated` (~20), `hold_saying_so` (~28, one `select!`), `exit_or_kill`'s
beat (~10 added). Cadences are named constants (`WAIT_HEARTBEAT`, `CANCEL_CHECK`), not magic values.
The one shared string is the greppable `still waiting` prefix, deliberately repeated at the three
narration sites rather than factored behind an abstraction. File length: `rust.rs` and `index.rs` are
over budget and recorded — see the file-length gate below.

## TODO

- [x] Record initial discovery (`2026-10-05-sharpen-apply-heartbeat-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-05-sharpen-apply-heartbeat.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (shared append-point; eight nodes would conflict, so planning does not edit it)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run the scoped tests (`./test -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon`) — verify 100% pass; whole-workspace health is CI's
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-05-sharpen-apply-heartbeat-initial-discovery.md`; edits (does not delete) the three todos unless a reproduction promotes one to ✅
- [ ] USER REVIEW — work complete, decide next steps

## Successor PRs

Forward links only (parent to child): none depends on this node. The later nodes
(`feature/sharpen/plan-header`, `feature/sharpen/retarget-impl`, `feature/sharpen/repoint-call`,
`feature/sharpen/repoint-facade`) neither consume nor extend it.
