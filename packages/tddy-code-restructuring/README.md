# tddy-code-restructuring

Replays a JSONL plan of named Rust refactoring intents via rust-analyzer through `tddy-lsp`.

**Feature doc:** [docs/ft/coder/rust-code-restructuring.md](../../docs/ft/coder/rust-code-restructuring.md)

## CLI

Exposed via `tddy-tools restructure`:

- `apply <plan.jsonl> [--dry-run] [--resume] [--from N|ID] [--stop-after N]`
- `load <plan.jsonl>...`, `unload <plan.jsonl>... | --all`, `plans` — hold plans in the index daemon's plan store (they need the daemon)
- `status <plan.jsonl>`
- `check <plan.jsonl> [--deep] [--budget LINES]` — `--deep` also reports the blast radius of every cross-crate move; `--budget` reports the files the plan names that have more than LINES **production lines** (before the first `#[cfg(test)]` that opens a `mod`), as a record rather than a gate
- `snapshot <plan.jsonl>` — rewrites the plan's header; for a plan of item anchors it also re-resolves them against the current tree
- `anchors <file.rs> --items A,B,C | --at L:C[-L:C]` — emits the anchor a plan carries; `--items` takes bare names, `krate::module::Alpha`, and `<Type>` / `<Type>#N` for an inherent `impl` block (`item_anchor::parse_item_list` is the one rule for every front end)
- `verify --against <git-ref>` — compares logical statements, and excuses and counts what an `extract_module` always causes

A run waits until the server is ready or until its caller stops waiting; there is no budget flag.
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

## Operations (v1)

`extract_method`, `extract_variable`, `rename_symbol`, `extract_module` (`reexport`, `to_file`),
`extract_module_to_file`, `extract_trait`, `inline_method`, `remove_unused_param` (`name`: the parameter),
`convert_tuple_return_to_struct` (`name`: the new struct), `move_module_to_crate` (`to`,
`reexport`), `move_cluster_to_crate` (`also`, `to`, `reexport`), `move_test_binary_to_crate` (`to`).

`remove_unused_param` and `convert_tuple_return_to_struct` rewrite every caller as well as the declaration,
through rust-analyzer's own assists; see [docs/signature-assists.md](docs/signature-assists.md).

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

## Where the code lives

`src/` is organised by what a file decides, and no file goes over the 500 production-line budget
except `backends/rust.rs` and `crate_move/test_binary.rs` (see their records in `docs/code-issues/`).

| Area | Modules |
|---|---|
| Plan vocabulary | `plan.rs`, `plan/codec.rs` (header codec, `hint_of`), `plan/item_path.rs` |
| Journal | `journal.rs`, `journal/group.rs` (`PreImage`, `OpenGroup`) |
| Plan store | `plan_store.rs`, `plan_store/refresh.rs`, `plan_store/live.rs`, `plan_store/live/fold.rs` |
| Runner | `runner/entry_points.rs` with `anchor_entry_points.rs`, `check_entry_points.rs`, `store_run.rs` (and `store_run/applied_op_record.rs`); `runner/group_gate.rs`; `runner/tidy.rs` with `tidy/{diagnostics,gating,format}.rs`; `runner/{budget,comparison,compile_gate,options,outcome,rehearsal,resume}.rs` |
| Verify | `verify.rs`, `verify/statements.rs`, `verify/tokens.rs` |
| Rust backend | `backends/rust.rs`, and beside it `line_diff`, `placeholder_checks`, `lsp_edits`, `import_text`, `module_text`, `visibility`, `seam_survey`, `facade`, `server_process`, `prelude_shadow`, `relative_visibility`, `inline_paths`, `imports`, `early_return`, `chatter` |
| Cross-crate moves | `crate_move/{moving,cluster,source_scan}.rs` with `moving/facade_writer.rs`, `cluster/stranded.rs`, `source_scan/{module_items,sighting_walk}.rs`; `crate_move/test_binary.rs` |

Rust-analyzer's progress is throttled per token to one line every two seconds in the printed stream
(`ServerChatter`'s default); a host that serves structured events builds `ServerChatter::unthrottled()`
so every phase reaches its clients. The scanners that mask strings and comments handle non-ASCII
identifiers.

## Authored transformations

Most operations delegate to a rust-analyzer assist; three are written here, because no assist
performs them: `extract_class`, the facade `use` line, and the cross-crate moves.

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
