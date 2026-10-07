# tddy-code-restructuring

Replays a JSONL plan of named Rust refactoring intents via rust-analyzer through `tddy-lsp`.

**Feature doc:** [docs/ft/coder/rust-code-restructuring.md](../../docs/ft/coder/rust-code-restructuring.md)

## CLI

Exposed via `tddy-tools restructure`:

- `apply <plan.jsonl> [--dry-run] [--resume] [--from N|ID] [--stop-after N]`
- `load <plan.jsonl>...`, `unload <plan.jsonl>... | --all`, `plans` — hold plans in the index daemon's plan store (they need the daemon)
- `status <plan.jsonl>`
- `check <plan.jsonl> [--deep] [--budget LINES]` — `--deep` also reports the blast radius of every cross-crate move and prints each operation's own notes (for `repoint_facade_imports`, the paths it would rewrite); `--budget` reports the files the plan names that have more than LINES **production lines** (before the first `#[cfg(test)]` that opens a `mod`), as a record rather than a gate
- `snapshot <plan.jsonl>` — rewrites the plan's header, or writes one when the plan has none (its first line is an operation); for a plan of item anchors it also re-resolves them against the current tree
- `anchors <file.rs> --items A,B,C | --at L:C[-L:C]` — emits the anchor a plan carries; `--items` takes bare names, `krate::module::Alpha`, and `<Type>` / `<Type>#N` for an inherent `impl` block (`item_anchor::parse_item_list` is the one rule for every front end)
- `verify --against <git-ref>` — compares logical statements, and excuses and counts what an `extract_module` always causes; `--retarget OLD=NEW` (repeatable) makes it account for a declared `retarget_impl`, and `--repoint OLD=NEW` (repeatable) for a declared `repoint_call`
- `warm` — loads the tree's crate graph into the index daemon (it needs the daemon)

A run waits until the server is ready or until its caller stops waiting; there is no budget flag.
While it waits it says so on a fixed heartbeat (`WAIT_HEARTBEAT`, 30 s): the stage it is in, how long
it has waited, which server it is waiting on, and how long that server has said nothing new — so a
silent server is diagnosed rather than given a deadline.
"Ready" means rust-analyzer has reported itself quiescent (or never sends the status at all) **and**
healthy: an index whose health is anything but `ok` is refused, quoting the server's message.

