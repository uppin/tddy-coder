# tddy-code-restructuring

Replays a JSONL plan of named Rust refactoring intents via rust-analyzer through `tddy-lsp`.

**Feature doc:** [docs/ft/coder/rust-code-restructuring.md](../../docs/ft/coder/rust-code-restructuring.md)

## CLI

Exposed via `tddy-tools restructure`:

- `apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N] [--indexing-budget SECONDS]`
- `status <plan.jsonl>`
- `check <plan.jsonl> [--deep] [--budget LINES] [--indexing-budget SECONDS]` — `--deep` also reports the blast radius of every cross-crate move; `--budget` reports the files the plan names that are longer than LINES, as a record rather than a gate
- `anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]`
- `verify --against <git-ref>`

Plans hold intents only — no source text (`text` / `code` / `content` refused). Unsupported operations are hard errors.

## Operations (v1)

`extract_method`, `extract_variable`, `rename_symbol`, `extract_module` (`reexport`, `to_file`), `extract_module_to_file`, `extract_trait`, `inline_method`, `move_module_to_crate` (`to`, `reexport`).

## Authored transformations

Most operations delegate to a rust-analyzer assist; three are written here, because no assist
performs them: `extract_class`, the facade `use` line, and `move_module_to_crate`.

rust-analyzer has no cross-crate move, so `move_module_to_crate` has nothing to delegate to. What
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
