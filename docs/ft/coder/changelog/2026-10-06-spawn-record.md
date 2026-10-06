# 2026-10-06 — A restructure run leaves a record of every process it starts

Feature: [Rust code restructuring](../rust-code-restructuring.md).

A restructure run starts `git`, `cargo check`, `rustfmt` and a language server, and the index daemon
starts that server and is itself started by a script — and until now nothing recorded any of it. A run
now writes one append-only JSONL record of every process **the engine, the CLI, the index daemon and
`run-index-daemon` start**: argv, cwd, pid, start time, and on exit the status or the terminating
signal. A daemon killed by `SIGKILL` leaves an end line naming the signal, which no log could show.

- **One recorder, every site.** `git`, `cargo check`, `rustfmt`, the self-started rust-analyzer and the
  daemon's language server all report through the same seam, so the record cannot be complete for some
  sites and silent for others. A structural test keeps a future spawn site from bypassing it.
- **What may reach the file.** argv is recorded only for programs the policy knows, and within those an
  argument that carries a credential (URL userinfo, a token prefix, a `--token`/`--password`/… value, a
  sensitive `-c key=value`) becomes `<redacted>`. The environment is recorded as names only. A record
  that cannot be written is logged and dropped — it never fails the run.
- **Where it lives.** The daemon writes `tddy-index-<tag>.spawns.jsonl` in its runtime directory
  (`--spawn-record`), rotated at 10 MiB; the cold command line writes `.restructure/spawns.jsonl`,
  created only once the run has state of its own, so a baseline-refused run still writes nothing.

It does not see rust-analyzer's own children (build scripts, the proc-macro server) — those are started
by rust-analyzer. The record says *what* ran and *how it ended*, not *why a process stalled*: for a
stall the wait heartbeat is the instrument.
