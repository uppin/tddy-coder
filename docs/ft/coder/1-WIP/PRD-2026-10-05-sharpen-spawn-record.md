# A restructure run records every process it starts - PRD

**Date**: 2026-10-05
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — a new section,
  "What a run executes", beside § Waiting and § The tidy: which processes a run starts, where the record
  is written, its format and what it cannot show.
- **Related surface**: `.agents/skills/code-restructuring/SKILL.md` — the operative instructions for the
  index daemon gain "read the spawn record" next to PR #586's "read the daemon's history log". Not a
  `docs/ft/` document, but it is what an agent follows, and it changes here.
- **Related Feature**: [Reusable LSP](../reusable-lsp.md) — unchanged in behaviour; the registry gains an
  optional observer of the language servers it starts.

## Summary

A `tddy-tools restructure` run, the index daemon behind it and the script that starts the daemon execute
many processes on the developer's behalf: `git`, `cargo check`, `rustfmt`, a language server, `cargo build`
and a `nix develop` environment capture. Nothing records any of them. When a daemon vanishes or a run does
not return, there is no way to say which process was started, with what arguments, or how it ended.

This PRD adds one append-only record of those processes — argv, working directory, pid, start time and
the exit status or terminating signal — and a line for the daemon's own exit. It records what this
repository starts. It does not claim to see what rust-analyzer starts.

## Background

During `#carve` 17/21 the warm index daemon died in the middle of a run, and the developer's
endpoint-protection agent raised "Malicious script was blocked" in the same window. The daemon's log had
been emptied by the restart that followed (fixed separately, by PR #586, which keeps the previous log).
The journal could not help: it holds the edit each operation produced and nothing about processes. A
second, related run did not return after `check --deep` had answered; its last line named a build
script, and nothing could say whether that script had been started, blocked or killed.

The developer's observation: the loss is not of the log but of the account of what ran. `exec` makes the
daemon the pid `--stop` signals, so a `SIGKILL` leaves no trace at all; a process an endpoint tool
kills leaves no line in the engine either.

## Proposed Changes

### What's Changing

- Every process the engine starts (`git`, `cargo check`, `rustfmt`, rust-analyzer on the cold path),
  every language server the index daemon's registry starts, and every process `run-index-daemon` starts
  (`cargo build`, the dev-shell environment capture, the daemon) is written to a JSONL record: a `start`
  line when the process exists and an `end` line with its exit status, or the signal that terminated it.
- The daemon's own exit — status or signal, including `SIGKILL` — is a line in the same record,
  written by the shell that launched it, which is now the daemon's parent instead of being replaced by it.
- The record is append-only and never truncated by a restart. The daemon's file is rotated at 10 MiB.
- The record carries **no secret**: the environment is listed by variable **name** only, and an argument
  that looks like a credential is replaced by a marker. Programs not on a short allow-list have their
  arguments withheld.
- A failure to write the record never fails or delays a run.
- Where the cold command line writes its record, and where the daemon's lives, are Open decisions in the
  changeset (D1, D2).

### What's Staying the Same

- What a run starts, with which arguments, in which order. The record observes; it decides nothing.
- The journal, its format and the `.restructure/` layout. (If the command line's record were to live
  under `.restructure/` before the baseline check, the existing promise "Nothing was written" would
  change; the changeset's recommendation keeps it.)
- `restructure` output on stdout and stderr. Nothing new is printed.
- `docs/ft/coder/1-OVERVIEW.md` is not edited by this PRD's planning; the reference is added at wrap.

## Impact Analysis

### Technical Impact

- `tddy-lsp`: a small observer trait and an optional observer on the registry.
- `tddy-code-restructuring`: a recorder, a JSONL sink with the redaction policy, and the six production
  spawn sites (rust-analyzer, `cargo check`, `rustfmt`, three `git` calls) routed through it; a structural
  test keeps future ones honest.
- `tddy-index-daemon`: a `--spawn-record <path>` argument and the wiring.
- `run-index-daemon` and the agent skill: script-side records, the watcher, rotation, documentation.
- Dependencies: none added (times are Unix milliseconds).
- Performance: one `write` per process start and per process end; negligible next to the processes.

### User Impact

- A developer or agent chasing a vanished daemon reads `tddy-index-<tag>.spawns.jsonl` (with `jq`) and
  sees what was started and how each process ended, including a daemon that was killed.
- No change to commands, flags a developer types, or exit codes. The daemon gains one flag the launch
  script passes.
- **Honest limits (stated in the docs):** rust-analyzer's own children — build scripts and the
  proc-macro server — are started by rust-analyzer and are invisible to this record; for a stall inside
  one, the heartbeat (a separate PR) names what rust-analyzer says it is doing. A signal death behind a
  shell reads as an exit above 128.

## Implementation Plan

1. A seam trait in `tddy-lsp`; the recorder, JSONL sink and policy in the engine. Failing acceptance
   tests first, per the changeset.
2. Route the engine's six production spawn sites through the recorder; add the structural guard.
3. Report the language server's start and its four ends from `tddy-lsp`; wire the daemon.
4. After PR #586 has merged: the script-side records and the watcher in `run-index-daemon`; the
   skill paragraph.
5. Docs at wrap; the todo `2026-10-05-restructure-no-record-of-what-an-apply-executes.md` is narrowed to
   its remaining item (a per-operation line) unless that item is taken.
6. Testing: engine integration tests with real `git`/`cargo` and a fake language server; `tddy-lsp`
   observer test; daemon test; two `#[ignore]`d production tests for the script.

## Acceptance Criteria

- [ ] After an `apply`, the record lists every process the engine started, each with argv, working
  directory, pid, start time, and an end line with exit status or signal ([Rust code restructuring](../rust-code-restructuring.md)).
- [ ] A refused baseline (the tree did not compile) records the failing `cargo check` and its exit status.
- [ ] A process killed by a signal is recorded with the signal and no exit code.
- [ ] A program that cannot be started is recorded as a failed start.
- [ ] No environment value, and no credential-looking argument, appears in the file.
- [ ] A second run appends after the first; nothing is truncated.
- [ ] A record that cannot be written does not fail the run.
- [ ] A language server started by the daemon's registry is recorded with its end.
- [ ] `run-index-daemon` records its own `cargo build`, environment capture and launch, and a daemon
  killed with `SIGKILL` leaves an exit line naming the signal.
- [ ] A test fails if production code starts a process outside the recorder.
- [ ] The docs say what the record cannot show (rust-analyzer's children).
- [ ] Tests passing for the packages touched, run scoped; whole-workspace health read from CI.

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md) — "What a run executes".
- [Reusable LSP](../reusable-lsp.md) — observer on the registry (no behaviour change).

### Related Documentation

- Changeset: [2026-10-05-sharpen-spawn-record.md](../../../dev/1-WIP/2026-10-05-sharpen-spawn-record.md)
- Todo that motivates it, **present only on branch `fix/index-daemon-log-history` (PR #586)**:
  `docs/dev/todo/2026-10-05-restructure-no-record-of-what-an-apply-executes.md`
- Open PR #586 — `fix(run-index-daemon): keep the previous run's log when a new daemon starts`; this node
  lands after it.
