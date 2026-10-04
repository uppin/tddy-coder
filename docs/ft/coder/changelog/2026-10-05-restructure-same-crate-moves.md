# 2026-10-05 — Restructure plans move items and modules inside a crate

Feature: [Rust code restructuring](../rust-code-restructuring.md#same-crate-moves) and
[Warm code-intelligence daemon](../warm-code-intelligence-daemon.md#running-it).

A restructure plan can now say "move these items into that module" and "move this module under that
parent", inside one crate, with the same safety net as the other operations: the compile gate, `verify`,
comments kept byte for byte, and callers re-pointed from the language server's own reference set.

- **`move_item`** moves a contiguous run of module-level items, from any file, into an existing module of
  the same crate. A line that carries `name` creates the destination first: `to` is then its parent, and a
  new empty module `name` is declared there, so a topic module can be gathered from items in several files
  in one plan.
- **`reparent_module`** moves a module's file, and the directory of its children, under another parent
  of the same crate. The `mod` declaration travels with its attributes and visibility, every path that
  named the module follows, and `super::` paths in the moved files are rebased.
- **`reexport`** says what happens to the callers: `glob` and `named` leave a `pub use` and edit no
  caller; `none` (or absent) re-points every caller; **`outside`** re-points the callers in the crate and
  leaves a facade only for what another package, or the package's own `tests/`, `examples/`, `benches/`
  and `src/bin`, reaches. A module move has no `named` facade and refuses it.
- Visibility is widened only as far as callers need, each widening reported. Refusals the text answers
  (a missing destination, a name already declared, the item's own module, a destination inside the moved
  module, `#[path]`) are reported by `check` without starting an index, and `check --deep` accepts a
  well-formed item-anchored plan.
- **`restructure warm`** loads the tree's crate graph into the index daemon and reports progress until it
  is queryable. **`./run-index-daemon`** now runs it for its checkout, so the first request no longer waits
  silently for the load; `--no-warm` skips it.
- **`restructure snapshot` of a plan with item anchors** is answered by the warm daemon (a new `Snapshot`
  RPC) when `TDDY_INDEX_SOCKET` is set, instead of starting a rust-analyzer of its own. A plan with no item
  anchors is still answered in process.
- **`restructure anchors` with a path relative to a package** names the repo-root path to write instead of
  reporting "no package"; it never resolves the path for the author.

Known limits are listed in the feature's
[Known limitations](../rust-code-restructuring.md#known-limitations): fields and `impl` members split by a
move are not widened, `reparent_module` does not widen what the moved tree reaches, and `outside` reads the
default target layout only.

Package entries:
[`tddy-code-restructuring`](../../../../packages/tddy-code-restructuring/docs/changesets/2026-10-05-same-crate-moves.md),
[`tddy-index-daemon`](../../../../packages/tddy-index-daemon/docs/changesets/2026-10-05-snapshot-rpc-and-warming.md),
[`tddy-tools`](../../../../packages/tddy-tools/docs/changesets/2026-10-05-restructure-warm-and-snapshot-routing.md).
