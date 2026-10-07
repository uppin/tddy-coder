# 2026-10-07 — `repoint_facade_imports` names a file's paths by the crate that defines them

**Type:** Feature

A new Rust operation, `repoint_facade_imports`, names every path of one file — or of every file of one
module — that goes through a `pub use` of **another crate** inside the file's own crate by the path
where the item is **defined**, in `use` items at any depth (splitting a grouped `use` whose members
need different qualifiers) and in bodies. Comments, strings and the crate's own paths are untouched.
`check --deep` lists the paths it would rewrite. Behaviour: [repoint-facade.md](../repoint-facade.md).

## What changed

- **Plan surface** — `RefactorKind::RepointFacadeImports` (`plan/refactor_kind.rs`); the refusals in
  `plan/codec/facade_imports_fields.rs` (new), wired through `plan/codec.rs`; `SUPPORTED` 24 → 25
  (`backends/rust.rs`). The line carries only an anchor: **no `RefactorOp` field is added**, so no
  struct literal is edited.
- **The operation** — `backends/rust/repoint_facade.rs` and its children: `scope.rs` (which files an
  anchor names), `rewrite.rs` (which surveyed paths go through a facade of another crate, and the four
  preconditions), `group.rs` (Rule P in place and Rule S by splitting, over one `use` statement) and
  `refusals.rs`. **Text-only**: the path survey resolves, and no server is asked anything — the
  `resolve` arm sits above `self.start(…)`.
- **Deep-check notes** — `Rehearsed.notes` (new field), filled from `Resolution.notes` and printed by
  `check --deep` through `console::note` (`runner/rehearsal.rs`, `check_entry_points.rs`). Every
  operation's notes now reach the deep check, not only this operation's.
- **Visibility only** — `crate_move.rs` widens `mod header` and `mod manifest_edits` to `pub(crate)`
  (`mod survey`/`mod reexports` are `move-fidelity`'s); `header::written_prefix`,
  `item_move/text::{use_statements, split_use}` and `sites::members_of` widen the same way.
- **Tests** — `tests/repoint_facade_imports_acceptance.rs` (library, `fake_lsp`-backed, no
  rust-analyzer), `tests/repoint_facade_imports_live_acceptance.rs` (live rust-analyzer, `cargo check`
  as the assertion; registered in `.config/nextest.toml` and `.config/rust-e2e.filterset`) and
  `tests/verify_accounts_for_facade_re_points.rs` (library: pins that `verify` already accounts for the
  result, with no declaration).

## Code issues

| Record | Measurement |
|---|---|
| `oversized-file-backends-rust.md` | **Grown, not closed**: wiring only (the `mod repoint_facade;` line, the `RepointFacadeImports` `SUPPORTED` entry, one `check` arm, one `resolve` arm — ~11 lines); **2,997 → 3,008 production lines**. All logic is in `backends/rust/repoint_facade/` and its children. History row appended; record kept — the split is deferred because every `#sharpen` node edits this file, and the impl-member seams remain |
| `dead-code-plan-filehint-modified.md` | **Unchanged — not touched**: this PR adds no `RefactorOp` field and edits neither `plan.rs` nor `plan/codec/file_hint.rs`; `FileHint::modified` still has one write site (`file_hint.rs:8,13`) and no reader outside tests |
| `complexity-rust-facade-lines.md`, `broken-restructure-anchors-empty-outline.md`, `oversized-file-test-binary.md` | untouched by this change |

## Backlog

Resolved and deleted:
`2026-10-05-restructure-cannot-re-point-an-import-through-a-facade-to-its-defining-crate` — the
operation, its grouped-`use` split and the `check --deep` list are exactly what that entry asked for
(it had landed on `master` through #532).

Kept: `2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node` — this node does not
resolve it; `rust.rs` is still over budget.

## Stack

Node 8 of 8 of the `#sharpen` stack, and the last. Depends on `tidy-engine-files` (the
`plan/refactor_kind.rs` home of `RefactorKind`) and on `move-fidelity` (the resolver's `pub(crate)`
module visibility). Lands after `repoint-call` ([#594](https://github.com/uppin/tddy-coder/pull/594))
on the line, and consumes nothing from it. No dependents.
