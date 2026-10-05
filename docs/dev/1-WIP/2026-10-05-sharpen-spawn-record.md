# Changeset: a restructure run leaves an append-only record of every process it starts

**Date**: 2026-10-05
**Status**: 🚧 In Progress
**Type**: Feature (engine and daemon observability; no change to what a run does)
**PR**: https://github.com/uppin/tddy-coder/pull/590 (draft)
**Stack**: `#sharpen` 3/8, branch `feature/sharpen/spawn-record`, PR title
`feat(code-restructuring,lsp,index-daemon): a record of every process a restructure run starts (#sharpen 3/8)`.
Based on `feature/sharpen/move-fidelity` (K=2) only because `gh stack` is linear; **no behavioural edge**
to any other node.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-05-sharpen-spawn-record-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

Records this node runs into. Code issues are named by path in backticks (they are renamed when their
symbol moves); todos are linked.

| Item | Verdict | What this change does about it |
|---|---|---|
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` (2,852 production lines) | ⚠ **During** | The one spawn in `backends/rust.rs` (`start`, line 685) is swapped for a call on a recorder held in a field. Net growth in `rust.rs` is held to **+8 lines** (field, builder, the call); every other line of the feature lives in `spawn_record.rs` and its children. Re-measure and append a history row to the record at green. |
| `packages/tddy-lsp/docs/code-issues/` | ℹ **Not analysed** | The directory does not exist, which is not the same as clean. This node edits `src/server_body.rs` and `src/registry.rs`; consider `/analyze-code-issues` on the crate after it lands. Nothing is claimed here. |
| `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md` | ⚠ **During** | Its "If you are about to change this code" says a run's waiting behaviour is the design. This node changes **no wait**: it only observes processes. It also adds the one thing that record's incident lacked: when a daemon dies, a line saying how. No edit to the record. |
| `packages/tddy-index-daemon/docs/code-issues/complexity-warm-narrate-until-loaded.md` | — | `warm.rs` is not touched. |
| `packages/tddy-code-restructuring/docs/code-issues/dead-code-plan-filehint-modified.md`, `oversized-file-test-binary.md`, `complexity-rust-facade-lines.md`, `broken-restructure-anchors-empty-outline.md` | — | Not in this node's path. The last one's `Claimed by` value is `none` (#537 merged); no live claim exists, so there is no wait-or-proceed fork. |
| [`2026-10-05-restructure-no-record-of-what-an-apply-executes.md`](../todo/2026-10-05-restructure-no-record-of-what-an-apply-executes.md) — on master since #586 merged | ⚠ **During**, becomes ✅ only if all three items are closed | Items 1 (spawn log) and 2 (the daemon's own exit) are in Scope. **Item 3 (a per-operation line naming the op id) is not**: see Decisions, D6. The entry therefore stays open and is *narrowed* to item 3 at wrap — it is deleted only if item 3 is delivered by then. |
| [`2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md`](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **During** | Same constraint as the first row: wiring only in `rust.rs`. |
| [`2026-10-05-restructure-engine-files-past-the-500-line-budget.md`](../todo/2026-10-05-restructure-engine-files-past-the-500-line-budget.md) | — | `plan.rs`, `plan/codec.rs`, `item_anchor.rs` are not touched here. (Resolved by `tidy-engine-files`.) |

**#586 has merged** (commit `e5a5b2d4`, in this branch's base). `run-index-daemon` now has
`archive_previous_log` and `HISTORY_FILE` (`tddy-index-<tag>.history.log`, line 77 and 138-158), and the
todo above is on master. This node edits `run-index-daemon` and
`.agents/skills/code-restructuring/SKILL.md` at those places, and no longer waits on anything: M5 and M6
are greenable with the rest. The history file is the text copy D2 describes (headers, rotated at 10 MiB
by a shell function), which is why D2's recommendation stands: the spawn record is a sibling file.

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md) —
  `spawn_record` module (recorder, JSONL sink, redaction), every production spawn site routed through
  it, `Options::spawns`, the CLI front end installing a file sink.
  - [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md) — the
    compile gate and tidy now record their `cargo`/`rustfmt` children
- **`tddy-lsp`**: no README; `docs/` holds [workspace-root.md](../../../packages/tddy-lsp/docs/workspace-root.md)
  — the `SpawnObserver` trait and the registry's optional observer; `LspServerBody` reports the language
  server's start and its end.
- **`tddy-index-daemon`**: [README.md](../../../packages/tddy-index-daemon/README.md) and
  [code-index-service.md](../../../packages/tddy-index-daemon/docs/code-index-service.md) — `--spawn-record
  <path>`, the observer wired into the registry and into the operations' `Options`.
- **Repo scripts and agent docs (not packages)**: `run-index-daemon` (script-side records, the
  daemon's exit line, rotation of the record), `.agents/skills/code-restructuring/SKILL.md` (where to
  read the record).
- **`tddy-tools`**: **not edited.** `restructure` runs in `tddy-code-restructuring`'s own front end
  (`restructure_cli.rs`); `tddy-tools` only forwards.

## Related Feature Documentation

- [PRD-2026-10-05-sharpen-spawn-record.md](../../ft/coder/1-WIP/PRD-2026-10-05-sharpen-spawn-record.md)
- [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) — gains a section on what a run
  executes and where it is recorded.

## Summary

A `restructure` run starts `git`, `cargo check`, `rustfmt` and a language server, and the daemon starts
that server and is itself started by a script that runs `cargo build` and `nix develop`. Today nothing
records any of it: the journal holds `WorkspaceEdit`s only. This node adds one append-only JSONL record of
every process **the engine, the CLI, the index daemon and `run-index-daemon` start** — argv, cwd, pid,
start time, and on exit the status or the terminating signal — and a line for the daemon's own exit.

It does **not** see rust-analyzer's children (build scripts, the proc-macro server): those are spawned
by rust-analyzer, not by anything this repository runs.

## Background

During `#carve` 17/21 a warm index daemon died mid-run and, in the same window, the developer's endpoint
protection raised "Malicious script was blocked". The daemon's log had already been truncated by the
restart; PR #586 keeps it from now on. What no log can answer is *what was executed* and *how each
process ended*. A process killed by a signal, a `cargo check` that blocked, a daemon that vanished by
`SIGKILL` leave no line anywhere, because `exec` makes the daemon the pid `--stop` signals and nothing is
left to observe its exit. The todo (see Prerequisites) asks for exactly this; this node is the engine
half it names.