Every writing `apply` ends with a **tidy** once the tree compiles: the imports rustc reports unused are
removed, an import only the parent's tests use is gated `#[cfg(test)]`, and every file written is
formatted. See [docs/readiness-and-gates.md](docs/readiness-and-gates.md#the-tidy).

Every writing `apply` is bracketed by `cargo check --all-targets` over the packages it touches. A
tree that did not compile before the plan is refused with nothing written; a tree the plan's
accepted operations left uncompilable fails the run, with the compiler's errors, the edits left on
disk and the way to roll them back. `check --deep` does not run the compiler, so a clean deep check
is not a promise that the applied tree builds. Operations a plan gives the same `"group"` are gated at the
group's end and rolled back exactly when it does not compile. See
[docs/readiness-and-gates.md](docs/readiness-and-gates.md).

A plan anchors an operation by **item**: a crate-rooted path such as `tddy_core::workflow::Stack::new`
plus a range relative to that item, resolved through rust-analyzer's outline when the run opens. An edit
elsewhere in the file leaves the anchor correct; an edit to the item itself is refused, naming it. See
[docs/item-anchors.md](docs/item-anchors.md).

A plan is read through a **plan store** (`plan_store.rs`): operations carry stable ids, the applied plan's
pending anchors are rewritten after each operation, and the plan is written back. See
[docs/plan-store.md](docs/plan-store.md).

The plan store also keeps **every other loaded plan** current (`PlanStore::{fold_foreign_op,
reresolve_files, stale_ops}`): an operation applied from one plan is folded into the rest, files that
change underneath the daemon are re-resolved, and an operation whose item changed or was edited by
another plan is **stale** — reported by `ListPlans`, `PlanStatus` and `check`, and refused by `apply`
before any write. See [docs/plan-store.md](docs/plan-store.md#live-plans).

A cross-crate move (`move_module_to_crate`, `move_cluster_to_crate`) reads every path the moved file
names — `use` items at any depth and bodies — from one **path survey** (`crate_move/survey.rs`), resolved
by segment and followed through the origin's re-exports to the defining crate; the rewrite, the
dependency-back test and the destination's manifest are all derived from it. See
[docs/path-survey.md](docs/path-survey.md).

Plans hold intents only — no source text (`text` / `code` / `content` refused). Unsupported operations are hard errors.

## Driving it from something other than a command line

Two properties make that possible, and both are load-bearing:

- **Every entry point takes the workspace root it acts on.** Nothing here reads the process
  directory, so one process can serve several worktrees. `StatePaths`, `open_run`, `restore_ledger`
  and `commit_operation` are public so a host can drive the apply loop without re-deriving
  `.restructure/` or re-implementing the write-ahead commit sequence — note `open_run` is the only
  concurrency gate that exists and there is no lock file, so a host serializes per root itself.
- **Nothing here prints.** Results come back as values — `RunSummary`, `PlanProgress`,
  `Vec<Finding>`, `Range`, `Comparison` — and progress goes to caller-owned sinks on `Options`. Only
  `restructure_cli` writes to a console, and a test reads this crate's sources to keep that true: a
  host speaking a protocol on its own stdout would otherwise have its frames corrupted by a finding.

A backend closes every document it opens before an entry point returns, so a server shared across
requests reads the tree on disk again after each one. See
[docs/readiness-and-gates.md](docs/readiness-and-gates.md#documents-are-closed-when-an-operation-ends).

`tddy-index-daemon` is that host. See
[warm-code-intelligence-daemon.md](../../docs/ft/coder/warm-code-intelligence-daemon.md).

## What a run executes

A run starts `git`, `cargo check`, `rustfmt` and, on the cold command line, a language server. Each
one goes through a `SpawnRecorder` (`src/spawn_record.rs`), which reports a start (argv, cwd, pid,
the **names** of the environment it was given) and an end (exit code, terminating signal, or a
spawn that failed) to a `SpawnObserver`. The default recorder tells nobody, as `Options::progress`
defaults to discard; a host that wants a record installs a sink on `Options::spawns`, or
`RustBackend::with_spawn_recorder` for the server the backend starts itself.

The only sink shipped here is `JsonlSpawnRecord` (`spawn_record/jsonl.rs`): one JSON object per
line, appended `O_APPEND`, never truncated, so a restarted writer adds after its predecessor. A
`start` line is written as soon as the process exists; the `end` line when it is waited on — so a
run killed in between leaves a `start` with no `end`, which is the point. Times are Unix
milliseconds (`at_unix_ms`), so no date-formatting dependency is pulled in.

**Secrets never reach the file.** `argv` is recorded only for an allow-listed program (`git`,
`cargo`, `rustfmt`, `rust-analyzer`, `nix`, `setsid`, `sh`); any other program is recorded with a
count of its arguments and no more. Within an allow-listed program, an argument carrying URL
userinfo, a token prefix, a `--*token*`-shaped flag's value, or the value of a `-c <key>=<value>`
whose key names a credential is replaced by `<redacted>`. The environment is recorded as **names
only**.

**What it cannot show.** rust-analyzer's own children — build scripts and the proc-macro server —
are started by rust-analyzer, not by anything this crate runs, so they are never recorded; a
`SIGKILL` is visible only where a parent watched the child; and a cold CLI run killed before
`.restructure/` exists leaves no record (see below). A stall is the heartbeat's question, not this
record's.

`a record that cannot be written never fails the run`: the sink logs the failure and stops recording
to that file. The structural test `tests/every_spawn_is_recorded.rs` fails if a `Command::new` or
`.spawn()` appears in production source outside `spawn_record.rs`, so the next spawn site cannot
bypass the recorder.

## Operations (v1)

`extract_method`, `extract_variable`, `rename_symbol`, `extract_module` (`reexport`, `to_file`),
`extract_module_to_file`, `extract_trait`, `inline_method`, `remove_unused_param` (`name`: the parameter),
`convert_tuple_return_to_struct` (`name`: the new struct), `move_module_to_crate` (`to`,
`reexport`), `move_cluster_to_crate` (`also`, `to`, `reexport`), `move_test_binary_to_crate` (`to`), `move_item` (`to`,
`name`, `reexport`), `reparent_module` (`to`, `reexport`), `retarget_impl` (`to_type`),
`repoint_call` (`callee`) and `repoint_facade_imports` (no field: an anchor only).

`retarget_impl` rewrites an inherent `impl`'s self type to another type of the same crate — the whole
block, or the run of members its anchor names (the block is split at the run, in place). It re-points
the `Old::` paths the moved members wrote, adds one `use`, and refuses before writing when a moved
member reads a field the new type lacks; see [docs/retarget-impl.md](docs/retarget-impl.md).

`repoint_call` rewrites the part of a call **in front of** its argument list, keeping the arguments
byte for byte: one call's callee (an `item` anchor with a relative range over the call and `callee` =
the new callee), or, anchored on a method with no range, the receiver of every call of that method
(`callee` = a `$receiver<hops>.<method>` template whose hops are inserted after each receiver). The
bulk form refuses, all at once, every reference that is not a method call; see
[docs/repoint-call.md](docs/repoint-call.md).

`repoint_facade_imports` names every path of one file (or of every file of one module) that goes
through a `pub use` of another crate by the path where the item is **defined**, in `use` items at any
depth — splitting a grouped `use` whose members need different qualifiers — and in bodies; comments,
strings and the crate's own paths are untouched, and `check --deep` lists the paths it would rewrite.
It is text-only: no server is asked anything. See
[docs/repoint-facade.md](docs/repoint-facade.md).

`move_item` and `reparent_module` move items, and a module with its directory, **within one crate**, with
`reexport: outside` leaving a facade only for what another package reaches; see
[docs/same-crate-moves.md](docs/same-crate-moves.md).

`remove_unused_param` and `convert_tuple_return_to_struct` rewrite every caller as well as the declaration,
through rust-analyzer's own assists; see [docs/signature-assists.md](docs/signature-assists.md).

`change_param_type`, `add_param`, `reorder_params`, `change_return_type`, `add_call_arg`, `remove_call_arg`,
`change_call_arg` and `reorder_call_args` edit one declaration or one call, so a group pairs a signature
change with its callers; see [docs/signature-rewrites.md](docs/signature-rewrites.md).

`move_cluster_to_crate` moves a **set** of modules as one unit — `anchor` is the first member and
`also` names the rest — in a single edit, so the tree is never half-moved. That is what makes a
mutually-referencing group movable at all: moved one at a time, each module's reference to a sibling
still in the origin would make the destination depend on the crate it left, and no ordering of
one-module operations can resolve a cycle. A plain `check` reports such a set when a plan spreads it
over separate moves, at its first operation — see
[docs/readiness-and-gates.md](docs/readiness-and-gates.md#the-partial-cluster-finding). It also reports a
moved module's body path to a module that stays behind, and a destination that already has the module —
see [the body-path and merge findings](docs/readiness-and-gates.md#the-body-path-and-merge-findings).

`move_test_binary_to_crate` moves `<crate>/tests/<name>.rs` to the crate it exercises. A test binary
is a different shape from a module — cargo auto-discovers it, so there is no `mod` line to remove;
nothing can reference it, so `reexport` is refused; and the destination gains
`[dev-dependencies]`, not `[dependencies]`. See
[docs/test-binary-moves.md](docs/test-binary-moves.md).
[docs/facades.md](docs/facades.md) — the facade and `pub mod` a cross-crate move writes.

Run state is keyed by the **plan**, at `<root>/.restructure/<plan stem>-<digest>/`, so one plan
follows another under the same root without hand-archiving and `--resume` resumes the plan it was
given.

A cold CLI run's process record lives at `<root>/.restructure/spawns.jsonl` — beside the state
directory, not inside it, because `.restructure/` is created only **after** the baseline
`cargo check`, and a refusal says "nothing was written". `ColdRunSpawnRecord`
(`spawn_record/deferred.rs`) therefore holds every line in memory until `.restructure/` exists and
writes nothing for a run refused before its state directory is created. The gap it leaves is
stated: a run killed during the baseline check — the longest wait before `.restructure/` exists —
leaves no record on this path. The daemon's own record is always open and has no such gap.

## Where the code lives

`src/` is organised by what a file decides. Over the 500 production-line budget are `backends/rust.rs` and
`crate_move/test_binary.rs` (records in `docs/code-issues/`).

| Area | Modules |
|---|---|
| Plan vocabulary | `plan.rs`, `plan/refactor_kind.rs` (`RefactorKind`), `plan/codec.rs` with `plan/codec/{file_hint,groups}.rs`, `plan/item_path.rs` |
| Journal | `journal.rs`, `journal/group.rs` (`PreImage`, `OpenGroup`) |
| Process record | `spawn_record.rs` with `spawn_record/{jsonl,deferred,redact}.rs` (`SpawnRecorder`, `JsonlSpawnRecord`, `ColdRunSpawnRecord`, `redacted`) |
| Plan store | `plan_store.rs`, `plan_store/refresh.rs`, `plan_store/live.rs`, `plan_store/live/fold.rs` |
| Runner | `runner/entry_points.rs` with `anchor_entry_points.rs`, `check_entry_points.rs`, `store_run.rs` (and `store_run/applied_op_record.rs`); `runner/group_gate.rs`; `runner/tidy.rs` with `tidy/{diagnostics,gating,format}.rs`; `runner/{budget,comparison,compile_gate,options,outcome,rehearsal,resume}.rs` |
| Verify | `verify.rs`, `verify/statements.rs`, `verify/tokens.rs`, `verify/retarget.rs` (`Declared`, R1 and R2), `verify/repoint.rs` (`Repoint`, R-call) |
| Rust backend | `backends/rust.rs`, and beside it `item_move/` (with `canonical_paths.rs`, `doc_links.rs`), `retarget_impl/` (with `outline.rs`, `rewrite.rs`, `fields.rs`, `imports.rs`, `preflight.rs`), `repoint_call/` (with `single.rs`, `sites.rs`, `receivers.rs`), `repoint_facade/` (with `scope.rs`, `rewrite.rs`, `group.rs`, `refusals.rs`), `module_reparent/`, `signature_rewrites`, `return_type`, `line_diff`, `placeholder_checks`, `lsp_edits`, `import_text`, `module_text`, `visibility`, `seam_survey`, `facade`, `server_process`, `prelude_shadow`, `relative_visibility`, `inline_paths`, `imports`, `early_return`, `chatter` |
| Cross-crate moves | `crate_move/{moving,cluster,source_scan}.rs` with `moving/facade_writer.rs`, `cluster/stranded.rs`, `source_scan/{module_items,sighting_walk}.rs`; `crate_move/test_binary.rs`. `crate_move::survey` and `crate_move::reexports` are `pub(crate)`, read by `item_move`'s canonical-path pass |

Rust-analyzer's progress is throttled per token to one line every two seconds in the printed stream
(`ServerChatter`'s default); a host that serves structured events builds `ServerChatter::unthrottled()`
so every phase reaches its clients. The scanners that mask strings and comments handle non-ASCII
identifiers.

## Authored transformations

Most operations delegate to a rust-analyzer assist; four are written here, because no assist
performs them: `extract_class`, the facade `use` line, the cross-crate moves, and `retarget_impl`.

rust-analyzer has no cross-crate move, so these have nothing to delegate to. What
keeps it honest is that it is engine-**informed**: every caller it rewrites comes from a real
`textDocument/references` result, never a text search, and every acceptance test ends in
`cargo check` — because a tree that reads correctly and does not compile is exactly the failure an
authored transformation invites. That is not hypothetical: the operation's first live run emitted a
manifest-less move that passed 271 unit tests and failed to build, on both the facade and the
no-facade path.

What it costs is that the moved file's header pass is mechanical rather than typed. A module that has
left its crate cannot be type-checked until it is *in* the destination, so there is no server to ask
which names went unresolved — the operation re-points the `crate::`/`super::` qualifiers at the head
of `use` declarations, which changed meaning by definition, and nothing else. The
[feature doc's known limitations](../../docs/ft/coder/rust-code-restructuring.md#known-limitations)
list what that leaves for the build to catch.

## Between the assist and the result

The delegated operations take an assist's output as a draft, not a result. The import pass restores
what the cut stranded, weighing only the names the seam lost and applying an import only when it
reduces its name's unresolved occurrences; two lexical repairs undo the `modname::` rewrites the
assist writes into calls it leaves behind; an `extract_method` whose range returns from the
enclosing function is refused before the assist runs; an `extract_method` carries the
function-local `use` items its range needs into the new function; and an `extract_variable` asks
the server about a position that can answer, binds a borrowed place as the borrow, and never waits
on the server without a bound. See
[docs/assist-output-repairs.md](docs/assist-output-repairs.md) and
[docs/readiness-and-gates.md](docs/readiness-and-gates.md).

Several `extract_method`s in one function compose only when the plan orders them **bottom-up**, last
range first, so no anchor is ever translated through another extraction's edit. The engine does not
re-anchor them: that would mean re-deriving each later anchor from the produced text.
