# 2026-10-06 — A waiting run names its stage and the server it waits on

**Type:** Feature

A restructure run that waits for rust-analyzer says nothing once the server stops talking. Every polling
wait of the Rust backend now beats a heartbeat (`WAIT_HEARTBEAT`, 30 s) naming its stage, the elapsed
time, the server and its last words with how long they have been unchanged; a request in flight on the
shared client is narrated too; the cancel message names the stage. The index daemon's queue beats, and a
caller that has hung up is noticed within one cadence. **No deadline is added** — a run still waits until
the server is ready or its caller stops it. Behaviour:
[rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) § Waiting.

## What changed

- **The heartbeat (`tddy-code-restructuring`, `backends/rust/wait.rs`)** — `WaitStage` (its text),
  `Waiting` (begin, elapsed, `beat_due`) and the pure `heartbeat_line`; `WAIT_HEARTBEAT` is 30 s. The
  five polling waits (`readiness.rs::ensure_indexed`, `readiness.rs::await_answer`, the assist wait,
  `settled_outline`, `locate_symbol`) each own a `Waiting` and beat through the run's `progress` sink;
  `keep_waiting`'s 100 ms slice checks the beat beside the cancellation token, so a beat is at most
  100 ms late. `RustBackend::with_wait_heartbeat`, `Options.wait_heartbeat` and `registry_for_waiting`
  carry the cadence (a constant in production; an injected `Duration` for tests, no test-only branch).
- **A request in flight (`lsp_bridge.rs`)** — `LspClientBridge::request` waits in heartbeat-sized
  slices, calling the ticker outside the runtime context between slices, so a request already sent to
  the shared server is narrated while it waits.
- **The cancel message (`lib.rs`, `rust.rs`)** — `RestructureError::IndexingIncomplete` gains `stage`;
  the refusal reads `… had not finished indexing after {n}s while {stage} (last progress: …)`. Every
  consumer matches with `..`.
- **The server's silence (`chatter.rs`)** — `ServerChatter::quiet_for` is the time since the server last
  said something *new* (a changed progress line, a quiescent or health flip), which is what the beat
  reports as `unchanged for`.
- **The daemon queue (`tddy-index-daemon`, `operations.rs`)** — `serve_apply`/`serve_check` wait for the
  root gate with `select!` against a heartbeat interval, sending `still waiting (Ns) — queued behind
  another operation on <root>`; a failed send drops the request out of the queue. `CodeIndexPorts` gains
  `wait_heartbeat`; `apply.rs` and `queries.rs` call `registry_for_waiting`.
- **The compile gate (`runner/compile_gate.rs`)** — `run_check` and `exit_or_kill` take the sink and the
  cadence; a `cargo check` that does not finish is narrated with its pid. Two partial cuts are recorded
  (`tidy.rs`, `group_gate.rs` — a discarding sink meanwhile).
- **The test double (`tddy-lsp`, `tests/bin/fake_lsp.rs`)** — three busy modes (`--never-quiescent`,
  `--goes-busy-after-hovers N`, `--hover-never-answers`) so a consumer can wait on a server that never
  gets ready without a real rust-analyzer.

## Honest limits

- The heartbeat **diagnoses, it does not cure**: a run stuck on a build script still waits until stopped.
- rust-analyzer's own children are invisible; the quoted progress text is evidence, not identification.
- A cold, self-started server is narrated between requests but not during one (its read is a blocking
  `read_line`); the polling stages of the cold path are covered.
- The `warm` stream stays silent on a silent server (its narrating function is too deeply nested to
  extend; a recorded follow-up).

## Code issues

| Record | Measurement |
|---|---|
| `tddy-code-restructuring` `oversized-file-backends-rust` | 2,864 → **2,975** production lines (+111; the `wait` module, the `wait_heartbeat` field and builder, `keep_waiting`'s beat, the five waits naming their stage). History row appended; split still deferred on stack overlap |
| `tddy-index-daemon` `index.rs` | 508 → **522** (this node's `wait_heartbeat` field and accessors, plus the inherited `spawn_recorder`). Over budget; split deferred |
| `tddy-index-daemon` `poisoned-warm-latch-on-interrupted-index` | Re-measured: **mechanism 1 does not hold on master** (the latch is set only on `quiescent: true`, and `RustBackend.indexed` is per request), mechanisms 2 and 3 stay open. Row appended; record kept |

## Backlog

No `docs/dev/todo/` entry was resolved. The two entries the plan scanned
(`2026-10-02-a-server-that-never-sends-server-status-leaves-settled-outline-waiting`,
`2026-10-02-rust-backend-locate-symbol-waits-on-an-empty-outline-with-no-deadline`) are **answered in
part, not closed**: the developer chose *no deadline* (D1), so the wait is now narrated but the question
they ask is answered in the negative. Both stay open, edited to say what the run now prints.