## Responsibility

- Define the seam through which a process start and end become a record, once, and route **every
  production spawn site of the engine and `tddy-lsp`** through it.
- Write the record as append-only JSONL, one `start` line and one `end` line per process, never
  truncated, with a redaction policy so an argument that carries a credential never reaches the file.
- Wire a file sink at the two front ends that own a process: the cold CLI path, and the index daemon.
- Make `run-index-daemon` record its own children and the daemon's exit (status or signal).
- Say, in the docs, what the record cannot show.

## Boundaries

- **No behaviour change.** What a run starts, with which arguments and in which order, is unchanged; the
  recorder wraps `Command`, it does not decide anything. A failure to write the record **does not fail
  the run**: it is logged through `log` and recording stops for that file (a recorder that can fail a
  run would turn an observability feature into a new way for `apply` not to return).
- **Not rust-analyzer's children.** Build scripts and the proc-macro server are started by
  rust-analyzer. The only evidence of them is rust-analyzer's own `$/progress` text, which the heartbeat
  node (`apply-heartbeat`) surfaces. The todo's "every build script" is unreachable from these sites and
  this node does not claim it.
- **No new external dependency.** Times are Unix milliseconds (`at_unix_ms`), not a formatted date, so
  no `chrono`/`time` is added (project rule: ASK before adding dependencies).
- **No per-operation line** naming the plan op id (todo item 3): out of scope, D6.
- **Not the journal.** `journal.rs` records edits; this record is a different file with a different
  reader. The journal's format and the `.restructure/` layout are untouched.
- **No `restructure` subcommand to read the record** — it is JSONL for `jq`; a reader command is a
  follow-up if wanted.
- **Not touched:** `plan.rs`, `plan/codec.rs`, `item_anchor.rs`, `item_move/*`, `crate_move/*`,
  `readiness.rs` (waits), `warm.rs`, `graph.rs`.
- **Does not edit `docs/ft/coder/1-OVERVIEW.md`** (shared append-point); the PRD reference is a wrap TODO.
- **No new live rust-analyzer test binary**, so `.config/rust-e2e.filterset` and the `rust-analyzer` group in `.config/nextest.toml` are not edited (the new tests run over `fake_lsp` or no server).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| none | No ancestor in the stack is consumed. `tidy-engine-files` and `move-fidelity` sit below it on the line only because `gh stack` needs one. | — | edit `plan.rs`, `plan/codec.rs`, `item_anchor.rs`, `item_move/*` or `crate_move/*` |

The real edge list for this node is **empty** (it consumes nothing from another node, and nothing consumes it).
PR #586 has merged and is part of the base; it is a prerequisite already satisfied, not a dependency.

## Draft PR contract

The first push of this PR (commit 2 of the planning wave) carries **the surface below with behaviour
that records nothing**, plus the failing tests that specify it. Tests must compile against it and fail
by assertion (zero records), not by compile error.

**Owned API surface**

`tddy-lsp` (new `src/spawn_observer.rs`, re-exported from `lib.rs`):

```rust
/// What a host that wants to know which processes a crate starts implements.
pub trait SpawnObserver: Send + Sync {
    /// Called once the process exists (pid known), or with `pid: None` and then `ended(SpawnFailed)`.
    fn started(&self, process: &ProcessStart) -> ProcessToken;
    fn ended(&self, token: ProcessToken, outcome: &ProcessOutcome);
}
pub struct ProcessStart { pub purpose: &'static str, pub program: String, pub args: Vec<String>,
                          pub cwd: Option<PathBuf>, pub env_names: Vec<String>, pub pid: Option<u32> }
#[derive(Clone, Copy)] pub struct ProcessToken(pub u64);
pub enum ProcessOutcome { Exited { code: i32 }, Signalled { signal: i32 }, SpawnFailed { error: String } }
impl LspRegistry { pub fn with_spawn_observer(self, observer: Arc<dyn SpawnObserver>) -> Self; }
```

`tddy-code-restructuring` (new `src/spawn_record.rs` + children `spawn_record/jsonl.rs`,
`spawn_record/redact.rs`):

```rust
#[derive(Clone)] pub struct SpawnRecorder { /* Arc<dyn SpawnObserver> */ }
impl SpawnRecorder {
    pub fn discard() -> Self;                                   // the default, as `discard()` is for progress
    pub fn new(observer: Arc<dyn SpawnObserver>) -> Self;
    pub fn output(&self, purpose: &'static str, command: &mut Command) -> io::Result<Output>;
    pub fn spawn(&self, purpose: &'static str, command: &mut Command) -> io::Result<RecordedChild>;
}
pub struct RecordedChild { /* Child + token */ }   // try_wait / wait / kill / take_stdout / take_stderr
pub struct JsonlSpawnRecord;                        // implements SpawnObserver
impl JsonlSpawnRecord { pub fn open(path: &Path) -> io::Result<Self>; }   // O_APPEND, one write per line
pub fn redacted(program: &str, args: &[String]) -> Vec<String>;
// Options gains:  pub spawns: SpawnRecorder          (default: SpawnRecorder::discard())
// RustBackend:    pub fn with_spawn_recorder(self, SpawnRecorder) -> Self
```

