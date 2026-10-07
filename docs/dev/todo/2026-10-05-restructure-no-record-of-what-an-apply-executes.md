# 2026-10-05 — Nothing records what a restructure run executes, so a killed daemon or a blocked script cannot be traced

**Category:** Future enhancement (engine observability)
**Source:** #carve 17/21 (PR #532) stage B1/B2 — the warm index daemon died mid-run and, in the same
window, the developer's endpoint-protection agent (CrowdStrike Falcon) raised "Malicious script was
blocked". Whether the two are related could not be established.

## What was looked for, and what was found

- **The daemon's own log** (`tddy-index-<tag>.log`) — it was **truncated by the very restart that
  followed the death** (`run-index-daemon` empties the live log before each launch, because readiness
  is read from it). The evidence of the first run is gone. Fixed on its own branch
  (`fix/index-daemon-log-history`): the previous log is now appended to
  `tddy-index-<tag>.history.log` before it is truncated.
- **macOS unified log** (`log show`, last 2–3 h): nothing naming a blocked script, a kill of the
  daemon's pid, or a crash report for `tddy-index-daemon` / `rust-analyzer`
  (`~/Library/Logs/DiagnosticReports` is empty). Falcon's own sensor logs are not readable without root.
- **The restructure journal** (`journal.rs`, one record per operation) records the `WorkspaceEdit` each
  operation produced. It does **not** record the processes the run spawns, so it cannot answer "what
  was executed".

## The gap

A run executes, on the developer's behalf and unseen: `nix develop` env capture, `setsid sh -c 'echo $$
>pid; exec daemon'`, `cargo metadata`, rust-analyzer, its proc-macro server, every crate's build
script, and `cargo check --all-targets` after each apply. An endpoint-protection tool that blocks one
of these (a freshly written shell script is a classic trigger) leaves the engine with a daemon that is
gone and no line saying which child, which argv, or which signal.

## What would close it

1. ~~**A spawn log**: every process the engine, the CLI and the index daemon start — argv, cwd, pid,
   start time, and on exit its status **or terminating signal** — appended (never truncated) to the
   daemon's history, and for a CLI run to a file beside the plan's journal.~~ **Delivered** by
   `#sharpen` 3/8 (`spawn-record`, PR #590): an append-only JSONL record, one `start` and one `end`
   line per process, redacted and environment-names-only, at
   `<TDDY_INDEX_RUNTIME_DIR>/tddy-index-<tag>.spawns.jsonl` for the daemon and
   `<root>/.restructure/spawns.jsonl` for a cold CLI run.
2. ~~**The daemon's own exit**: today `exec` makes the daemon the pid itself, so a `SIGKILL` leaves no
   trace at all. A one-line "exited: status/signal, at <time>" written by a parent that is *not* the
   pid `--stop` signals (or by the daemon's signal handlers for everything but `SIGKILL`) would make
   "killed by something else" distinguishable from "crashed".~~ **Delivered** by the same node: the
   daemon runs as a background child of a watcher shell, whose `wait` writes the `index-daemon` `end`
   line — a `SIGKILL` as `{"signal":9}`.
3. **A per-operation line in the apply output** naming the plan operation id, so a log line can be
   joined to the journal record it belongs to. **Still open.** The record carries `at_unix_ms` and
   `pid`, so a join by time is possible by hand, but nothing names the op. It needs `runner.rs` to
   tell the recorder which operation is running, which is not in `#sharpen` 3/8's scope.

Not done here: the engine change is its own node (`tddy-code-restructuring` is out of scope for the
`#carve` stack).
