# 2026-10-06 — A restructure run records every process it starts and how each ended

**Type:** Feature

One append-only JSONL record of every process a restructure run starts — the engine's `git`,
`cargo check` and `rustfmt`, the language server, and the index daemon and its script — with argv,
cwd, pid, start time, and on exit the status or the terminating signal. Observability only: what a
run starts, with which arguments and in which order, is unchanged. Behaviour:
[rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md).

## What changed

- **The seam (`tddy-lsp`, `spawn_observer.rs`)** — `SpawnObserver` (trait), `ProcessStart`,
  `ProcessToken`, `ProcessOutcome`. `LspRegistry::with_spawn_observer` carries one; the server body
  reports its start (program, args, cwd, pid) and each of its four ends through a
  `pub(crate) ObservedServerBody` wrapper, so `LspServerBody`'s public fields are untouched.
- **The recorder (`tddy-code-restructuring`, `spawn_record.rs`)** — `SpawnRecorder` wraps `Command`
  and is the only place the crate starts a process: `output`/`spawn` report a start with the pid, and
  `RecordedChild` reports the end through `try_wait`/`wait`. A failed spawn is a start with no pid and
  a `spawn_failed` end. The six production sites (`apply.rs` git, `backends/rust.rs` rust-analyzer,
  `runner/compile_gate.rs` cargo, `runner/tidy/format.rs` rustfmt) all go through it; a structural
  test (`every_spawn_is_recorded`) keeps a future site from bypassing it.
- **The sink and the policy (`spawn_record/jsonl.rs`, `spawn_record/redact.rs`)** — `JsonlSpawnRecord`
  opens `O_APPEND`, one newline-terminated line per write, `id` `<writer-pid>-<counter>`. argv is
  recorded only for an allow-listed program (`git`, `cargo`, `rustfmt`, `rust-analyzer`, `nix`,
  `setsid`, `sh`); within one, URL userinfo, token prefixes (`ghp_`/`gho_`/`github_pat_`/`sk-`/`xox`),
  `--*token*`/`--*password*`/`--*secret*`/`--*key*`/`--*auth*` values and sensitive `-c key=value`
  values become `<redacted>`. The environment is recorded as **names only**. A write that fails is
  logged and stops recording; it never fails the run.
- **The cold command line (`spawn_record/deferred.rs`)** — `ColdRunSpawnRecord` holds lines until
  `.restructure/` exists and writes nothing for a run the baseline check refuses, so the "nothing was
  written" guarantee holds.
- **The daemon (`tddy-index-daemon`)** — `--spawn-record <PATH>`; the observer rides inside the
  registry and the same recorder reaches `Options` for check and apply.
- **`run-index-daemon`** — `record_start`/`record_end` around its own children, rotated at 10 MiB, and
  the daemon launched under a `setsid sh -c` watcher that `wait`s on it, so a `SIGKILL` leaves an end
  line (a shell reports a signal death as `128+N`, so an exit status above 128 is read as a signal).

## Honest limits

- rust-analyzer's own children (build scripts, the proc-macro server) are never seen — they are
  started by rust-analyzer, not by anything this repository runs.
- A process killed between `spawn()` and the first write has no line; on the cold CLI path a run
  killed during the baseline check (before `.restructure/` exists) leaves no record.
- The record says *what* ran and *how it ended*, not *why a process stalled*.

## Code issues

| Record | Measurement |
|---|---|
| `oversized-file-backends-rust` | 2,852 → **2,864** production lines (+12; wiring only — the field, the builder and the `start` call site). History row appended; split still deferred on stack overlap |
| `tddy-index-daemon` `index.rs` | 491 → **505** (this node's `WorkspaceIndex::spawn_recorder`). Over budget; split deferred because the dependent node `apply-heartbeat` (#591) also grows this file |

## Backlog

No `docs/dev/todo/` entry was resolved. `2026-10-05-restructure-no-record-of-what-an-apply-executes`
is **narrowed, not deleted**: its items 1 (spawn log) and 2 (the daemon's own exit) land here; item 3
(a per-operation line naming the plan op id) is out of scope (decision D6) and stays open.