`tddy-index-daemon`: `--spawn-record <PATH>` on the daemon's argument struct (beside `--log-file`).

**Failing tests that specify it** (all in "Acceptance tests"): `spawn_record_acceptance.rs` (engine),
`every_spawn_is_recorded.rs` (engine), `spawn_observer_test.rs` (`tddy-lsp`), `spawn_record_acceptance.rs`
(daemon). Each states what it fails on today there.

## Green wave

**Wave:** 1 of 2.
**Greenable independently:** yes, including the `run-index-daemon` and `SKILL.md` hunks, now that #586 has merged.
**Concurrent with:** `feature/sharpen/tidy-engine-files`, `feature/sharpen/move-fidelity`, `feature/sharpen/apply-heartbeat`. No shared file with the first two; with `apply-heartbeat` the shared files are `backends/rust.rs` (a few wiring lines each; take the later one on rebase) and `runner/options.rs` (one field each); `tddy-lsp/tests/bin/fake_lsp.rs` is `apply-heartbeat`'s alone.
**Blocks:** none.
Real dependency edges (whole stack): `tidy-engine-files -> plan-header, retarget-impl, repoint-call, repoint-facade`; `move-fidelity -> repoint-facade`; `retarget-impl -> repoint-call`. Nothing else is an edge: `spawn-record` and `apply-heartbeat` consume nothing and nothing consumes them (#586 has merged, so `spawn-record` has no merge-order constraint either).

## Scope

- [ ] **Seam in `tddy-lsp`**: `SpawnObserver`, `ProcessStart`, `ProcessOutcome`, `LspRegistry::with_spawn_observer`; `LspServerBody` reports the language server's start (pid) and its end (exit code, signal, or the kill it issues on cancel).
- [ ] **Recorder in the engine**: `SpawnRecorder` with `output`/`spawn`/`RecordedChild`; the six production spawn sites routed through it (table in State A).
- [ ] **JSONL sink and redaction**: `JsonlSpawnRecord` (append-only, one write per line, start before end); `redacted()` allow-list plus pattern redaction; environment **names only**.
- [ ] **Structural guard**: a test that fails if `Command::new` appears in the engine's `src/` outside `spawn_record.rs` and `#[cfg(test)]`.
- [ ] **CLI front end**: `restructure` installs a file sink (location: D1).
- [ ] **Daemon**: `--spawn-record <path>`; the observer wired into the registry and into `Options` of check/apply.
- [ ] **`run-index-daemon`** (#586 is merged; the history file exists on the base): records `cargo build`, the `nix develop … env -0` capture and the launch; launches the daemon as a child of a watcher shell that writes the daemon's exit line (status or signal); rotates the record at 10 MiB like the history file.
- [ ] **Docs**: README, `readiness-and-gates.md`, feature doc, `code-index-service.md`, `SKILL.md` — what is recorded, where, what is not.
- [ ] **Limits stated**: rust-analyzer's own children invisible; `SIGKILL` of the daemon visible only through the watcher; a kill during the CLI's pre-state phase (if D1 = A″) leaves no file.
- [ ] **Re-measure** `oversized-file-backends-rust.md` and append a history row.
- [ ] **Testing**: all acceptance tests passing (scoped).

## Technical changes

### State A (current)

Verified in the tree at `origin/master` `a77bca29` (read in `.worktrees/engine-fixes-plan`); re-checked after #586 merged: the script gained `archive_previous_log`/`HISTORY_FILE` and nothing that changes a site below.

Every production process start, and what records it today (**nothing does**):

| Site | What it starts | Notes |
|---|---|---|
| `tddy-code-restructuring/src/backends/rust.rs:685` (`start`) | `rust-analyzer`, pinned toolchain, piped stdio | cold CLI path only; env set at 688-690 and 699-703; `Drop` waits for it (`:981`) |
| `…/runner/compile_gate.rs:253` (`run_check`) | `cargo check --all-targets --message-format <fmt> -p …` | the baseline, the result check, the tidy and the group gate all go through it; `exit_or_kill` (`:275`) kills on cancel and **does not** kill cargo's `rustc` children |
| `…/runner/tidy/format.rs:76` (`run_rustfmt`) | `rustfmt --edition E <file>` | per written file |
| `…/apply.rs:39` (`ensure_git_worktree`) | `git rev-parse --is-inside-work-tree` | |
| `…/apply.rs:177` (`git_output`) | `git <args>` (`ls-tree`, `show` from `verify`) | public fn |
| `…/apply.rs:189` (`run_git`) | `git mv`, `git add -N` | via `git_move`, `create_tracked_file` |
| `tddy-lsp/src/server_body.rs:55` | the language server (`rust-analyzer` in the daemon) | `ctx.register_child_pid(pid)` at `:83` is the only trace; stdout/stdin piped; the child ends at four places: `:89` (missing output channel, `start_kill` with no `wait`), `:165-166` (initialize failed), `:181` (it exited on its own), `:200-201` (cancelled: graceful shutdown, then kill) |
| `run-index-daemon:252` | `nix develop --profile … -c cargo build …` | |
| `run-index-daemon:282` | `nix develop --profile … -c env -0` (dev-shell environment capture) | |
| `run-index-daemon:335` | `setsid sh -c 'echo $$ >pid; exec daemon --grpc-uds …'` | `exec` makes the daemon the pid itself: **nothing observes its exit** |

(`runner/tidy.rs:551,565` and `apply.rs:227,237,317,329`, named in the planning brief, are inside
`#[cfg(test)]` and are **not** production sites.)

`journal.rs` records one `WorkspaceEdit` per operation and nothing about processes. The daemon logs
through `log` to the file `run-index-daemon` points its stderr at; since #586 (merged) the previous run's log is
kept in `tddy-index-<tag>.history.log`. `Options` carries three sinks (`progress`, `account`, `trace`) and
defaults them all to "discard", the pattern this node follows.

`.restructure/` is created by the run-opening gate, **after** the baseline `cargo check`, and
`tests/apply_compile_gate_acceptance.rs::writes_nothing_to_a_tree_that_did_not_compile_before_the_plan`
asserts `!workspace.holds(".restructure")` after a refused baseline, and the refusal text says "Nothing
was written." This constrains where a CLI record may live (D1).

`LaunchSpec` derives `Debug, Clone, PartialEq, Eq` (`tddy-lsp/src/allowlist.rs:25`), so an observer
cannot be a field of it.

### State B (target)

- One trait, `tddy_lsp::SpawnObserver`, is the seam. `SpawnRecorder` (engine) wraps an observer and is
  the only place the engine calls `Command::…` for a child it starts. `tddy-lsp` calls an observer held
  by `LspRegistry` around its one spawn.
- A start line is written **as soon as the process exists** (pid known); the end line when it is
  waited on, with `exit` or `signal`. A run killed in between leaves a `start` with no `end`, which is
  the point.
- Front ends choose the sink. Production code calls the recorder unconditionally; **there is no
  test-only branch**. The default recorder discards, exactly as `progress: discard()` does.
- Record format, v1 (one JSON object per line; `jq`-friendly):

```json
{"v":1,"event":"start","id":"4242-17","at_unix_ms":1791221000123,"origin":"engine","purpose":"compile-gate","program":"cargo","argv":["check","--all-targets","--message-format","json","-p","origin"],"cwd":"/repo","pid":51234,"env_names":[]}
{"v":1,"event":"end","id":"4242-17","at_unix_ms":1791221004456,"pid":51234,"elapsed_ms":4333,"outcome":{"exit":101}}
{"v":1,"event":"end","id":"4242-19","at_unix_ms":1791221009000,"pid":51240,"elapsed_ms":120000,"outcome":{"signal":9}}
{"v":1,"event":"end","id":"4242-20","at_unix_ms":1791221009001,"outcome":{"spawn_failed":"No such file or directory (os error 2)"}}
```

  `id` is `<writer pid>-<counter>` so two writers never collide. `origin` is `engine`, `lsp` or
  `script`. `purpose` is a short constant per site: `rust-analyzer`, `compile-gate`, `tidy-format`,
  `git`, `language-server` (the daemon's), and for the script `build-daemon`, `capture-dev-shell-env`,
  `index-daemon`.
- **Secrets.** The observer trait carries raw arguments (in-process, never persisted). The **JSONL sink**
  applies the policy before a byte is written: (1) argv is recorded only for an allow-listed program —
  `git`, `cargo`, `rustfmt`, `rust-analyzer`, `nix`, `setsid`, `sh`; any other program is recorded with
  `argv: ["<N arguments, not recorded>"]`; (2) in an allow-listed program's argv, an argument is replaced
  by `<redacted>` when it has URL userinfo (`://user:pass@`), looks like a token (`ghp_`, `gho_`,
  `github_pat_`, `sk-`, `xox`), follows or carries `--*token*`, `--*password*`, `--*secret*`, `--*key*`,
  `--*auth*`, or is the value of `-c <key>=<value>` where the key contains `token`, `auth`, `header`,
  `password` or `secret`; (3) the environment is recorded as **names only**, never values.
- Locations (D1, D2): the daemon's record is `<TDDY_INDEX_RUNTIME_DIR>/tddy-index-<tag>.spawns.jsonl`,
  passed by the script as `--spawn-record`; the cold CLI's record is chosen by D1.
- `run-index-daemon` launches the daemon as `setsid sh -c 'daemon … & echo $! >pid; wait $!; <write end
  line from $?>'`, so the shell that survives is the daemon's **parent** and not the pid `--stop`
  signals. `--stop` and `kill <pid>` still address the daemon; the watcher turns its death into
  `{"event":"end","purpose":"index-daemon","outcome":{"exit":0}}` or `{"signal":N}` (bash reports a signal
  death as `128+N`, so a genuine exit code above 128 is indistinguishable and is recorded as a signal;
  stated in the docs).

### Delta (what's changing)

#### `tddy-lsp`
- **API**: `SpawnObserver`, `ProcessStart`, `ProcessToken`, `ProcessOutcome`; `LspRegistry::with_spawn_observer`.
- **Implementation**: `LspServerBody` carries `Option<Arc<dyn SpawnObserver>>` (set by the registry),
  reports after `command.spawn()` succeeds and at each of its four ends (State A); the `:89` end gains a `wait` so its status is observable. Unix signal via `ExitStatusExt::signal`.
- **Tests**: `tests/spawn_observer_test.rs`.

#### `tddy-code-restructuring`
- **Architecture**: `spawn_record.rs` (recorder and child wrapper), `spawn_record/jsonl.rs`,
  `spawn_record/redact.rs`. `RunPurpose` constants live beside the recorder.
- **API**: as in Draft PR contract. `apply.rs` helpers (`ensure_git_worktree`, `git_output`,
  `run_git`) take a `&SpawnRecorder`; their callers in `runner.rs` (`:76`, `:158`), `runner/comparison.rs`
  (`:33,:37,:44`) pass `options.spawns`. `run_check` and `run_rustfmt` take the recorder through
  `Tidying`/the gate's arguments.
- **Implementation**: `restructure_cli.rs::install_console` installs a `JsonlSpawnRecord` (D1).
- **Tests**: `tests/spawn_record_acceptance.rs`, `tests/every_spawn_is_recorded.rs`.

#### `tddy-index-daemon`
- **API/CLI**: `--spawn-record <PATH>`.
- **Implementation**: `main.rs::wired` builds the registry `.with_spawn_observer(...)`;
  `operations.rs` puts the same recorder in the `Options` it builds for check and apply.
- **Tests**: `tests/spawn_record_acceptance.rs`; production test additions in
  `tests/detached_daemon_production.rs` (`#[ignore]`).

#### Scripts and docs
- `run-index-daemon`: shell functions `record_start`/`record_end` (JSON-escaped) around the three
  script-side spawns; the watcher launch; rotation of the spawn record next to the history file's.
- `SKILL.md`: how to read the record (`jq`), what it cannot show. Wrap updates the feature doc,
  README, `readiness-and-gates.md`, `code-index-service.md`.

## Implementation milestones

- [ ] **M1 — contract.** Types and signatures above compile; `Options.spawns` exists and defaults to
  discard; the four test files exist and **fail by assertion** (`expected N records, found 0`), with
  `./test -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon` run scoped and the failing names
  listed in this file.
- [ ] **M2 — the engine's sites.** The six production sites in State A go through the recorder;
  `every_spawn_is_recorded` is green (zero `Command::new` hits outside `spawn_record.rs`/tests);
  `spawn_record_acceptance` cases 1–4 green.
- [ ] **M3 — the JSONL sink and the policy.** `redact.rs` table test green; the file is opened
  `O_APPEND`, one `write` per line, flushed; a second run appends (case 7).
- [ ] **M4 — `tddy-lsp` and the daemon.** `spawn_observer_test` green; the daemon's acceptance test
  green; `--spawn-record` documented in `--help`.
- [ ] **M5 — `run-index-daemon`** *(#586 merged; the history file is on the base)*. Script-side records and the watcher; the two
  `#[ignore]`d production tests pass on a machine with the dev shell (command in Testing plan).
- [ ] **M6 — docs and records.** `SKILL.md`, feature doc section, README, `readiness-and-gates.md`,
  `code-index-service.md`; history row appended to `oversized-file-backends-rust.md`; todo narrowed.
- [ ] **M7 — gates, scoped.** `./test -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon`,
  `cargo clippy -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon --all-targets -- -D warnings`,
  `cargo fmt --check`; **the whole workspace is CI's** (`scripts/ci-status.sh`), not claimed from here.

## Testing plan

### Testing strategy

**Primary test approach: integration at the engine's own seam, with the real `git`/`cargo`/`rustfmt`
and no real rust-analyzer.** The fixture is the one `tests/apply_compile_gate_acceptance.rs` already
uses (`harness::moving_the_test_binary`): a test-binary move is authored by the engine "without asking
the server anything", so a full `runner::apply` spawns `git`, `cargo check` (baseline, result, tidy) and
`rustfmt` against a temporary workspace, with `fake_lsp` standing in for the language server.

**How a spawn is observed without a real rust-analyzer.** The seam is `SpawnObserver`. A test implements
the trait itself (a `Vec<(ProcessStart, ProcessOutcome)>` behind a mutex, defined in
`tests/harness/mod.rs`), builds `SpawnRecorder::new(Arc::new(collected))` and puts it in `Options.spawns`;
`tddy-lsp`'s test hands the same kind of observer to `LspRegistry::with_spawn_observer`. **Production code
has no test-only branch**: it calls the recorder on every path, and the recorder's default discards. The
trait is the seam a host has anyway (the daemon's JSONL sink is the second implementation), so no type
exists only for tests. A structural test (`every_spawn_is_recorded`, same technique as
`tests/library_returns_its_results.rs`) is what keeps a future spawn site from bypassing it.

### Testing options analysis

#### Option 1 (chosen): recorder trait + fixtures above
**Test level**: Integration (engine, `tddy-lsp`, daemon) + structural source scan + `#[ignore]`d production
tests for the script.
**Scope**: argv, cwd, pid presence, exit status, signal, failed spawn, redaction, append-only, every site
reachable, daemon wiring, the watcher.
**Trade-offs**: real `cargo check` on a tiny crate costs seconds per test (same cost class as
`apply_compile_gate_acceptance`); the script's tests need `nix develop` and minutes, so they are
production tests by `docs/dev/guides/testing.md` and **not part of `./test`**.

#### Option 2 (rejected): observe through `strace`/`dtruss`-style process tracing
Not portable (`strace` is Linux-only, `dtruss` needs root on macOS), not deterministic, and it would observe rust-analyzer's children — which would claim
more than the design delivers.

#### Option 3 (rejected): a global `OnceLock` recorder
No signature churn, but tests share one process under `cargo test`, so two tests cannot hold different
sinks; `nextest`'s process-per-test hides it, plain `./verify` does not.

### Coverage requirements

- [ ] Happy path: every site's start and end, with argv and cwd.
- [ ] Error scenarios: non-zero exit (baseline `cargo check` 101), signal, failed spawn.
- [ ] Edge cases: secret-looking argv and environment values; a program not on the allow-list; a record
  file that cannot be written (the run still succeeds).
- [ ] Integration points: engine to `tddy-lsp` observer to daemon file.
- [ ] Actual effects: file contents parsed as JSON line by line, not substring-matched.

## Acceptance tests

Names read as behaviour; `Given/When/Then` bodies as in `cancellation_acceptance.rs`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/spawn_record_acceptance.rs`
Fixture: `harness::a_workspace_whose_test_binary_stands_alone()` and a collecting observer; `fake_lsp`
as the language server; **no rust-analyzer**; real `git`, `cargo`, `rustfmt`.
- [ ] `an_apply_records_every_process_it_starts_with_its_argv_cwd_and_exit_status` — the record holds
  `git rev-parse --is-inside-work-tree`, a baseline `cargo check --all-targets --message-format json -p
  origin`, the result check, the tidy's check, `rustfmt` for each file the tidy formats and `git` for the move (which of these a
  given fixture reaches is confirmed at M1; the test asserts the ones it reaches, and the union over the
  cases covers all six engine sites);
  every `cwd` equals the workspace root; each start is paired with an end whose exit is `0`.
  *Fails today:* `Options.spawns` is never called, so `0 records, expected at least 5` (after M1).
- [ ] `a_refused_baseline_records_the_failing_cargo_check_with_its_exit_status` — rewrite the origin lib
  so it does not compile; the record holds the baseline `cargo check` ending `exit 101` and **no**
  `rustfmt`; and `.restructure` still does not exist (the existing guarantee). *Fails today:* no records.
- [ ] `a_process_killed_by_a_signal_is_recorded_with_the_signal_and_no_exit_code` (`#[cfg(unix)]`) —
  `recorder.output("t", sh -c 'kill -9 $$')` ends `{"signal":9}`. *Fails today:* the contract stub records nothing, so no `end` line exists to read.
- [ ] `a_program_that_cannot_be_started_is_recorded_as_a_failed_spawn` — a program name that does not
  exist: one `end` line with `spawn_failed`, no `pid`.
- [ ] `a_cancelled_compile_gate_records_the_cargo_it_killed_as_signalled` — the fixture crate's
  `build.rs` sleeps for a minute (the incident's shape: a build script that never finishes); the run's
  token is cancelled after the baseline's `start` line appears; the run returns `CallerStopped`, and the
  record shows that `cargo` ended `signal 9`. It also asserts the record names **no** build-script process
  — the stated limit.
- [ ] `a_record_never_carries_an_environment_value_or_a_secret_looking_argument` — a recorder over the
  JSONL sink running `git -c http.extraheader=Authorization:\ Bearer\ abc --version https://u:p@host/x
  --token=ghp_abc` (offline: `--version` ignores the rest), env `SECRET=value`: the file contains `<redacted>` for each, contains no `value`,
  `abc` or `u:p`, and lists `SECRET` among `env_names`.
- [ ] `a_program_off_the_allow_list_has_its_arguments_withheld` — `argv` is the single placeholder.
- [ ] `a_second_run_appends_to_the_record_instead_of_replacing_it` — two applies against one file leave
  the first run's lines first, byte for byte.
- [ ] `a_record_that_cannot_be_written_does_not_fail_the_run` — the sink points at a read-only path;
  the apply still returns its `RunSummary`.
- [ ] `a_language_server_the_backend_starts_itself_is_recorded_with_the_names_of_its_pinned_environment`
  — the one engine site `fake_lsp`-over-the-registry cannot reach: `RustBackend::new(<fake_lsp>,
  <tmp cargo home>, <tmp rustup home holding a settings.toml>)` with a recorder, one `anchor_for`; the
  record holds a `rust-analyzer`-purpose start whose `env_names` are `CARGO_HOME`, `RUSTUP_HOME`,
  `RUSTUP_TOOLCHAIN` (and `CARGO`/`RUSTC` when the toolchain's binaries exist), no values, and an end when
  the backend is dropped (`Drop` closes stdin and waits). *Fails today:* no record. **Unverified at
  planning time:** that `fake_lsp` completes the self-started handshake this way (no existing test
  starts a backend this way; the cancellation suites use the bridge) — confirm at M1; if it does not, the
  site is covered by the structural test and one `#[ignore]`d live case in `move_module_to_crate_acceptance`
  style instead.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/every_spawn_is_recorded.rs`
- [ ] `no_production_code_starts_a_process_except_through_the_recorder` — scans `src/**/*.rs` outside
  `#[cfg(test)]` for `Command::new` and `.spawn()`; allowed only in `spawn_record.rs`. *Fails today* with
  the six sites listed by `file:line` (the same output a reader needs to find them).

### `tddy-code-restructuring` — unit, `packages/tddy-code-restructuring/src/spawn_record/redact.rs`
- [ ] table test `redacted_cases` — one row per rule in State B (userinfo, token prefixes, `--token`
  both forms, `-c` with sensitive key, non-allow-listed program).

### `tddy-lsp` — `packages/tddy-lsp/tests/spawn_observer_test.rs` (fixture: `fake_lsp`)
- [ ] `a_language_server_launch_is_reported_with_its_program_args_cwd_and_pid` — registry with an
  observer, `get_or_spawn`, then the start is reported with `program == CARGO_BIN_EXE_fake_lsp`, the
  root as `cwd`, a pid equal to the one `ctx.register_child_pid` got.
- [ ] `a_language_server_that_is_shut_down_is_reported_ended_with_how_it_died` — `shutdown_all`, then an
  end with `Signalled { 9 }` or `Exited { 0 }` (the graceful `exit` path wins when the fake honours it;
  the test accepts exactly those two and nothing else).
- [ ] `a_server_that_exits_before_the_handshake_is_reported_ended_with_its_exit_code` —
  `--exit-immediately`; end `Exited { 0 }` although initialization failed.
- [ ] `a_registry_without_an_observer_behaves_exactly_as_before` — the existing `registry_reuse_test`
  suite stays green unchanged (cited, not copied).
  *All three fail today:* the registry has no observer to call (after M1: zero events).

### `tddy-index-daemon` — `packages/tddy-index-daemon/tests/spawn_record_acceptance.rs` (fixture: `fake_lsp`)
- [ ] `a_served_warm_appends_the_language_server_it_started_to_the_record_file` — service wired with a
  `JsonlSpawnRecord` on a temp file; a `Warm` against `--loads-crate-graph` leaves one start line for the
  fake server with `origin: "lsp"`.
- [ ] `a_restarted_daemon_appends_after_the_previous_runs_lines` — two services over one file.
- [ ] `the_spawn_record_flag_names_the_file_the_daemon_writes` — `tddy-index-daemon --help` documents
  `--spawn-record`, and a single-shot run with it creates the file (uses the daemon's existing
  single-shot lifetime, no socket).

### Production tests (`#[ignore]`d) — `packages/tddy-index-daemon/tests/detached_daemon_production.rs`
- [ ] `a_daemon_killed_with_sigkill_leaves_an_exit_line_naming_the_signal` — real `run-index-daemon`
  with its own `TDDY_INDEX_RUNTIME_DIR`; `kill -9 <pid from the pid file>`; within 5 s the record ends the
  `index-daemon` entry with `{"signal":9}`.
- [ ] `an_orderly_stop_leaves_an_exit_line_with_status_zero_and_the_script_side_spawns_are_recorded` —
  `--stop`, then the record holds `capture-dev-shell-env` and `index-daemon` starts (and `build-daemon`
  when `TDDY_INDEX_DAEMON_BIN` is unset) and the `index-daemon` end.
  *Run:* `./dev cargo test -p tddy-index-daemon --test detached_daemon_production -- --ignored --test-threads=1`.

## Technical Debt & Production Readiness

*(empty — populated during development)*

## Decisions & Trade-offs

Decisions already taken (quoted from the developer, 2026-10-05):
- "Log-history fix is its own PR #586 — NOT in this stack." This node therefore neither includes nor
  redoes the history file; it lands after #586 (merge order).
- 8-node decomposition approved.

### D1 (taken: recommended option A″): where does the cold CLI's record live?

**Taken at the surface commit: A″, as recommended.** The contract does not yet install a file sink in `restructure_cli.rs`; green does, and holds spawns before `.restructure/` exists in memory.

The todo says "for a CLI run to a file beside the plan's journal". The code says that cannot be done
without breaking a stated guarantee: the baseline `cargo check` runs **before** `.restructure/` exists,
the refusal says "Nothing was written", and
`apply_compile_gate_acceptance::writes_nothing_to_a_tree_that_did_not_compile_before_the_plan` asserts
`.restructure` is absent.

| Option | Cost |
|---|---|
| **A. Beside the journal, `<root>/.restructure/spawns.jsonl` (repo-scoped), created as soon as the first record is written** | Honours the todo; **creates `.restructure/` before the baseline gate**, so the refusal text and that test must change ("Nothing was written to the tree" and the test asserting only `golden.rs`/sources). A changed guarantee: the developer's call. |
| **A″. Same file, but spawns before the state directory exists are held in memory and written at the moment `.restructure/` is created; a baseline-refused run writes nothing** | Keeps the guarantee and the test. **Limit:** a run killed during `git rev-parse` or the baseline `cargo check` (the longest pre-state wait) leaves no record on the cold CLI path. The daemon path is unaffected (its file is always open). |
| **B. Outside the tree** (`$TMPDIR/tddy-restructure-<digest>.spawns.jsonl`, or an env var naming the file) | Never touches the tree. A default location in `$TMPDIR` is lost with a `nix develop` shell's `TMPDIR` (the exact problem `run-index-daemon`'s `durable_tmpdir` exists for), and re-implementing that logic in Rust would drift from the script. |

**Recommendation: A″** for the cold CLI, plus the daemon's own always-open file. It keeps an existing,
tested promise, and the one gap it leaves is named. Choose A if you would rather the record cover the
baseline check and accept a one-line wording change.

### D2 (taken: recommended, a sibling `.spawns.jsonl`): the daemon's record — beside the history file or inside it?

**Taken: a sibling file**, re-checked against the merged #586 code (the history file is still headered text written by a shell function). The daemon's flag is `--spawn-record <PATH>`; the script chooses the path.

PR #586's history file is a **text copy of the previous run's log, rewritten by a shell function** with
`=== previous run … ===` headers and rotated at 10 MiB. A spawn record written by the daemon *while it
runs* cannot live in a file the script concatenates into after the fact without the two interleaving, and
JSONL parsed by `jq` cannot tolerate the headers.

**Recommendation:** a sibling file, `tddy-index-<tag>.spawns.jsonl`, in the same `TDDY_INDEX_RUNTIME_DIR`,
written by the daemon and the script, **never truncated by the launcher**, rotated by the script at
10 MiB to `.spawns.jsonl.1` (the history file's rule). Alternative: write into `.history.log` — rejected
for the interleaving above.

### D3 (taken: recommended, `tddy-lsp`): which crate owns the seam trait?

**Taken: `tddy-lsp`** (`src/spawn_observer.rs`, re-exported from `lib.rs`). No new crate edge.

`tddy-lsp` cannot depend on `tddy-code-restructuring` (the engine depends on `tddy-lsp`), and both must
report. **Recommendation: the trait lives in `tddy-lsp`** (no new crate edge; `tddy-lsp` is edited
anyway for `server_body.rs`; blast radius = engine, daemon, `tddy-lsp-executor`). Alternative:
`tddy-task`, the lowest crate both already depend on and the one that already models a task's child pid
(`register_child_pid`); a more natural home, a wider rebuild ripple, and a core crate edited by an
observability node.

### D4 (taken: recommended, explicit): explicit recorder in `Options` vs a process-global

**Taken: explicit.** `Options.spawns` and `RustBackend::with_spawn_recorder` exist, defaulting to `SpawnRecorder::discard()`.

**Recommendation: explicit** (`Options.spawns`, `RustBackend::with_spawn_recorder`), because this repo's
own pattern for "where a run's side channel goes" is a sink on `Options` defaulting to `discard()`
(`progress`, `account`, `trace`), and because tests then hold different sinks in one process. Cost:
`apply.rs`'s three helpers gain a parameter (callers: `runner.rs:76`, `:158`, `comparison.rs:33,37,44`).
A global (`OnceLock`, like `log::set_logger`) avoids that and is rejected for test isolation.

### D5 (settled by the code, recorded here): the record is written after the spawn, not before

A pid is the point of the record, and it exists only after `spawn()`. A process cannot be recorded
"in flight" before it exists; a spawn that fails is recorded as a start-less end. The `start` line is the
first thing written after `spawn()` returns, before the process is waited on.

### D6 (taken: recommended, left out): the per-operation line (todo item 3)

**Taken: left out**; the todo stays open at item 3.

The todo's third item — print the op id in the apply output so a log line can be joined to the journal
record — is **not** in this node's scope in the brief. **Recommendation: leave it out** and keep it open
in the narrowed todo; the record carries `at_unix_ms` and `pid`, and a join by time is possible by hand.
It would need `runner.rs` to tell the recorder which operation is running. Say if you want it here.

### D7 (settled): the watcher shell for the daemon's exit

`exec` is what makes `$$` the daemon's pid (`run-index-daemon`'s comment at `:314-317`), and the reason
it was chosen is that `--stop` must signal the daemon, not a wrapper. A watcher that runs the daemon as
a **background child** and `wait`s on it keeps `$!` as the daemon's pid (written to the pid file) while
making the shell the parent that can read `$?`. `SIGKILL` of the daemon is then visible, which no handler
inside the daemon could ever show. Cost: one more long-lived process per daemon (the watcher). Alternative
(signal handlers inside the daemon only) records everything **except** `SIGKILL`, the very case the todo
names — rejected.

### Decisions taken in the surface commit (not in the plan above)

- **`JsonlSpawnRecord::open` refuses.** It returns an error naming `TODO(spawn-record)`, so a front end given `--spawn-record` fails loudly (the daemon logs the error and exits non-zero) rather than believing it records. Consequence: the tests that use the file sink fail at `open` with that message, not by "found 0 lines". Tests that use a collecting observer fail by assertion (zero records). Green replaces `open` with the `O_APPEND` open and the observer methods with the line writes.
- **`SpawnRecorder::output`/`spawn` run the command and tell nobody**, marked `TODO(spawn-record)`: the process behaves exactly as without a recorder. No production site calls the recorder yet, which is what `every_spawn_is_recorded` reports.
- **`redacted` withholds every argument** until the policy is written, so nothing wired to it can leak one.
- **`LspRegistry` holds the observer and exposes it (`spawn_observer()`)**; `LspServerBody` is not changed (its public fields are built literally by `tests/server_body_test.rs`, and adding one would edit that suite). Green decides how the body receives it; the contract's "`LspServerBody` carries an `Option<Arc<dyn SpawnObserver>>`" is therefore not yet true. The daemon can build the `Options.spawns` recorder from `servers.spawn_observer()`.
- **`a_second_run_appends...` uses two recorders over one file running `git version`** rather than two full applies (same property, seconds instead of tens of seconds).
- **`a_record_that_cannot_be_written_does_not_fail_the_run`** gets a sink whose file opens and then refuses every write: a named pipe whose reader has gone (`mkfifo`, unix only). A read-only path cannot do it, because `open` would simply fail before any run.
- **The fake-server self-start is verified**: `RustBackend::new(<fake_lsp>, …)` with a settings file completes `initialize` (the question after it is answered "`level` is not an item the file defines"), so the `rust-analyzer`-purpose test needs no `#[ignore]`d fallback.
- **`a_registry_without_an_observer_behaves_exactly_as_before`** is not a new test: it is the existing `registry_reuse_test` suite, cited as the plan says; run green at this commit (9 passed).
- Net growth in `backends/rust.rs` is 11 lines (import, field with its `allow(dead_code)` attribute, a documented builder, two constructor lines). Green drops the attribute and should trim the builder's doc to hold the planned +8.

### Failing at the surface commit

Engine `spawn_record_acceptance` (10 of 10), `every_spawn_is_recorded` (1), unit `spawn_record::redact::tests::redacted_cases` (1), `tddy-lsp` `spawn_observer_test` (3 of 3), daemon `spawn_record_acceptance` (3 of 3), and the two `#[ignore]`d production tests (run once locally: both fail at "the script gave the daemon a spawn record"). `registry_reuse_test`, `server_body_test`, `apply_compile_gate_acceptance`, `apply_tidy_acceptance` and `library_returns_its_results` stay green.

### Honest limits (also in the docs)

- rust-analyzer's own children (build scripts, proc-macro server) are never seen.
- A signal death behind a shell is `128+N`, so an exit status above 128 is read as a signal.
- If the process that owns the file is killed between `spawn()` and the first write, that process has no
  line.
- The record explains *what* ran and *how it ended*, not *why a process stalled*: for a stall the
  heartbeat (`apply-heartbeat`) is the instrument.

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
*(empty)*

### @validate-tests
*(empty)*

### @prod-ready
*(empty)*

### @analyze-clean-code
*(empty)*

## TODO

- [x] Record initial discovery (`2026-10-05-sharpen-spawn-record-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-05-sharpen-spawn-record.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (shared append-point; eight nodes would conflict, so planning does not edit it)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-lsp -p tddy-code-restructuring -p tddy-index-daemon`) — verify 100% pass; whole-workspace health is CI's
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-05-sharpen-spawn-record-initial-discovery.md`; narrows (does not delete) the `no-record-of-what-an-apply-executes` todo unless D6 is taken
- [ ] USER REVIEW — work complete, decide next steps

## Successor PRs

Forward links only (parent to child): none depends on this node. The stack's later nodes
(`feature/sharpen/apply-heartbeat`, `feature/sharpen/plan-header`, `feature/sharpen/retarget-impl`,
`feature/sharpen/repoint-call`, `feature/sharpen/repoint-facade`) neither consume nor extend it.
