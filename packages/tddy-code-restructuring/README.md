# tddy-code-restructuring

Replays a JSONL plan of named Rust refactoring intents via rust-analyzer through `tddy-lsp`.

**Feature doc:** [docs/ft/coder/rust-code-restructuring.md](../../docs/ft/coder/rust-code-restructuring.md)

## CLI

Exposed via `tddy-tools restructure`:

- `apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]`
- `status <plan.jsonl>`
- `check <plan.jsonl> [--deep] [--budget LINES]` — `--deep` also reports the blast radius of every cross-crate move; `--budget` reports the files the plan names that are longer than LINES, as a record rather than a gate
- `anchors <file.rs> --items A,B,C`
- `verify --against <git-ref>`

A run waits until the server is ready or until its caller stops waiting; there is no budget flag.

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

`tddy-index-daemon` is that host. See
[warm-code-intelligence-daemon.md](../../docs/ft/coder/warm-code-intelligence-daemon.md).

## Operations (v1)

`extract_method`, `extract_variable`, `rename_symbol`, `extract_module` (`reexport`, `to_file`),
`extract_module_to_file`, `extract_trait`, `inline_method`, `move_module_to_crate` (`to`,
`reexport`), `move_cluster_to_crate` (`also`, `to`, `reexport`), `move_test_binary_to_crate` (`to`).

`move_cluster_to_crate` moves a **set** of modules as one unit — `anchor` is the first member and
`also` names the rest — in a single edit, so the tree is never half-moved. That is what makes a
mutually-referencing group movable at all: moved one at a time, each module's reference to a sibling
still in the origin would make the destination depend on the crate it left, and no ordering of
one-module operations can resolve a cycle.

`move_test_binary_to_crate` moves `<crate>/tests/<name>.rs` to the crate it exercises. A test binary
is a different shape from a module — cargo auto-discovers it, so there is no `mod` line to remove;
nothing can reference it, so `reexport` is refused; and the destination gains
`[dev-dependencies]`, not `[dependencies]`. See
[docs/test-binary-moves.md](docs/test-binary-moves.md).

Run state is keyed by the **plan**, at `<root>/.restructure/<plan stem>-<digest>/`, so one plan
follows another under the same root without hand-archiving and `--resume` resumes the plan it was
given.

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
